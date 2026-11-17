//! 任务栏图标：按 SAP 系统标识上色、客户端号居中。
//!
//! SAP GUI 的所有会话窗口同属一个进程、一个窗口类，任务栏默认取的是**类图标**，
//! 所以开几个系统都长得一模一样。给窗口单独发 `WM_SETICON` 之后，任务栏按钮
//! 就会按窗口（也就是按系统标识 + 客户端那一组）取图标了。
//!
//! 只在启动时和每次登录后各扫一次：同一组的按钮共用图标，第一个窗口设过就够了，
//! 没必要轮询。

use std::collections::HashMap;
use std::sync::Mutex;

use windows_sys::Win32::Foundation::{HWND, LPARAM};
use windows_sys::Win32::Graphics::Gdi::{CreateBitmap, DeleteObject};
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateIconIndirect, EnumWindows, GetClassNameW, GetWindowTextW, SendMessageW, ICONINFO,
};

/// SAP 会话窗口的类名，所有系统都用这一个。
const SESSION_CLASS: &str = "SAP_FRONTEND_SESSION";
const WM_SETICON: u32 = 0x0080;
const ICON_SMALL: usize = 0;
const ICON_BIG: usize = 1;

/// Windows 11 的任务栏图标在 100% 缩放下是 24px，125% 是 30px、150% 是 36px。
///
/// 尺寸给错会被系统缩放：Windows 找"不小于目标"的最近尺寸再缩小，给的图标比目标
/// 还小就只能放大——那正是数字糊成一团的原因。所以按窗口 DPI 一像素不差地画。
fn taskbar_icon_size(hwnd: HWND) -> u32 {
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    let dpi = if dpi == 0 { 96 } else { dpi };
    ((24 * dpi + 48) / 96).max(16)
}

/// SID → 颜色，与卡片上 SID 标签用的是同一套（角色色）。
/// 具体色值来自用户可编辑的配色配置，见 [`crate::core::colors`]。
fn sid_color(
    sid: &str,
    palette: &crate::core::colors::Palette,
    roles: &HashMap<String, String>,
) -> (u8, u8, u8) {
    // 手动指定的角色优先（键就是系统标识），没设过才按标识推断。
    let role = roles
        .get(sid)
        .and_then(|key| crate::core::role::SystemRole::from_key(key))
        .unwrap_or_else(|| crate::core::role::SystemRole::detect(sid));
    let rgba = crate::core::colors::rgba(&palette.role.of_role(role).fg);
    let channel = |value: f32| (value.clamp(0., 1.) * 255.).round() as u8;
    (channel(rgba.r), channel(rgba.g), channel(rgba.b))
}

/// 已创建的图标句柄，按 `(系统标识, 客户端, 像素尺寸)` 缓存：重复创建会泄漏
/// GDI 对象，而且任务栏在句柄换掉时会闪一下。尺寸要进 key：不同缩放的显示器
/// 需要的图标大小不同。
static ICONS: Mutex<Option<HashMap<(String, String, u32), isize>>> = Mutex::new(None);

/// 扫一遍现有会话窗口，把缺图标的补上。启动时和每次登录后调用即可。
pub fn apply_session_icons(dark: bool, roles: &std::collections::HashMap<String, String>) {
    // 只有 SAP GUI 8.10 且任务栏按「系统标识和客户端」分组时才值得动手：
    // 窗口本来不分组的话，每张图标都一样，画了也没意义。
    if !crate::platform::system::taskbar_grouped_by_system() {
        log::info!(
            "任务栏图标跳过：SAP GUI 的「窗口在任务栏中的分组」不是「按系统标识和客户端」"
        );
        return;
    }

    let sessions = collect_sessions();
    if sessions.is_empty() {
        log::info!("任务栏图标跳过：没有找到 SAP 会话窗口");
        return;
    }

    let mut guard = match ICONS.lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };
    let icons = guard.get_or_insert_with(HashMap::new);
    // 图标底色跟着当前明暗的那套配色走；换主题后缓存要清，否则还是旧色。
    let palette = crate::core::colors::Colors::load().palette(dark).clone();

    let total = sessions.len();
    for (hwnd, sid, client) in sessions {
        let size = taskbar_icon_size(hwnd);
        let small = icon_for(icons, &sid, &client, size, &palette, roles);
        // 大图标给 Alt+Tab、任务视图用，约等于 32px @100% 的等比版本。
        let big = icon_for(icons, &sid, &client, (size * 4).div_ceil(3), &palette, roles);
        unsafe {
            SendMessageW(hwnd, WM_SETICON, ICON_BIG, big as LPARAM);
            SendMessageW(hwnd, WM_SETICON, ICON_SMALL, small as LPARAM);
        }
    }
    log::info!("任务栏图标：已处理 {total} 个会话窗口");
}

/// 取缓存，没有就现画一张。
fn icon_for(
    icons: &mut HashMap<(String, String, u32), isize>,
    sid: &str,
    client: &str,
    size: u32,
    palette: &crate::core::colors::Palette,
    roles: &HashMap<String, String>,
) -> isize {
    let key = (sid.to_string(), client.to_string(), size);
    match icons.get(&key) {
        Some(icon) => *icon,
        None => {
            let icon = build_icon(sid, client, size, palette, roles);
            icons.insert(key, icon);
            icon
        }
    }
}

/// 换配色后必须清缓存：缓存键只有 (SID, 客户端, 尺寸)，颜色不在里面。
pub fn clear_icon_cache() {
    if let Ok(mut guard) = ICONS.lock() {
        if let Some(icons) = guard.as_mut() {
            icons.clear();
        }
    }
}

/// 窗口标题形如 `DS4(2)/100 传输组织器`：系统标识 + 会话号 + 客户端。
fn collect_sessions() -> Vec<(HWND, String, String)> {
    unsafe extern "system" fn callback(hwnd: HWND, lparam: LPARAM) -> i32 {
        let out = &mut *(lparam as *mut Vec<(HWND, String, String)>);

        let mut class = [0u16; 64];
        let class_len = GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32).max(0) as usize;
        if String::from_utf16_lossy(&class[..class_len]) != SESSION_CLASS {
            return 1;
        }

        let mut title = [0u16; 512];
        let title_len = GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as i32).max(0) as usize;
        if let Some((sid, client)) = split_title(&String::from_utf16_lossy(&title[..title_len])) {
            out.push((hwnd, sid, client));
        }
        1
    }

    let mut sessions: Vec<(HWND, String, String)> = Vec::new();
    unsafe {
        EnumWindows(Some(callback), &mut sessions as *mut _ as LPARAM);
    }
    sessions
}

/// `DS4(2)/100 传输组织器` → (`DS4`, `100`)。
fn split_title(title: &str) -> Option<(String, String)> {
    let open = title.find('(')?;
    let sid = title[..open].trim();
    let rest = &title[open + 1..];
    let close = rest.find(')')?;
    let after = rest.get(close + 1..)?.strip_prefix('/')?;
    let client: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
    if sid.is_empty() || client.is_empty() {
        return None;
    }
    Some((sid.to_string(), client))
}

fn build_icon(
    sid: &str,
    client: &str,
    size: u32,
    palette: &crate::core::colors::Palette,
    roles: &HashMap<String, String>,
) -> isize {
    let (r, g, b) = sid_color(sid, palette, roles);
    // 圆角底色走像素绘制（抗锯齿），数字用点阵——小尺寸下的矢量字体笔画会糊。
    let mut pixels = render_background(size, (r, g, b));
    draw_digits(&mut pixels, size, client);

    unsafe {
        // 32bpp 的 DIB 位图，像素顺序是 BGRA（我们已经按这个顺序填好）。
        let color = CreateBitmap(
            size as i32,
            size as i32,
            1,
            32,
            pixels.as_ptr() as *const _,
        );
        // mask 全 0：透明度由 color 的 alpha 通道决定。
        let mask = CreateBitmap(size as i32, size as i32, 1, 1, std::ptr::null());
        let info = ICONINFO {
            fIcon: 1,
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: mask,
            hbmColor: color,
        };
        let icon = CreateIconIndirect(&info);
        DeleteObject(color as _);
        DeleteObject(mask as _);
        icon as isize
    }
}

/// 圆角方块 + 竖向微渐变 + 内侧高光。返回 BGRA（预乘 alpha）。
fn render_background(size: u32, color: (u8, u8, u8)) -> Vec<u8> {
    let mut pixels = vec![0u8; (size * size * 4) as usize];
    let radius = size as f32 * 0.27;

    for y in 0..size {
        for x in 0..size {
            let coverage = rounded_coverage(x as f32, y as f32, size as f32, radius);
            if coverage <= 0.0 {
                continue;
            }
            // 顶部略亮、底部略暗，纯色块在大面积上太死板。
            let t = y as f32 / size as f32;
            let shade = 1.0 + (0.5 - t) * 0.16;
            let mut rgb = [
                (color.0 as f32 * shade).clamp(0.0, 255.0),
                (color.1 as f32 * shade).clamp(0.0, 255.0),
                (color.2 as f32 * shade).clamp(0.0, 255.0),
            ];

            // 内缩一圈的覆盖差 = 边缘：给边缘 1px 提亮，看起来才有厚度。
            let inner = rounded_coverage(x as f32, y as f32, size as f32, radius - 1.2);
            let rim = (coverage - inner).clamp(0.0, 1.0) * 0.30;
            for channel in &mut rgb {
                *channel = *channel * (1.0 - rim) + 255.0 * rim;
            }

            let index = ((y * size + x) * 4) as usize;
            // 预乘 alpha：否则半透明边缘会出现一圈深色描边。
            pixels[index] = (rgb[2] as f32 * coverage).round() as u8;
            pixels[index + 1] = (rgb[1] as f32 * coverage).round() as u8;
            pixels[index + 2] = (rgb[0] as f32 * coverage).round() as u8;
            pixels[index + 3] = (coverage * 255.0).round() as u8;
        }
    }

    pixels
}

/// 三档点阵对应任务栏的三种图标尺寸（24 / 30 / 36px）：数字跟着图标一起变大，
/// 而不是所有尺寸都用同一套小字。像素对齐、不做抗锯齿，边缘永远干净。
const GLYPHS_SMALL: [&str; 10] = [
    "011110110011110011110011110011110011110011110011110011110011110011011110",
    "001100011100001100001100001100001100001100001100001100001100001100011110",
    "011110110011000011000011000110000110001100011000110000110000110000111111",
    "011110110011000011000011001110001110000011000011000011000011110011011110",
    "000110001110010110100110110110110110111111000110000110000110000110000110",
    "111111110000110000110000111110111110000011000011000011000011110011011110",
    "001110011000110000110000111110110011110011110011110011110011110011011110",
    "111111000011000011000110000110001100001100011000011000011000011000011000",
    "011110110011110011110011011110011110110011110011110011110011110011011110",
    "011110110011110011110011011111000011000011000011000011000011011110111100",
];

const GLYPHS_MEDIUM: [&str; 10] = [
    "001111000111111011100111111001111110011111100111111001111110011111100111111001111110011111100111111001111110011111100111111001110111111000111100",
    "000110000011100001111000000110000001100000011000000110000001100000011000000110000001100000011000000110000001100000011000000110000111111001111110",
    "001111000111111011100111111001110000011100000111000011100001110000111000011100001110000011100000111000001110000011100000111000001111111111111111",
    "001111000111111011100111000001110000011100001110001111000011110000001110000001110000011100000111000001111110011111100111111001110111111000111100",
    "000011100001111000111110011101101110011011100110111001101110011011100110111111111111111100000110000001100000011000000110000001100000011000000110",
    "111111111111111111100000111000001110000011111100111111100000011100000111000001110000011100000111000001111110011111100111111001110111111000111100",
    "000111100011111001110000111000001110000011111100111111101110011111100111111001111110011111100111111001111110011111100111111001110111111000111100",
    "111111111111111100000111000001110000111000001110000111000001110000111000001110000111000001110000011100000111000001110000011100000111000001110000",
    "001111000111111011100111111001111110011111100111011111100011110001111110111001111110011111100111111001111110011111100111111001110111111000111100",
    "001111000111111011100111111001111110011111100111111001110111111100111111000001110000011100000111000001110000111011111100111110000111000000110000",
];

/// 36px 档：10×20，数字再大一档。
const GLYPHS_LARGE: [&str; 10] = [
    "00111111000111111110111000011111100001111110000111111000011111100001111110000111111000011111100001111110000111111000011111100001111110000111111000011111100001111110000111111000011101111111100011111100",
    "00001110000001111000001111100001101110000000111000000011100000001110000000111000000011100000001110000000111000000011100000001110000000111000000011100000001110000000111000000011100001111111100111111110",
    "00111111000111111110111000011111100001110000000111000000011100000011100000011110000011110000011110000011110000011110000011110000001110000000111000000011100000001110000000111000000011111111111111111111",
    "00111111000111111110111000011100000001110000000111000000111000011111000001111100000000111000000001110000000111000000011100000001110000000111111000011111100001111110000111111000011101111111100011111100",
    "00000111000000111100000111110000111011000111001100111000110011100011001110001100111000110011100011001111111111111111111100000011000000001100000000110000000011000000001100000000110000000011000000001100",
    "11111111111111111111111000000011100000001110000000111111110011111111100000000111000000011100000001110000000111000000011100000001110000000111111000011111100001111110000111111000011101111111100011111100",
    "00011111000011111100011100000011100000001110000000111111110011111111101110000111111000011111100001111110000111111000011111100001111110000111111000011111100001111110000111111000011101111111100011111100",
    "11111111111111111111000000011100000001110000001110000000111000000111000000011100000011100000001110000001110000000111000000111000000011100000001110000000111000000011100000001110000000111000000011100000",
    "00111111000111111110111000011111100001111110000111111000011111100001110111111110001111110001111111101110000111111000011111100001111110000111111000011111100001111110000111111000011101111111100011111100",
    "00111111000111111110111000011111100001111110000111111000011111100001111110000111011111111100111111110000000111000000011100000001110000000111000000011100000011101111111100111111100011111000001110000000",
];

/// 把客户端号画进像素缓冲。放不下就只留底色（至少还能按颜色认系统）。
fn draw_digits(pixels: &mut [u8], size: u32, client: &str) {
    // 三档点阵对应任务栏的三种图标尺寸：24px 用 6×12、30px 用 8×18、36px 用 10×20。
    let (glyphs, width, height) = if size < 28 {
        (&GLYPHS_SMALL, 6u32, 12u32)
    } else if size < 34 {
        (&GLYPHS_MEDIUM, 8u32, 18u32)
    } else {
        (&GLYPHS_LARGE, 10u32, 20u32)
    };
    const GAP: u32 = 1;

    let mut digits: Vec<usize> = client
        .bytes()
        .filter(|byte| byte.is_ascii_digit())
        .map(|byte| (byte - b'0') as usize)
        .collect();
    digits.truncate(3);
    if digits.is_empty() {
        return;
    }

    let total = digits.len() as u32 * width + (digits.len() as u32 - 1) * GAP;
    if total > size || height > size {
        return;
    }
    let origin_x = (size - total) / 2;
    let origin_y = (size - height) / 2;

    for (position, digit) in digits.iter().enumerate() {
        let glyph = glyphs[*digit].as_bytes();
        let base_x = origin_x + position as u32 * (width + GAP);
        for row in 0..height {
            for column in 0..width {
                if glyph[(row * width + column) as usize] != b'1' {
                    continue;
                }
                let x = base_x + column;
                let y = origin_y + row;
                if x >= size || y >= size {
                    continue;
                }
                let index = ((y * size + x) * 4) as usize;
                pixels[index] = 255;
                pixels[index + 1] = 255;
                pixels[index + 2] = 255;
                pixels[index + 3] = 255;
            }
        }
    }
}

/// 圆角矩形的覆盖率，每像素 4×4 超采样，边缘就不会有硬锯齿。
fn rounded_coverage(x: f32, y: f32, size: f32, radius: f32) -> f32 {
    const SAMPLES: u32 = 4;
    let mut hits = 0;
    for sy in 0..SAMPLES {
        for sx in 0..SAMPLES {
            let px = x + (sx as f32 + 0.5) / SAMPLES as f32;
            let py = y + (sy as f32 + 0.5) / SAMPLES as f32;
            if inside_round_rect(px, py, size, radius) {
                hits += 1;
            }
        }
    }
    hits as f32 / (SAMPLES * SAMPLES) as f32
}

fn inside_round_rect(x: f32, y: f32, size: f32, radius: f32) -> bool {
    if x < 0.0 || y < 0.0 || x > size || y > size {
        return false;
    }
    let radius = radius.max(0.0).min(size / 2.0);
    let cx = x.clamp(radius, size - radius);
    let cy = y.clamp(radius, size - radius);
    let dx = x - cx;
    let dy = y - cy;
    dx * dx + dy * dy <= radius * radius
}





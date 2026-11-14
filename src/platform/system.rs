//! 系统信息：Windows 版本、系统语言、SAP GUI 版本。
//!
//! 三个结论都只在启动时问一次，用来决定新用户的默认配置，运行期不再变化。

/// Windows 内部版本号（Win10 19045、Win11 22631…）。
#[cfg(target_os = "windows")]
pub fn windows_build() -> Option<u32> {
    let key = windows_registry::LOCAL_MACHINE
        .open(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion")
        .ok()?;
    key.get_string("CurrentBuildNumber")
        .ok()?
        .trim()
        .parse()
        .ok()
}

/// Mica 是 Win11 才有的合成材质；更早的系统上设了也没有效果。
/// 门槛取 22621（Win11 22H2）：`DWMWA_SYSTEMBACKDROP_TYPE` 从这一版才有，
/// 21H2 上调用不报错但也不生效，界面会变成"半透明却没材质"的脏灰。
#[cfg(target_os = "windows")]
pub fn supports_mica() -> bool {
    windows_build().is_some_and(|build| build >= 22621)
}

#[cfg(not(target_os = "windows"))]
pub fn supports_mica() -> bool {
    false
}

/// 系统界面语言是不是中文：决定「跟随系统」时用哪套文案。
#[cfg(target_os = "windows")]
pub fn prefers_chinese() -> bool {
    let Ok(key) = windows_registry::CURRENT_USER.open(r"Control Panel\International") else {
        return false;
    };
    key.get_string("LocaleName")
        .ok()
        .is_some_and(|name| name.to_lowercase().starts_with("zh"))
}

#[cfg(not(target_os = "windows"))]
pub fn prefers_chinese() -> bool {
    false
}

/// SAP GUI 的「窗口在任务栏中的分组」：`2` = 按系统标识和客户端。
///
/// 只有这一档才会把同系统同客户端的窗口合到一个按钮上；否则每个窗口一个按钮，
/// 各自设图标没有意义。这个键本身也是 8.10 才有的，读不到就当作不支持。
#[cfg(target_os = "windows")]
pub fn taskbar_grouped_by_system() -> bool {
    taskbar_grouping() == Some(2)
}

/// `TaskbarGrouping` 的原始值：`2` = 按系统标识和客户端。读不到就是 `None`。
#[cfg(target_os = "windows")]
pub fn taskbar_grouping() -> Option<u32> {
    let key = windows_registry::CURRENT_USER
        .open(r"Software\SAP\General\Appearance")
        .ok()?;
    key.get_u32("TaskbarGrouping").ok()
}

/// 「组窗口」是 8.10 才加的：更早的 GUI 根本没有这个键，也就谈不上设置。
pub fn supports_taskbar_group() -> bool {
    saplogon_version().is_some_and(|major| major >= 8100)
}

/// 把「组窗口」设成「按系统标识和客户端」。
///
/// 调用前必须确认 SAP GUI 没在运行：GUI 退出时会拿内存里的旧值回写这个键，
/// 运行期间写进去的会在它关掉那一刻被抹掉。
#[cfg(target_os = "windows")]
pub fn set_taskbar_grouping_grouped() -> std::io::Result<()> {
    let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
    let (key, _) = hkcu.create_subkey(r"Software\SAP\General\Appearance")?;
    key.set_value("TaskbarGrouping", &2u32)?;
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn set_taskbar_grouping_grouped() -> std::io::Result<()> {
    Ok(())
}

/// SAP GUI 8.10 默认把窗口标题里的系统信息关着，标题只剩事务名，
/// 任务栏图标因此拿不到 client。
#[cfg(target_os = "windows")]
pub fn additional_title_info_enabled() -> bool {
    windows_registry::CURRENT_USER
        .open(r"Software\SAP\SAPGUI Front\SAP Frontend Server\Administration")
        .ok()
        .and_then(|key| key.get_u32("ShowAdditionalTitleInfo").ok())
        .is_some_and(|value| value == 1)
}

#[cfg(not(target_os = "windows"))]
pub fn additional_title_info_enabled() -> bool {
    false
}

/// 打开窗口标题里的系统/客户端信息（SAP Note 757964）。
#[cfg(target_os = "windows")]
pub fn enable_additional_title_info() -> std::io::Result<()> {
    let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
    let (key, _) =
        hkcu.create_subkey(r"Software\SAP\SAPGUI Front\SAP Frontend Server\Administration")?;
    key.set_value("ShowAdditionalTitleInfo", &1u32)?;
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn enable_additional_title_info() -> std::io::Result<()> {
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn taskbar_grouped_by_system() -> bool {
    false
}

/// SAP GUI 8.10 起才认识 `-uuid`，更早的版本会把它当普通参数忽略掉。
pub fn supports_uuid_mode() -> bool {
    saplogon_version().is_some_and(|major| major >= 8100)
}

/// 从 `saplogon.exe` 的版本资源取主版本号：8.10 读作 8100。
#[cfg(target_os = "windows")]
fn saplogon_version() -> Option<u32> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
    };

    let exe = crate::services::shcut::find_sapshcut("")?.with_file_name("saplogon.exe");
    if !exe.is_file() {
        return None;
    }
    let wide: Vec<u16> = exe
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let root: Vec<u16> = "\\".encode_utf16().chain(std::iter::once(0)).collect();

    unsafe {
        let size = GetFileVersionInfoSizeW(wide.as_ptr(), std::ptr::null_mut());
        if size == 0 {
            return None;
        }
        let mut buffer = vec![0u8; size as usize];
        if GetFileVersionInfoW(wide.as_ptr(), 0, size, buffer.as_mut_ptr().cast()) == 0 {
            return None;
        }
        let mut data: *mut std::ffi::c_void = std::ptr::null_mut();
        let mut length = 0u32;
        if VerQueryValueW(
            buffer.as_ptr().cast(),
            root.as_ptr(),
            &mut data,
            &mut length,
        ) == 0
            || data.is_null()
        {
            return None;
        }
        // VS_FIXEDFILEINFO：签名、结构版本、dwFileVersionMS、dwFileVersionLS…
        let info = std::slice::from_raw_parts(data as *const u32, 4);
        Some(info[2] >> 16)
    }
}

#[cfg(not(target_os = "windows"))]
fn saplogon_version() -> Option<u32> {
    None
}

/// 系统代理，`None` = 没有启用或没配。
///
/// 公司网络里 GitHub 基本只能从代理出去；`ureq` 只认显式给定的代理，不会去读
/// 「Internet 选项」，所以这里自己把系统设置读出来交给它。
///
/// `ProxyServer` 有两种写法：`host:port`，以及按协议分开的
/// `http=h:1;https=h:2`。HTTPS 目标走的是 CONNECT，用的是 http 那一项。
#[cfg(target_os = "windows")]
pub fn system_proxy() -> Option<String> {
    let key = windows_registry::CURRENT_USER
        .open(r"Software\Microsoft\Windows\CurrentVersion\Internet Settings")
        .ok()?;
    if key.get_u32("ProxyEnable").ok()? != 1 {
        return None;
    }
    let server = key.get_string("ProxyServer").ok()?;

    let server = server.trim();
    // 按协议的写法里找 `http=` 那一项；顺序不固定，不能只看第一个键值对。
    let host = server
        .split(';')
        .filter_map(|part| part.split_once('='))
        .find(|(name, _)| name.trim().eq_ignore_ascii_case("http"))
        .map(|(_, value)| value.trim())
        .or_else(|| {
            server
                .split(';')
                .map(str::trim)
                .find(|part| !part.is_empty() && !part.contains('='))
        })?;
    if host.is_empty() {
        return None;
    }
    if host.contains("://") {
        Some(host.to_string())
    } else {
        Some(format!("http://{host}"))
    }
}

#[cfg(not(target_os = "windows"))]
pub fn system_proxy() -> Option<String> {
    None
}

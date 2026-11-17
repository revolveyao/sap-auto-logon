//! 应用自有配色：卡片徽标、角色、卡片自身、窗口底色，浅色/深色各一套。
//!
//! 这些不属于 gpui-kit 的主题——主题管的是面板、控件、菜单这些通用语义色；
//! 这里管的是「卡片墙」自己的外观。单独存一份 JSON：改坏了删掉就回到默认。

use serde::{Deserialize, Serialize};

use crate::core::paths;

/// 一组徽标色：底色 + 文字色。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TagColor {
    pub bg: String,
    pub fg: String,
}

impl Default for TagColor {
    fn default() -> Self {
        Self {
            bg: "#e5e5e5".into(),
            fg: "#171717".into(),
        }
    }
}

impl TagColor {
    fn new(bg: u32, fg: u32) -> Self {
        Self {
            bg: hex_of(bg),
            fg: hex_of(fg),
        }
    }
}

/// 系统角色色：系统标识徽标用它，任务栏图标取其中的文字色当底色。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RoleColors {
    pub dev: TagColor,
    pub qas: TagColor,
    pub prd: TagColor,
    pub other: TagColor,
}

impl Default for RoleColors {
    fn default() -> Self {
        // 角色色单独给一组：比循环色更"实"（底色更暗、字色更深）。
        // 直接复用循环色的话四个底色都在 0.95 明度附近，色相不同但看着一样糊。
        Self {
            dev: TagColor::new(0xDFE9FB, 0x1B4F8F),
            qas: TagColor::new(0xF6E7C6, 0x7A5210),
            prd: TagColor::new(0xF7D9DE, 0x8E2432),
            other: TagColor::new(0xD6ECEE, 0x0F5B5B),
        }
    }
}

impl RoleColors {
    /// 按角色取色。
    pub fn of_role(&self, role: crate::core::role::SystemRole) -> &TagColor {
        use crate::core::role::SystemRole;
        match role {
            SystemRole::Dev => &self.dev,
            SystemRole::Qas => &self.qas,
            SystemRole::Prd => &self.prd,
            SystemRole::Other => &self.other,
        }
    }

    pub fn all(&self) -> [&TagColor; 4] {
        [&self.dev, &self.qas, &self.prd, &self.other]
    }
}

/// 卡片自身的颜色。默认值取自 gpui-kit 默认主题在当前界面上呈现的样子，
/// 接管后换成用户主题也不会变样。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CardColors {
    pub background: String,
    pub border: String,
    /// 悬停时叠在底色上的那一层。
    pub hover: String,
    pub selected_background: String,
    pub selected_border: String,
    pub foreground: String,
    pub muted_foreground: String,
}

impl CardColors {
    fn light() -> Self {
        Self {
            background: hex_of(0xFFFFFF),
            border: hex_of(0xE5E5E5),
            hover: hex_of(0xF5F5F5),
            selected_background: hex_of(0x55A0FC),
            selected_border: hex_of(0x06B6D4),
            foreground: hex_of(0x0A0A0A),
            muted_foreground: hex_of(0x737373),
        }
    }

    fn dark() -> Self {
        Self {
            background: hex_of(0x2A2A2C),
            border: hex_of(0x262626),
            hover: hex_of(0x262626),
            selected_background: hex_of(0x1D4ED8),
            selected_border: hex_of(0x22D3EE),
            foreground: hex_of(0xFAFAFA),
            muted_foreground: hex_of(0xA3A3A3),
        }
    }
}

impl Default for CardColors {
    fn default() -> Self {
        Self::light()
    }
}

/// 一个明暗模式下的整套自有配色。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Palette {
    /// 客户端号按字典序排序后循环取用，固定 6 组。
    pub cycle: Vec<TagColor>,
    pub role: RoleColors,
    pub card: CardColors,
    /// 不透明材质档铺的整窗底色。
    pub shell: String,
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            cycle: default_light_cycle(),
            role: RoleColors::default(),
            card: CardColors::light(),
            shell: hex_of(0xFAFAFA),
        }
    }
}

impl Palette {
    fn dark() -> Self {
        let cycle = default_dark_cycle();
        Self {
            role: RoleColors {
                dev: TagColor::new(0x24405F, 0xA8C7F0),
                qas: TagColor::new(0x685722, 0xE0C080),
                prd: TagColor::new(0x642133, 0xE8A0A8),
                other: TagColor::new(0x25505A, 0x90D0D0),
            },
            cycle,
            card: CardColors::dark(),
            shell: hex_of(0x161618),
        }
    }

    /// 第 `index` 组循环色，越界取模——与原来的 6 色循环一致。
    pub fn cycle_at(&self, index: usize) -> &TagColor {
        let len = self.cycle.len();
        if len == 0 {
            return &EMPTY_TAG;
        }
        &self.cycle[index % len]
    }

    /// 解析成可直接画图的颜色，供 Card 组件使用。
    pub fn card_palette(&self) -> CardPalette {
        CardPalette {
            background: rgba(&self.card.background),
            border: rgba(&self.card.border),
            hover: rgba(&self.card.hover),
            selected_background: rgba(&self.card.selected_background),
            selected_border: rgba(&self.card.selected_border),
            foreground: rgba(&self.card.foreground),
            muted_foreground: rgba(&self.card.muted_foreground),
        }
    }
}

/// 已解析的卡片配色。
#[derive(Debug, Clone, Copy)]
pub struct CardPalette {
    pub background: gpui_kit::Rgba,
    pub border: gpui_kit::Rgba,
    pub hover: gpui_kit::Rgba,
    pub selected_background: gpui_kit::Rgba,
    pub selected_border: gpui_kit::Rgba,
    pub foreground: gpui_kit::Rgba,
    pub muted_foreground: gpui_kit::Rgba,
}

static EMPTY_TAG: TagColor = TagColor {
    bg: String::new(),
    fg: String::new(),
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Colors {
    pub light: Palette,
    pub dark: Palette,
    /// 这套主题的派生基色（hex）：切回来时派生行还是同一个起点。
    pub derive_base: Option<String>,
}

impl Default for Colors {
    fn default() -> Self {
        Self {
            light: Palette::default(),
            dark: Palette::dark(),
            derive_base: None,
        }
    }
}

impl Colors {
    /// 当前模式对应的那一套。
    pub fn palette(&self, dark: bool) -> &Palette {
        if dark { &self.dark } else { &self.light }
    }

}

/// 预置主题的源色。取自重设计体系（参考 `sap-theme-studio`），
/// 再由 [`palette_from_sources`] 映射到本应用的 28 个色槽。
pub struct Sources {
    pub background: &'static str,
    pub base: &'static str,
    pub text: &'static str,
    pub field: &'static str,
    pub accent: &'static str,
    pub focus: &'static str,
    pub secondary: &'static str,
    pub info: &'static str,
    pub positive: &'static str,
    pub critical: &'static str,
    pub negative: &'static str,
}

/// 预置主题：首启种入 `themes\`，之后与用户自建主题一视同仁（可改可删）。
pub struct BuiltinPreset {
    /// 固定 id：跨机器稳定，导入同名主题时据此覆盖。
    pub id: &'static str,
    pub name: &'static str,
    pub sources: Sources,
}

pub const BUILTIN_DEFAULT_ID: &str = "builtin-default";

pub const PRESETS: &[BuiltinPreset] = &[
    BuiltinPreset {
        id: "preset-night-wind",
        name: "夜风",
        sources: Sources {
            background: "#eef8fa",
            base: "#ffffff",
            text: "#0a0a0a",
            field: "#5f6b7a",
            accent: "#00bbdb",
            focus: "#00b7d7",
            secondary: "#ed4b9b",
            info: "#00a4e8",
            positive: "#558316",
            critical: "#f87103",
            negative: "#cc0012",
        },
    },
    BuiltinPreset {
        id: "preset-icecream",
        name: "冰淇淋",
        sources: Sources {
            background: "#fffcfc",
            base: "#ffffff",
            text: "#0a0a0a",
            field: "#5f6b7a",
            accent: "#ffced3",
            focus: "#ffced3",
            secondary: "#ed4b9b",
            info: "#00a4e8",
            positive: "#558316",
            critical: "#f87103",
            negative: "#cc0012",
        },
    },
    BuiltinPreset {
        id: "preset-soft-orange",
        name: "柔橙",
        sources: Sources {
            background: "#fff4ef",
            base: "#ffffff",
            text: "#172554",
            field: "#5f6b7a",
            accent: "#f95e16",
            focus: "#f95e16",
            secondary: "#ed4b9b",
            info: "#00a4e8",
            positive: "#558316",
            critical: "#f87103",
            negative: "#cc0012",
        },
    },
    BuiltinPreset {
        id: "preset-warm-sand",
        name: "暖沙",
        sources: Sources {
            background: "#f6f2ea",
            base: "#fffdf9",
            text: "#33261a",
            field: "#584d42",
            accent: "#b45309",
            focus: "#92400e",
            secondary: "#8c6a3f",
            info: "#0070f2",
            positive: "#17804e",
            critical: "#f0b429",
            negative: "#af251d",
        },
    },
];

/// 源色 → 一套浅深配色。
///
/// 浅色直接用源色；深色按主色色相派生，保证同一主题的浅深是配套的。
pub fn palette_from_sources(sources: &Sources) -> Colors {
    let accent = readable_accent(sources.accent);
    let cycle_hues = [
        hue_of(&accent),
        hue_of(sources.secondary),
        hue_of(sources.info),
        hue_of(sources.positive),
        hue_of(sources.critical),
        hue_of(sources.negative),
    ];
    let mut cycle = Vec::with_capacity(6);
    for hue in cycle_hues {
        cycle.push(tag_for(hue, false));
    }
    let roles = [
        hue_of(&accent),
        hue_of(sources.critical),
        hue_of(sources.negative),
        hue_of(sources.info),
    ];

    let light = Palette {
        cycle,
        role: RoleColors {
            dev: tag_for(roles[0], false),
            qas: tag_for(roles[1], false),
            prd: tag_for(roles[2], false),
            other: tag_for(roles[3], false),
        },
        card: CardColors {
            background: sources.base.to_string(),
            border: mix_hex(sources.base, sources.text, 0.12),
            hover: mix_hex(sources.base, sources.text, 0.05),
            selected_background: accent.clone(),
            selected_border: sources.focus.to_string(),
            foreground: sources.text.to_string(),
            muted_foreground: sources.field.to_string(),
        },
        shell: sources.background.to_string(),
    };

    Colors {
        light,
        dark: derive_from_hue(hue_of(&accent)).dark,
        derive_base: Some(accent.clone()),
    }
}

/// 主色太浅时（如冰淇淋的 `#ffced3`）当选中底色几乎看不出来，先压暗再用。
fn readable_accent(hex: &str) -> String {
    if luminance(hex) <= 0.90 {
        return hex.to_string();
    }
    let hue = hue_of(hex);
    hsl_hex(hue, 0.62, 0.62)
}

/// 由一个色相生成一组徽标色：淡底 + 深字（浅色）/ 深底 + 亮字（深色）。
fn tag_for(hue: f32, dark: bool) -> TagColor {
    if dark {
        TagColor {
            bg: hsl_hex(hue, 0.44, 0.26),
            fg: hsl_hex(hue, 0.55, 0.76),
        }
    } else {
        TagColor {
            bg: hsl_hex(hue, 0.55, 0.94),
            fg: hsl_hex(hue, 0.60, 0.34),
        }
    }
}

/// 按色相派生一整套配色（派生主题 / 预置主题的深色半边都用它）。
pub fn derive_from_hue(hue: f32) -> Colors {
    Colors {
        light: derive_palette(hue, false),
        dark: derive_palette(hue, true),
        derive_base: Some(hsl_hex(hue, 0.70, 0.55)),
    }
}

/// 派生时的「个性化」版本：客户端与角色的色相、饱和度带确定性抖动。
///
/// 等距 60° 铺满色环看起来像调色板样本，不像一成套的配色；这里按基色算出
/// 一组伪随机偏移——同一个基色每次都得到同一套（可复现），但相邻色差不再均匀，
/// 观感更自然。
pub fn derive_from_color(hue: f32) -> Colors {
    let seed = hue_seed(hue);
    Colors {
        light: derive_palette_jittered(hue, false, seed),
        dark: derive_palette_jittered(hue, true, seed),
        derive_base: Some(hsl_hex(hue, 0.70, 0.55)),
    }
}

fn hue_seed(hue: f32) -> u32 {
    (hue.rem_euclid(360.0) * 1000.0) as u32
}

/// 确定性伪随机：同一个 seed+index 永远给同一个值，落在 -1..1。
fn wobble(seed: u32, index: u32) -> f32 {
    let mut value = seed
        .wrapping_mul(1_664_525)
        .wrapping_add(index.wrapping_mul(1_013_904_223))
        .wrapping_add(12_345);
    value ^= value >> 13;
    value = value.wrapping_mul(1_274_126_177);
    value ^= value >> 16;
    (value % 2001) as f32 / 1000.0 - 1.0
}

fn derive_palette_jittered(hue: f32, dark: bool, seed: u32) -> Palette {
    let mut cycle = Vec::with_capacity(6);
    for index in 0..6u32 {
        // 57° 步长不会整除 360，六组不会撞在一起；再叠 ±26° 抖动。
        let offset = index as f32 * 57.0 + wobble(seed, index) * 26.0;
        let h = (hue + offset).rem_euclid(360.0);
        let saturation = (0.52 + wobble(seed, index + 100) * 0.10).clamp(0.32, 0.72);
        cycle.push(if dark {
            TagColor {
                bg: hsl_hex(h, saturation * 0.85, 0.26),
                fg: hsl_hex(h, saturation, 0.76),
            }
        } else {
            TagColor {
                bg: hsl_hex(h, saturation, 0.94),
                fg: hsl_hex(h, saturation, 0.34),
            }
        });
    }

    // 角色保留语义：开发跟基色、测试琥珀、生产红、其他青，各自再抖一点。
    let roles = [
        (205.0 + wobble(seed, 200) * 8.0).rem_euclid(360.0),
        (45.0 + wobble(seed, 201) * 10.0).rem_euclid(360.0),
        (355.0 + wobble(seed, 202) * 10.0).rem_euclid(360.0),
        (185.0 + wobble(seed, 203) * 10.0).rem_euclid(360.0),
    ];
    let mut built = Vec::with_capacity(4);
    for (index, h) in roles.into_iter().enumerate() {
        let saturation = (0.5 + wobble(seed, 300 + index as u32) * 0.10).clamp(0.34, 0.7);
        built.push(if dark {
            TagColor {
                bg: hsl_hex(h, saturation * 0.85, 0.24),
                fg: hsl_hex(h, saturation, 0.78),
            }
        } else {
            TagColor {
                bg: hsl_hex(h, saturation, 0.93),
                fg: hsl_hex(h, saturation, 0.35),
            }
        });
    }

    let mut palette = derive_palette(hue, dark);
    palette.cycle = cycle;
    palette.role = RoleColors {
        dev: built[0].clone(),
        qas: built[1].clone(),
        prd: built[2].clone(),
        other: built[3].clone(),
    };
    palette
}

/// 一套命名主题。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NamedTheme {
    /// GUID，判重与文件名的唯一依据。
    pub id: String,
    /// 只是描述，可重名。
    pub name: String,
    /// 内置「默认」为 true：不可改、不可删、不落盘。
    pub builtin: bool,
    pub colors: Colors,
}

impl Default for NamedTheme {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            builtin: false,
            colors: Colors::default(),
        }
    }
}

impl NamedTheme {
    fn builtin_default() -> Self {
        Self {
            id: BUILTIN_DEFAULT_ID.to_string(),
            name: "默认".to_string(),
            builtin: true,
            colors: Colors::default(),
        }
    }
}

/// 主题库：目录里每套主题一个 `<id>.json`，内置「默认」来自代码常量。
pub struct ThemeBook {
    dir: std::path::PathBuf,
    themes: Vec<NamedTheme>,
    current: String,
}

impl ThemeBook {
    pub fn dir() -> std::path::PathBuf {
        paths::config_dir().join("themes")
    }

    /// 扫描目录。`preferred` 是想用的主题 id，找不到就回落到默认。
    pub fn load(preferred: &str) -> Self {
        let dir = Self::dir();
        let mut themes = vec![NamedTheme::builtin_default()];

        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                    continue;
                }
                match std::fs::read_to_string(&path)
                    .map_err(|error| error.to_string())
                    .and_then(|text| {
                        serde_json::from_str::<NamedTheme>(&text)
                            .map_err(|error| error.to_string())
                    }) {
                    Ok(mut theme) => {
                        if theme.id.is_empty() || theme.id == BUILTIN_DEFAULT_ID {
                            continue;
                        }
                        theme.builtin = false;
                        themes.push(theme);
                    }
                    Err(error) => {
                        log::warn!("跳过主题文件 {}：{error}", path.display());
                    }
                }
            }
        }

        themes.sort_by(|a, b| {
            b.builtin
                .cmp(&a.builtin)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });

        let current = if themes.iter().any(|theme| theme.id == preferred) {
            preferred.to_string()
        } else {
            BUILTIN_DEFAULT_ID.to_string()
        };

        Self {
            dir,
            themes,
            current,
        }
    }

    pub fn themes(&self) -> &[NamedTheme] {
        &self.themes
    }

    pub fn current_id(&self) -> &str {
        &self.current
    }

    /// 正在使用的配色。
    pub fn active(&self) -> &Colors {
        self.themes
            .iter()
            .find(|theme| theme.id == self.current)
            .map(|theme| &theme.colors)
            .unwrap_or(&self.themes[0].colors)
    }

    pub fn set_current(&mut self, id: &str) -> bool {
        if self.themes.iter().any(|theme| theme.id == id) {
            self.current = id.to_string();
            true
        } else {
            false
        }
    }

    pub fn find(&self, id: &str) -> Option<&NamedTheme> {
        self.themes.iter().find(|theme| theme.id == id)
    }

    /// 写一套主题。内置「默认」不落盘（它来自代码）。
    pub fn save_theme(&mut self, mut theme: NamedTheme) -> anyhow::Result<()> {
        if theme.builtin || theme.id == BUILTIN_DEFAULT_ID {
            return Ok(());
        }
        if theme.id.is_empty() {
            theme.id = uuid::Uuid::new_v4().to_string();
        }
        std::fs::create_dir_all(&self.dir)?;
        let text = serde_json::to_string_pretty(&theme)?;
        let path = self.dir.join(format!("{}.json", theme.id));
        let temp = path.with_extension("json.tmp");
        std::fs::write(&temp, text)?;
        std::fs::rename(&temp, &path)?;

        match self.themes.iter_mut().find(|item| item.id == theme.id) {
            Some(slot) => *slot = theme,
            None => self.themes.push(theme),
        }
        self.sort();
        Ok(())
    }

    /// 删一套主题。内置主题与「只剩默认」时拒绝。
    pub fn remove_theme(&mut self, id: &str) -> bool {
        let Some(index) = self.themes.iter().position(|theme| theme.id == id) else {
            return false;
        };
        if self.themes[index].builtin {
            return false;
        }
        let path = self.dir.join(format!("{id}.json"));
        let _ = std::fs::remove_file(path);
        self.themes.remove(index);
        if self.current == id {
            self.current = self.themes[0].id.clone();
        }
        true
    }

    /// 生成一个未被占用的名字：主题 2、主题 3…
    pub fn next_name(&self) -> String {
        for index in 2..1000 {
            let name = format!("主题 {index}");
            if !self.themes.iter().any(|theme| theme.name == name) {
                return name;
            }
        }
        "主题".to_string()
    }

    /// 首次启动把预置主题写进目录；已存在（同 id）就跳过，不覆盖用户改动。
    pub fn seed_presets(&mut self) {
        for preset in PRESETS {
            if self.themes.iter().any(|theme| theme.id == preset.id) {
                continue;
            }
            let theme = NamedTheme {
                id: preset.id.to_string(),
                name: preset.name.to_string(),
                builtin: false,
                colors: palette_from_sources(&preset.sources),
            };
            if let Err(error) = self.save_theme(theme) {
                log::warn!("写入预置主题 {} 失败：{error}", preset.name);
            }
        }
    }

    /// 迁移上一版的单文件配色（`colors.json`），转成一套用户主题。
    pub fn migrate_legacy(&mut self) {
        if self.themes.len() > 1 {
            return;
        }
        let legacy = paths::config_dir().join("colors.json");
        let Ok(text) = std::fs::read_to_string(&legacy) else {
            return;
        };
        let Ok(colors) = serde_json::from_str::<Colors>(&text) else {
            return;
        };
        let theme = NamedTheme {
            id: uuid::Uuid::new_v4().to_string(),
            name: "默认配色".to_string(),
            builtin: false,
            colors,
        };
        if self.save_theme(theme).is_ok() {
            let _ = std::fs::remove_file(legacy);
            log::info!("已把旧的 colors.json 迁移成一套主题");
        }
    }

    fn sort(&mut self) {
        self.themes.sort_by(|a, b| {
            b.builtin
                .cmp(&a.builtin)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
    }
}

/// 两个颜色按 `t` 混色（RGB 线性插值）。
fn mix_hex(from: &str, to: &str, t: f32) -> String {
    let (ar, ag, ab) = rgb_of(from);
    let (br, bg, bb) = rgb_of(to);
    let t = t.clamp(0., 1.);
    let blend = |a: f32, b: f32| ((a + (b - a) * t) * 255.).round() as u32;
    hex_of((blend(ar, br) << 16) | (blend(ag, bg) << 8) | blend(ab, bb))
}

fn rgb_of(hex: &str) -> (f32, f32, f32) {
    let value = u32::from_str_radix(hex.trim().trim_start_matches('#'), 16).unwrap_or(0x808080);
    (
        ((value >> 16) & 0xFF) as f32 / 255.,
        ((value >> 8) & 0xFF) as f32 / 255.,
        (value & 0xFF) as f32 / 255.,
    )
}

/// 相对亮度（够用来判断「这个颜色太浅/太深」，不必走完整的 WCAG 公式）。
fn luminance(hex: &str) -> f32 {
    let (r, g, b) = rgb_of(hex);
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// 取色相（度）。
fn hue_of(hex: &str) -> f32 {
    let (r, g, b) = rgb_of(hex);
    hue_from_rgb(r, g, b)
}

/// 由 RGB（0..1）取色相（度）。派生主题时把选色器里的颜色转成色相用。
pub fn hue_from_rgb(r: f32, g: f32, b: f32) -> f32 {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    if delta <= f32::EPSILON {
        return 205.0;
    }
    let hue = if max == r {
        60.0 * (((g - b) / delta) % 6.0)
    } else if max == g {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };
    hue.rem_euclid(360.0)
}

/// 按色相派生的基础实现：浅色与深色各有一套固定的饱和/明度关系。
fn derive_palette(hue: f32, dark: bool) -> Palette {
    let mut cycle = Vec::with_capacity(6);
    for index in 0..6 {
        let h = (hue + index as f32 * 60.0) % 360.0;
        cycle.push(tag_for(h, dark));
    }

    // 角色用固定语义色相：开发跟主色、测试琥珀、生产红、其他青。
    // 之前测试/生产只差 8°，深色下几乎同色。
    let roles = [205.0, 45.0, 355.0, 185.0];
    let dev = tag_for(roles[0] % 360.0, dark);
    let qas = tag_for(roles[1] % 360.0, dark);
    let prd = tag_for(roles[2] % 360.0, dark);
    let other = tag_for(roles[3] % 360.0, dark);

    let card = if dark {
        CardColors {
            background: hsl_hex(hue, 0.06, 0.16),
            border: hsl_hex(hue, 0.05, 0.24),
            hover: hsl_hex(hue, 0.06, 0.22),
            selected_background: hsl_hex(hue, 0.55, 0.32),
            selected_border: hsl_hex(hue, 0.70, 0.62),
            foreground: hsl_hex(hue, 0.04, 0.96),
            muted_foreground: hsl_hex(hue, 0.04, 0.68),
        }
    } else {
        CardColors {
            background: hsl_hex(hue, 0.12, 0.99),
            border: hsl_hex(hue, 0.10, 0.90),
            hover: hsl_hex(hue, 0.10, 0.95),
            selected_background: hsl_hex(hue, 0.72, 0.62),
            selected_border: hsl_hex(hue, 0.65, 0.52),
            foreground: hsl_hex(hue, 0.12, 0.10),
            muted_foreground: hsl_hex(hue, 0.06, 0.45),
        }
    };

    Palette {
        role: RoleColors {
            dev,
            qas,
            prd,
            other,
        },
        cycle,
        card,
        shell: if dark {
            hsl_hex(hue, 0.05, 0.09)
        } else {
            hsl_hex(hue, 0.06, 0.98)
        },
    }
}
fn default_light_cycle() -> Vec<TagColor> {
    vec![
        TagColor::new(0xE3EDFB, 0x1F4E8C),
        TagColor::new(0xE2F2E7, 0x1E6B3A),
        TagColor::new(0xFBF0DC, 0x7A5410),
        TagColor::new(0xF6E4E7, 0x8E2F3C),
        TagColor::new(0xEDE7F8, 0x553388),
        TagColor::new(0xE1F1F1, 0x155F5F),
    ]
}

fn default_dark_cycle() -> Vec<TagColor> {
    // 深色下底色如果只在 0.18 的亮度附近打转，六个色相看起来几乎一样。
    // 这里把亮度提到 0.25 上下、饱和度提到 0.4 以上，让每一组的色相真能看出来。
    vec![
        TagColor::new(0x24405F, 0xA8C7F0),
        TagColor::new(0x254A33, 0xA8DDB8),
        TagColor::new(0x685722, 0xE0C080),
        TagColor::new(0x642133, 0xE8A0A8),
        TagColor::new(0x402B64, 0xC0A8E8),
        TagColor::new(0x25505A, 0x90D0D0),
    ]
}

/// `0xRRGGBB` → `#rrggbb`。
fn hex_of(value: u32) -> String {
    format!("#{:06x}", value)
}

/// HSLA（色相按度）→ `#rrggbb`。色系派生用，不依赖 gpui 的颜色类型。
fn hsl_hex(hue: f32, saturation: f32, lightness: f32) -> String {
    let h = hue.rem_euclid(360.0) / 360.0;
    let s = saturation.clamp(0., 1.);
    let l = lightness.clamp(0., 1.);
    let channel = |n: f32| {
        let k = (n + h * 12.0) % 12.0;
        let a = s * l.min(1.0 - l);
        let value = l - a * ((k - 3.0).min(9.0 - k).min(1.0)).max(-1.0);
        (value.clamp(0., 1.) * 255.).round() as u32
    };
    hex_of((channel(0.) << 16) | (channel(8.) << 8) | channel(4.))
}

/// `#rrggbb` / `rrggbb` → 颜色。解析不了就当灰，界面不会因为一个坏值画不出来。
pub fn rgba(hex: &str) -> gpui_kit::Rgba {
    u32::from_str_radix(hex.trim().trim_start_matches('#'), 16)
        .map(gpui_kit::rgb)
        .unwrap_or_else(|_| gpui_kit::rgb(0x808080))
}

/// 颜色 → `#rrggbb`，供编辑器把选色器里挑的值写回配置。
pub fn to_hex(color: gpui_kit::Hsla) -> String {
    let rgba = gpui_kit::Rgba::from(color);
    let channel = |value: f32| (value.clamp(0., 1.) * 255.).round() as u32;
    format!(
        "#{:02x}{:02x}{:02x}",
        channel(rgba.r),
        channel(rgba.g),
        channel(rgba.b)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsl_hex_matches_known_values() {
        assert_eq!(hsl_hex(0.0, 0.0, 0.0), "#000000");
        assert_eq!(hsl_hex(0.0, 0.0, 1.0), "#ffffff");
        assert_eq!(hsl_hex(0.0, 1.0, 0.5), "#ff0000");
        assert_eq!(hsl_hex(120.0, 1.0, 0.5), "#00ff00");
        assert_eq!(hsl_hex(240.0, 1.0, 0.5), "#0000ff");
    }

    #[test]
    fn presets_map_to_full_palettes() {
        for preset in PRESETS {
            let colors = palette_from_sources(&preset.sources);
            assert_eq!(colors.light.cycle.len(), 6, "{} 客户端色不足", preset.name);
            assert!(!colors.light.card.background.is_empty());
            assert!(!colors.dark.card.background.is_empty());
        }
    }
}

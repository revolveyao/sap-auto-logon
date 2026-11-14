//! 界面配置，存 `%APPDATA%\SapAutoLogon\settings.toml`。
//!
//! 每个字段都带 `#[serde(default)]`：段缺了就是默认值，手工删掉一段不会让
//! 整份配置读不出来。

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::core::paths;

/// 配置版本：需要**一次性**改掉已保存的值时递增。
/// 1：默认窗口宽度改成「刚好一行四张卡」，旧的 `window.bounds` 会让它看起来没生效。
const REVISION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    /// `None` = 升级前写下的配置
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<u32>,
    pub general: General,
    pub behavior: Behavior,
    pub sap: Sap,
    pub window: WindowState,
    /// 快捷方式别名：uuid → 显示名。只存本地，绝不写回 SAP GUI 的配置。
    /// 放顶层而不是 window 段：它不是窗口状态，清窗口位置时不该一起丢。
    #[serde(default)]
    pub aliases: HashMap<String, String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            revision: Some(REVISION),
            general: General::default(),
            behavior: Behavior::default(),
            sap: Sap::default(),
            window: WindowState::default(),
            aliases: HashMap::new(),
        }
    }
}

/// 窗口位置、上次登录的系统与最近登录列表：下次启动直接恢复到上次离开的地方。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowState {
    /// `[x, y, width, height]`，逻辑像素
    pub bounds: Option<[f32; 4]>,
    pub last_service: String,
    /// 最近登录过的快捷方式 uuid，最新的在前
    pub recent: Vec<String>,
    /// 固定在「最近登录」最前面的快捷方式 uuid
    pub pinned: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct General {
    /// `system` / `light` / `dark`
    pub theme: String,
    /// `mica` / `acrylic` / `opaque`
    pub material: String,
    /// 空 = 跟随系统
    pub language: String,
    pub start_minimized: bool,
    pub autostart: bool,
    pub logging: bool,
    pub log_level: String,
    /// 空 = 默认目录
    pub log_dir: String,
    /// 每次启动时检查一次新版本
    pub auto_check_update: bool,
}

impl Default for General {
    fn default() -> Self {
        Self {
            theme: "system".into(),
            // Win11 之前没有 Mica：设成 mica 只会得到一块半透明的脏灰。
            material: if crate::platform::system::supports_mica() {
                "mica"
            } else {
                "opaque"
            }
            .into(),
            language: String::new(),
            start_minimized: false,
            autostart: false,
            logging: true,
            log_level: "error".into(),
            log_dir: String::new(),
            auto_check_update: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Behavior {
    /// `minimize` / `exit`
    pub after_login: String,
    /// `tray` / `exit`
    pub on_close: String,
}

impl Default for Behavior {
    fn default() -> Self {
        Self {
            after_login: "minimize".into(),
            on_close: "tray".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Sap {
    /// `-uuid` 实验模式：把目标系统交给 SAP GUI 自己解析，不拼 guiparm。
    pub use_uuid: bool,
    /// 空 = 自动探测
    pub sapshcut_path: String,
    pub default_language: String,
    /// 上次新建快捷方式时选的基础系统 uuid
    pub last_base_service: String,
}

impl Default for Sap {
    fn default() -> Self {
        Self {
            // 8.10 起 `-uuid` 才被 SAP GUI 接受；老版本按新规则拼会登录失败。
            use_uuid: crate::platform::system::supports_uuid_mode(),
            sapshcut_path: String::new(),
            default_language: "ZH".into(),
            last_base_service: String::new(),
        }
    }
}

impl AppSettings {
    /// 界面语言：空字符串表示跟随系统，只有中英两套文案。
    pub fn locale(&self) -> &'static str {
        match self.general.language.as_str() {
            "en" => "en",
            "zh_CN" | "zh-CN" => "zh-CN",
            _ if crate::platform::system::prefers_chinese() => "zh-CN",
            _ => "en",
        }
    }

    pub fn load() -> Self {
        Self::load_from(&paths::settings_path())
    }

    pub fn load_from(path: &Path) -> Self {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        let mut settings = match toml::from_str::<Self>(&text) {
            Ok(settings) => settings,
            Err(error) => {
                log::warn!("配置解析失败（{}）：{error}，本次用默认值", path.display());
                return Self::default();
            }
        };
        if settings.revision.unwrap_or(0) < REVISION {
            // 旧版本把窗口尺寸一直存着，新默认值会被它盖掉：升级时清一次位置尺寸。
            settings.window.bounds = None;
            settings.revision = Some(REVISION);
        }
        settings
    }

    /// 先写临时文件再改名：直接覆写时一次中断会留下半个 TOML。
    pub fn save(&self) -> anyhow::Result<()> {
        let path = paths::settings_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(self)?;
        let temp = path.with_extension("toml.tmp");
        std::fs::write(&temp, text)?;
        std::fs::rename(&temp, &path)?;
        Ok(())
    }
}

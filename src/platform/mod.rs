//! 平台相关：单实例、托盘、开机自启。

pub mod autostart;
pub mod folder_picker;
pub mod process;
pub mod single_instance;
pub mod system;
#[cfg(target_os = "windows")]
pub mod taskbar;
#[cfg(not(target_os = "windows"))]
pub mod taskbar {
    pub fn apply_session_icons(
        _roles: &std::collections::HashMap<String, String>,
        _palette: &crate::core::colors::Palette,
    ) {}
    pub fn clear_icon_cache() {}
}
pub mod tray;
pub mod window;

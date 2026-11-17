//! 系统托盘：图标、菜单与事件通道。
//!
//! `tray-icon` / `muda` 的事件走 `set_event_handler` 注册的全局回调，回调在托盘
//! 图标自己的窗口过程里同步执行（Windows 上就是主线程的消息泵）。回调里没有
//! `&mut App`，所以它只把事件丢进通道（[`install_event_handlers`]），由 gpui 的
//! 任务在主线程取走、转换成 [`TrayAction`]。

use std::sync::OnceLock;

use rust_i18n::t;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder, TrayIconEvent};

/// 托盘菜单触发的动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    /// 双击图标或点「打开主窗口」
    ShowWindow,
    /// 打开设置
    Settings,
    Quit,
}

/// 托盘上发生的事，由库的回调在消息宿主线程上投递进来。
pub enum TrayEvent {
    Menu(tray_icon::menu::MenuId),
    Icon(TrayIconEvent),
}

/// 事件投递端。`set_event_handler` 是进程级注册、只能设一次，所以发送端放在
/// 全局而不是 [`Tray`] 上 —— 它与具体哪一个托盘实例无关。
static TRAY_EVENTS: OnceLock<async_channel::Sender<TrayEvent>> = OnceLock::new();
static TRAY_INBOX: OnceLock<async_channel::Receiver<TrayEvent>> = OnceLock::new();

/// 注册全局托盘事件回调（进程内只生效一次），返回主线程消费的接收端。
pub fn install_event_handlers() -> async_channel::Receiver<TrayEvent> {
    TRAY_INBOX
        .get_or_init(|| {
            let (tx, rx) = async_channel::unbounded();
            let _ = TRAY_EVENTS.set(tx);
            TrayIconEvent::set_event_handler(Some(|event| {
                if let Some(tx) = TRAY_EVENTS.get() {
                    let _ = tx.try_send(TrayEvent::Icon(event));
                }
            }));
            MenuEvent::set_event_handler(Some(|event: tray_icon::menu::MenuEvent| {
                if let Some(tx) = TRAY_EVENTS.get() {
                    let _ = tx.try_send(TrayEvent::Menu(event.id));
                }
            }));
            rx
        })
        .clone()
}

pub struct Tray {
    _tray: TrayIcon,
    show_id: tray_icon::menu::MenuId,
    settings_id: tray_icon::menu::MenuId,
    quit_id: tray_icon::menu::MenuId,
    // 菜单项必须一直存活：底层 muda 持有它们的引用，丢弃会变成空白项。
    _version: MenuItem,
    _separator: PredefinedMenuItem,
    _show: MenuItem,
    _settings: MenuItem,
    _quit: MenuItem,
}

impl Tray {
    pub fn create() -> Result<Self, String> {
        let menu = Menu::new();
        let version = MenuItem::new(
            format!("SAP Auto Logon v{}", env!("CARGO_PKG_VERSION")),
            false,
            None,
        );
        let separator = PredefinedMenuItem::separator();
        let show = MenuItem::new(t!("app.tray_show").to_string(), true, None);
        let settings = MenuItem::new(t!("app.tray_settings").to_string(), true, None);
        let quit = MenuItem::new(t!("app.tray_quit").to_string(), true, None);

        menu.append_items(&[&version, &separator, &show, &settings, &quit])
            .map_err(|error| format!("构建托盘菜单失败：{error}"))?;

        let tray = TrayIconBuilder::new()
            .with_icon(load_icon()?)
            .with_menu(Box::new(menu))
            .with_tooltip(t!("app.tray_tooltip").to_string())
            .build()
            .map_err(|error| format!("创建托盘图标失败：{error}"))?;

        Ok(Self {
            _tray: tray,
            show_id: show.id().clone(),
            settings_id: settings.id().clone(),
            quit_id: quit.id().clone(),
            _version: version,
            _separator: separator,
            _show: show,
            _settings: settings,
            _quit: quit,
        })
    }

    /// 把一个托盘事件翻译成动作；认不出的返回 `None`。
    pub fn action_for_event(&self, event: &TrayEvent) -> Option<TrayAction> {
        match event {
            TrayEvent::Menu(id) => self.action_for_menu(id),
            TrayEvent::Icon(event) => matches!(event, TrayIconEvent::DoubleClick { .. })
                .then_some(TrayAction::ShowWindow),
        }
    }

    fn action_for_menu(&self, id: &tray_icon::menu::MenuId) -> Option<TrayAction> {
        if *id == self.show_id {
            Some(TrayAction::ShowWindow)
        } else if *id == self.settings_id {
            Some(TrayAction::Settings)
        } else if *id == self.quit_id {
            Some(TrayAction::Quit)
        } else {
            None
        }
    }
}

/// 内嵌的 32x32 PNG 解成 RGBA。
fn load_icon() -> Result<Icon, String> {
    let decoded = image::load_from_memory(include_bytes!("../../assets/tray-32.png"))
        .map_err(|error| format!("解码托盘图标失败：{error}"))?;
    let rgba = decoded.to_rgba8();
    let (width, height) = rgba.dimensions();
    Icon::from_rgba(rgba.into_raw(), width, height)
        .map_err(|error| format!("构建托盘图标失败：{error}"))
}


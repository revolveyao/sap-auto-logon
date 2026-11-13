//! 主视图：卡片网格 + 右侧状态栏，全部操作走右键菜单与弹框。

use std::collections::{HashMap, HashSet};

use gpui_kit::base::InteractiveElementExt as _;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenuItem};
use gpui_kit::component::{Icon, IconName};
use gpui_kit::component::label::Label;
use gpui_kit::component::notification::NotificationType;
use gpui_kit::component::select::{Select, SelectEvent, SelectState};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{ActiveTheme as _, IndexPath, Sizable as _, WindowExt as _};
use rust_i18n::t;
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::core::settings::AppSettings;
use crate::landscape::model::{Landscape, Service, ServiceKind, TreeNode};
use crate::services::shcut::{self, LoginRequest};
use crate::ui::card::Card;

const TITLE_BAR: f32 = 40.;

/// 新建快捷方式的草稿：选中的目标文件夹与基础系统。
#[derive(Clone)]
struct NewShortcut {
    folder_id: String,
    base_id: String,
}

pub struct RootView {
    settings: AppSettings,
    landscape: Option<Landscape>,
    tray: Option<crate::platform::tray::Tray>,
    selected: Option<String>,
    status: String,
    /// 已保存密码的快捷方式 uuid（启动时批量查一次凭据）
    password_ready: HashSet<String>,
    /// 客户端号的 6 色循环表（landscape 变化时重建）
    client_colors: HashMap<String, usize>,
    /// 当前悬停的卡片（卡片的过渡动画靠它驱动）
    hovered_card: Option<String>,
    /// 目录选择的后台结果通道（None = 没有正在进行的挑选）
    folder_picker_rx: Option<std::sync::mpsc::Receiver<Option<String>>>,
    /// 凭据扫描的后台结果通道`r
    cred_rx: Option<std::sync::mpsc::Receiver<Vec<String>>>,
    /// 结果写回哪个输入框：true = 编辑弹框，false = 新建弹框
    folder_picker_edit: bool,
    /// 选中计数：每次选中自增，旧的自动清除定时器据此判断是否过期
    select_generation: u64,
    /// 本次运行里每个快捷方式上次登录的结果：true 成功 / false 失败
    last_result: HashMap<String, bool>,
    /// 正在等待输入密码的快捷方式 uuid；Some 时显示密码弹框
    password_target: Option<String>,
    /// 设置弹框是否打开
    settings_target: bool,
    settings_logdir: Entity<InputState>,
    /// 正在等待确认删除的快捷方式 uuid
    delete_target: Option<String>,
    /// 正在编辑参数的快捷方式 uuid
    editing: Option<String>,
    edit_name: Entity<InputState>,
    edit_client: Entity<InputState>,
    edit_user: Entity<InputState>,
    edit_language: Entity<InputState>,
    edit_workdir: Entity<InputState>,
    /// 新建快捷方式草稿；Some 时显示新建弹框
    creating: Option<NewShortcut>,
    new_name: Entity<InputState>,
    new_client: Entity<InputState>,
    new_user: Entity<InputState>,
    new_language: Entity<InputState>,
    new_workdir: Entity<InputState>,
    new_reuse: Entity<bool>,
    set_autostart: Entity<bool>,
    set_logging: Entity<bool>,
    set_uuid: Entity<bool>,
    new_winmax: bool,
    /// 跟踪级别显示值：无跟踪 / 短 / 中 / 长
    new_trace: String,
    new_trace_select: Entity<SelectState<Vec<String>>>,
    new_winmax_select: Entity<SelectState<Vec<String>>>,
    new_language_select: Entity<SelectState<Vec<String>>>,
    /// 下拉选中的语言代码（LAISO），同步进 new_language 输入框供其余逻辑复用
    new_language_code: String,
    new_folder_select: Entity<SelectState<Vec<String>>>,
    new_base_select: Entity<SelectState<Vec<String>>>,
    /// 显示文本 → uuid，Select 只回传文本，靠它反查目标
    new_folder_map: Vec<(String, String)>,
    new_base_map: Vec<(String, String)>,
    /// 当前选中基础系统的名字，用于自动拼标题
    new_base_name: String,
    /// 上一次自动拼出来的标题：与它不同说明用户手动改过，不再覆盖
    auto_name: String,
    settings_material_select: Entity<SelectState<Vec<String>>>,
    settings_language_select: Entity<SelectState<Vec<String>>>,
    settings_loglevel_select: Entity<SelectState<Vec<String>>>,
    alias_input: Entity<InputState>,
    alias_target: Option<String>,
    username_input: Entity<InputState>,
    password_input: Entity<InputState>,
    remember: bool,
    /// 已发现的新版本；`None` = 还没查过或已经是最新。
    update_release: Option<crate::services::update::Release>,
    /// 正在检查或下载，避免重复点击。
    update_busy: bool,
}

impl RootView {
    pub fn new(
        settings: AppSettings,
        tray: Option<crate::platform::tray::Tray>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let alias_input = cx.new(|cx| InputState::new(window, cx).placeholder(t!("app.alias_placeholder").to_string()));
        let username_input = cx.new(|cx| InputState::new(window, cx).placeholder(t!("app.account_placeholder").to_string()));
        let password_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(t!("app.password_placeholder").to_string())
                .masked(true)
        });

        let edit_name = cx.new(|cx| InputState::new(window, cx).placeholder(t!("app.name_placeholder").to_string()));
        let edit_client = cx.new(|cx| InputState::new(window, cx).placeholder(t!("app.client_placeholder").to_string()));
        let edit_user = cx.new(|cx| InputState::new(window, cx).placeholder(t!("app.user_placeholder").to_string()));
        let edit_language = cx.new(|cx| InputState::new(window, cx).placeholder(t!("app.language_placeholder").to_string()));
        let edit_workdir = cx.new(|cx| InputState::new(window, cx).placeholder(t!("app.workdir_placeholder").to_string()));

        let new_name = cx.new(|cx| InputState::new(window, cx).placeholder("名称"));
        let new_client = cx.new(|cx| InputState::new(window, cx).placeholder(t!("app.client_placeholder").to_string()));
        let new_user = cx.new(|cx| InputState::new(window, cx).placeholder(t!("app.user_placeholder").to_string()));
        let new_language = cx.new(|cx| InputState::new(window, cx).placeholder(t!("app.language_placeholder").to_string()));
        let new_workdir = cx.new(|cx| InputState::new(window, cx).placeholder(t!("app.workdir_placeholder").to_string()));
        let settings_logdir =
            cx.new(|cx| InputState::new(window, cx).placeholder(t!("app.log_dir_placeholder").to_string()));

        let new_reuse = cx.new(|_| true);
        let set_autostart = cx.new(|_| settings.general.autostart);
        let set_logging = cx.new(|_| settings.general.logging);
        let set_uuid = cx.new(|_| settings.sap.use_uuid);
        let new_folder_select = cx.new(|cx| SelectState::new(Vec::<String>::new(), None, window, cx));
        let new_base_select = cx.new(|cx| SelectState::new(Vec::<String>::new(), None, window, cx));

        // 静态下拉选项用 Vec<String>（它自带 SearchableListItem 实现）。
        let new_trace_select = cx.new(|cx| {
            SelectState::new(
                vec![
                    "无跟踪".to_string(),
                    "级别1：错误".to_string(),
                    "级别2：错误和警告".to_string(),
                    "级别3：全部（诊断跟踪）".to_string(),
                ],
                Some(IndexPath::default()),
                window,
                cx,
            )
        });

        let new_winmax_select = cx.new(|cx| {
            SelectState::new(
                vec!["普通窗口".to_string(), "最大化的".to_string()],
                Some(IndexPath::new(1)),
                window,
                cx,
            )
        });

        // 语言选项在打开新建弹框时按界面语言重建（名称要跟着 i18n 走）。
        let new_language_select = cx.new(|cx| {
            SelectState::new(Vec::<String>::new(), None, window, cx)
        });


        let settings_material_select = cx.new(|cx| {
            let index = match settings.general.material.as_str() {
                "acrylic" => 1,
                "opaque" => 2,
                _ => 0,
            };
            SelectState::new(
                vec!["Mica".to_string(), "亚克力".to_string(), "不透明".to_string()],
                Some(IndexPath::new(index)),
                window,
                cx,
            )
        });
        let settings_language_select = cx.new(|cx| {
            let index = match settings.general.language.as_str() {
                "zh_CN" | "zh-CN" => 1,
                "en" => 2,
                _ => 0,
            };
            SelectState::new(
                vec!["跟随系统".to_string(), "简体中文".to_string(), "English".to_string()],
                Some(IndexPath::new(index)),
                window,
                cx,
            )
        });
        let settings_loglevel_select = cx.new(|cx| {
            // 值仍按 log 的级别名存，界面给中文：这里的字符串只是给人看的。
            let index = match settings.general.log_level.as_str() {
                "warn" => 1,
                "info" => 2,
                "debug" => 3,
                "trace" => 4,
                _ => 0,
            };
            SelectState::new(
                vec![
                    "错误".to_string(),
                    "警告".to_string(),
                    "信息".to_string(),
                    "调试".to_string(),
                    "跟踪".to_string(),
                ],
                Some(IndexPath::new(index)),
                window,
                cx,
            )
        });

        let mut view = Self {
            settings,
            landscape: None,
            tray,
            selected: None,
            status: "就绪".into(),
            password_ready: HashSet::new(),
            client_colors: HashMap::new(),
            hovered_card: None,
            folder_picker_rx: None,
            cred_rx: None,
            folder_picker_edit: false,
            select_generation: 0,
            last_result: HashMap::new(),
            password_target: None,
            settings_target: false,
            settings_logdir,
            delete_target: None,
            editing: None,
            edit_name,
            edit_client,
            edit_user,
            edit_language,
            edit_workdir,
            creating: None,
            new_name,
            new_client,
            new_user,
            new_language,
            new_workdir,
            new_reuse,
            set_autostart,
            set_logging,
            set_uuid,
            new_winmax: true,
            new_trace: "无跟踪".into(),
            new_trace_select,
            new_winmax_select,
            new_language_select,
            new_language_code: String::new(),
            new_folder_select,
            new_base_select,
            new_folder_map: Vec::new(),
            new_base_map: Vec::new(),
            new_base_name: String::new(),
            auto_name: String::new(),
            settings_material_select,
            settings_language_select,
            settings_loglevel_select,
            alias_input,
            alias_target: None,
            username_input: username_input.clone(),
            password_input: password_input.clone(),
            remember: true,
            update_release: None,
            update_busy: false,
        };
        view.reload(cx);

        // 把已经开着的 SAP 会话窗口的任务栏图标按系统标识换掉。
        crate::platform::taskbar::apply_session_icons();

        // 材质是否真的落到 DWM 上，肉眼判断不可靠：启动时读回一次记进日志。
        log::info!(
            "窗口背景读回：{:?}（2 = Mica）",
            crate::platform::window::backdrop_type()
        );

        // 「跟随系统」必须跟到运行中的切换：外观变化由窗口通知，这里重切主题。
        let this = cx.entity().downgrade();
        window
            .observe_window_appearance(move |window, cx| {
                let appearance = window.appearance();
                this.update(cx, |view, cx| view.follow_system_appearance(appearance, cx))
                    .ok();
            })
            .detach();

        cx.subscribe_in(
            &username_input,
            window,
            |this, _state, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.login(window, cx);
                }
            },
        )
        .detach();
        cx.subscribe_in(
            &password_input,
            window,
            |this, _state, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.login(window, cx);
                }
            },
        )
        .detach();

        // Select 的选中结果走事件回写，不回调外部闭包。
        cx.subscribe_in(
            &view.new_trace_select,
            window,
            |this, _state, event: &SelectEvent<Vec<String>>, _window, cx| {
                if let SelectEvent::Confirm(Some(value)) = event {
                    this.new_trace = value.clone();
                    cx.notify();
                }
            },
        )
        .detach();

        cx.subscribe_in(
            &view.new_winmax_select,
            window,
            |this, _state, event: &SelectEvent<Vec<String>>, _window, cx| {
                if let SelectEvent::Confirm(Some(value)) = event {
                    this.new_winmax = value == "最大化的";
                    cx.notify();
                }
            },
        )
        .detach();

        cx.subscribe_in(
            &view.settings_material_select,
            window,
            |this, _state, event: &SelectEvent<Vec<String>>, window, cx| {
                if let SelectEvent::Confirm(Some(value)) = event {
                    this.settings.general.material = match value.as_str() {
                        "亚克力" => "acrylic",
                        "不透明" => "opaque",
                        _ => "mica",
                    }
                    .to_string();
                    let _ = this.settings.save();
                    // 材质是窗口属性，能当场换掉，不必重启。
                    window.set_background_appearance(material_appearance(
                        &this.settings.general.material,
                    ));
                    cx.notify();
                }
            },
        )
        .detach();

        cx.subscribe_in(
            &view.settings_language_select,
            window,
            |this, _state, event: &SelectEvent<Vec<String>>, _window, cx| {
                if let SelectEvent::Confirm(Some(value)) = event {
                    this.settings.general.language = match value.as_str() {
                        "English" => "en",
                        "简体中文" => "zh_CN",
                        _ => "",
                    }
                    .to_string();
                    let _ = this.settings.save();
                    let locale = this.settings.locale();
                    rust_i18n::set_locale(locale);
                    gpui_kit::component::set_locale(locale);
                    cx.refresh_windows();
                    cx.notify();
                }
            },
        )
        .detach();

        cx.subscribe_in(
            &view.settings_loglevel_select,
            window,
            |this, _state, event: &SelectEvent<Vec<String>>, _window, cx| {
                if let SelectEvent::Confirm(Some(value)) = event {
                    this.settings.general.log_level = match value.as_str() {
                        "警告" => "warn",
                        "信息" => "info",
                        "调试" => "debug",
                        "跟踪" => "trace",
                        _ => "error",
                    }
                    .to_string();
                    let _ = this.settings.save();
                    this.status = t!("app.settings_restart").to_string();
                    cx.notify();
                }
            },
        )
        .detach();

        cx.subscribe_in(
            &view.new_client,
            window,
            |this, _input, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::Change) {
                    this.refresh_new_name(window, cx);
                }
            },
        )
        .detach();

        cx.subscribe_in(
            &view.new_language,
            window,
            |this, _input, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::Change) {
                    this.refresh_new_name(window, cx);
                }
            },
        )
        .detach();

        view.start_tray_poll(window, cx);
        view
    }

    /// 托盘事件轮询：菜单点击投递到全局通道，这里在 UI 线程排空。
    /// 用 spawn（不绑定窗口）：窗口收进托盘后轮询必须继续跑。
    fn start_tray_poll(&self, _window: &mut Window, cx: &mut Context<Self>) {
        let this = cx.entity().clone();
        cx.spawn(async move |_weak_view, cx| loop {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(200))
                .await;

            let action =
                cx.update(|cx| this.update(cx, |view, cx| view.poll_tray(cx)));
            let Some(action) = action else {
                continue;
            };
            log::info!("tray action: {action:?}");

            match action {
                crate::platform::tray::TrayAction::Quit => {
                    let _ = cx.update(|cx| cx.quit());
                    return;
                }
                crate::platform::tray::TrayAction::Settings => {
                    // 先把窗口显示出来（不依赖 gpui 的窗口句柄），再弹设置。
                    crate::platform::window::show();
                    let _ = cx.update(|cx| {
                        log::info!("tray settings: {} window(s)", cx.windows().len());
                        for handle in cx.windows() {
                            let result = handle.update(cx, |_root, window, cx| {
                                // ShowWindow 之后 gpui 可能还认为窗口不可见，
                                // 不重绘的话恢复出来的是一张空窗口。
                                window.refresh();
                                let _ = this.update(cx, |view, cx| {
                                    view.open_settings_dialog(window, cx);
                                });
                            });
                            if result.is_err() {
                                log::warn!("tray settings: window update failed");
                            }
                        }
                    });
                }
                crate::platform::tray::TrayAction::ShowWindow => {
                    // 直接按进程找 HWND 显示，窗口隐藏时 gpui 的 handle 可能已失效。
                    crate::platform::window::show();
                    let _ = cx.update(|cx| {
                        for handle in cx.windows() {
                            let _ = handle.update(cx, |_root, window, _cx| {
                                window.refresh();
                                window.activate_window();
                            });
                        }
                    });
                }
            }
        })
        .detach();
    }    fn poll_tray(&self, _cx: &mut Context<Self>) -> Option<crate::platform::tray::TrayAction> {
        self.tray.as_ref().and_then(|tray| tray.poll())
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        self.reload_data(cx);
        // 先用上次扫描的缓存点亮状态灯，后台扫描再校正（错了就变灰）。
        if let Some(path) = crate::landscape::locate::landscape_path() {
            self.password_ready = crate::core::cache::load_password_ready(&path)
                .into_iter()
                .collect();
        }
        self.start_credential_scan(cx);
    }

    /// 凭据扫描放后台线程：49 个 CredReadW 在 UI 线程上要一两秒。
    /// 结果经 channel 回传，主线程轮询后更新状态灯，启动不被拖慢。
    fn start_credential_scan(&mut self, cx: &mut Context<Self>) {
        let Some(landscape) = &self.landscape else {
            return;
        };
        let uuids: Vec<String> = landscape
            .services
            .iter()
            .map(|service| service.uuid.clone())
            .collect();
        if uuids.is_empty() {
            return;
        }

        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let ready: Vec<String> = uuids
                .into_iter()
                .filter(|uuid| {
                    crate::core::cred::load(&crate::core::cred::target_for(uuid)).is_some()
                })
                .collect();
            let _ = tx.send(ready);
        });
        self.cred_rx = Some(rx);

        let this = cx.entity().downgrade();
        cx.spawn(async move |_weak, cx| loop {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(200))
                .await;
            let done = this
                .update(cx, |view, cx| {
                    let Some(rx) = &view.cred_rx else {
                        return true;
                    };
                    match rx.try_recv() {
                        Ok(ready) => {
                            view.cred_rx = None;
                            if let Some(path) = crate::landscape::locate::landscape_path() {
                                crate::core::cache::save_password_ready(&path, ready.clone());
                            }
                            view.password_ready = ready.into_iter().collect();
                            cx.notify();
                            true
                        }
                        Err(std::sync::mpsc::TryRecvError::Empty) => false,
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                            view.cred_rx = None;
                            true
                        }
                    }
                })
                .unwrap_or(true);
            if done {
                break;
            }
        })
        .detach();
    }

    /// 只重新解析 landscape，不查凭据——写回后调它，避免 49 次 CredReadW 拖慢保存。
    fn reload_data(&mut self, cx: &mut Context<Self>) {
        self.landscape = load_landscape();
        self.refresh_palettes();
        self.status = match &self.landscape {
            Some(landscape) => {
                let entries = count_entries(&landscape.tree);
                let folders = count_folders(&landscape.tree);
                // 快捷方式的判定规则一改就要能从日志看出效果，不必靠截图猜。
                log::info!(
                    "landscape: {} 个服务 / {} 个快捷方式 / {} 条树条目 / {} 个文件夹",
                    landscape.services.len(),
                    landscape
                        .services
                        .iter()
                        .filter(|service| service.is_shortcut())
                        .count(),
                    entries,
                    folders
                );
                format!("{} 个快捷方式 · {} 个文件夹", entries, folders)
            }
            None => "未找到 SAP Landscape 文件".into(),
        };
        cx.notify();
    }


    /// 客户端与语言的配色表：取 landscape 里出现过的全部取值，排序后按 6 色循环分配，
    /// 这样同一个客户端在任何卡片上颜色一致，且新增取值不会打乱已有顺序。
    fn refresh_palettes(&mut self) {
        let Some(landscape) = &self.landscape else {
            self.client_colors.clear();
            return;
        };
        let mut clients: Vec<String> = landscape
            .services
            .iter()
            .map(|service| service.client.trim().to_string())
            .filter(|client| !client.is_empty())
            .collect();
        clients.sort();
        clients.dedup();

        self.client_colors = clients
            .iter()
            .enumerate()
            .map(|(index, client)| (client.clone(), index % 6))
            .collect();
    }

    /// 选中卡片：3 秒后自动清除（用计数防止旧定时器清掉新选中）。
    fn select(&mut self, service_id: String, window: &mut Window, cx: &mut Context<Self>) {
        self.selected = Some(service_id.clone());
        self.select_generation += 1;
        let generation = self.select_generation;
        cx.spawn(async move |weak_view, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_secs(3))
                .await;
            weak_view
                .update(cx, |view, cx| {
                    if view.select_generation == generation {
                        view.selected = None;
                        cx.notify();
                    }
                })
                .ok();
        })
        .detach();
        self.selected = Some(service_id.clone());
        self.settings.window.last_service = service_id.clone();
        let bounds = window.bounds();
        self.settings.window.bounds = Some([
            f32::from(bounds.origin.x),
            f32::from(bounds.origin.y),
            f32::from(bounds.size.width),
            f32::from(bounds.size.height),
        ]);
        let _ = self.settings.save();

        let mut user = String::new();
        let mut password = String::new();
        if let Some(landscape) = &self.landscape {
            if let Some(service) = landscape.service(&service_id) {
                user = service.user.clone();
            }
        }
        if self.remember {
            if let Some((saved_user, saved_password)) =
                crate::core::cred::load(&crate::core::cred::target_for(&service_id))
            {
                user = saved_user;
                password = saved_password;
            }
        }

        self.username_input.update(cx, |state, cx| {
            state.set_value(user, window, cx);
        });
        self.password_input.update(cx, |state, cx| {
            state.set_value(password, window, cx);
        });
        cx.notify();
    }

    fn remember_recent(&mut self, service_id: &str) {
        let mut recent = self.settings.window.recent.clone();
        recent.retain(|id| id != service_id);
        recent.insert(0, service_id.to_string());
        recent.truncate(6);
        self.settings.window.recent = recent;
        let _ = self.settings.save();
    }

    /// 最近区的「删除」：只从最近列表移除，不动 SAP 里的快捷方式。
    fn remove_recent(&mut self, service_id: &str) {
        self.settings.window.recent.retain(|id| id != service_id);
        self.settings.window.pinned.retain(|id| id != service_id);
        let _ = self.settings.save();
    }

    fn toggle_pin(&mut self, service_id: &str) {
        if self.settings.window.pinned.iter().any(|id| id == service_id) {
            self.settings.window.pinned.retain(|id| id != service_id);
        } else {
            self.settings.window.pinned.insert(0, service_id.to_string());
            if !self.settings.window.recent.iter().any(|id| id == service_id) {
                self.settings.window.recent.insert(0, service_id.to_string());
            }
        }
        let _ = self.settings.save();
    }

    /// 上移 / 下移最近登录里的顺序（`delta` 为 -1 或 1）。
    fn move_recent(&mut self, service_id: &str, delta: isize) {
        // 固定项的显示顺序由 pinned 列表决定，改 recent 不会有效果。
        let list = if self
            .settings
            .window
            .pinned
            .iter()
            .any(|id| id == service_id)
        {
            &mut self.settings.window.pinned
        } else {
            &mut self.settings.window.recent
        };
        if let Some(index) = list.iter().position(|id| id == service_id) {
            let target = index as isize + delta;
            if target >= 0 && (target as usize) < list.len() {
                list.swap(index, target as usize);
                let _ = self.settings.save();
            }
        }
    }

    fn confirm_new_shortcut(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(creating) = self.creating.clone() else {
            return false;
        };
        let Some(landscape) = &self.landscape else {
            return false;
        };
        let Some(base) = landscape.service(&creating.base_id).cloned() else {
            self.status = "基础系统已不存在".into();
            cx.notify();
            return false;
        };
        let work_dir = self.new_workdir.read(cx).value().trim().to_string();

        let client = self.new_client.read(cx).value().trim().to_string();
        if client.len() != 3 || !client.chars().all(|c| c.is_ascii_digit()) {
            self.status = t!("app.client_invalid").to_string();
            self.show_error_dialog(t!("app.client_invalid").to_string(), window, cx);
            return false;
        }
        let user = self.new_user.read(cx).value().trim().to_string();
        let language = {
            let value = self.new_language.read(cx).value().trim().to_string();
            if value.is_empty() {
                self.settings.sap.default_language.clone()
            } else {
                value
            }
        };
        let mut name = self.new_name.read(cx).value().trim().to_string();
        if name.is_empty() || name == base.name {
            name = format!("{}_{}_{}", base.name, client, language);
        }

        let service = Service {
            uuid: uuid::Uuid::new_v4().to_string(),
            name,
            description: base.name.clone(),
            kind: ServiceKind::Reference,
            systemid: base.systemid.clone(),
            client,
            user,
            language,
            server: String::new(),
            msid: String::new(),
            routerid: String::new(),
            link: base.uuid.clone(),
            reuse: *self.new_reuse.read(cx),
            sncop: match self.new_trace.as_str() {
                "级别1：错误" => "1".to_string(),
                "级别2：错误和警告" => "2".to_string(),
                "级别3：全部（诊断跟踪）" => "3".to_string(),
                _ => String::new(),
            },
            winmax: self.new_winmax,
            work_dir,
            shortcut: true,
        };
        let item_uuid = uuid::Uuid::new_v4().to_string();
        let folder_id = creating.folder_id.clone();

        let Some(path) = crate::landscape::locate::landscape_path() else {
            self.status = t!("app.landscape_not_found").to_string();
            cx.notify();
            return false;
        };
        if crate::landscape::write::sap_logon_running() {
            self.status = t!("app.sap_logon_running").to_string();
            self.show_error_dialog(t!("app.sap_logon_running").to_string(), window, cx);
            return false;
        }

        let result = std::fs::read_to_string(&path)
            .map_err(anyhow::Error::from)
            .and_then(|text| {
                crate::landscape::write::add_shortcut(&text, &folder_id, &service, &item_uuid)
            })
            .and_then(|updated| {
                crate::landscape::write::save_atomic(&path, &updated).map(|_| ())
            });

        match result {
            Ok(()) => {
                self.creating = None;
                self.settings.sap.last_base_service = base.uuid.clone();
                let _ = self.settings.save();
                self.status = t!("app.created_restart", name = service.name).to_string();
                self.toast(self.status.clone(), NotificationType::Success, window, cx);
                self.reload_data(cx);
                true
            }
            Err(error) => {
                let message = t!("app.create_failed", error = error.to_string()).to_string();
                self.status = message.clone();
                self.show_error_dialog(message, window, cx);
                false
            }
        }
    }

    fn begin_edit(&mut self, service_id: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(service) = self
            .landscape
            .as_ref()
            .and_then(|landscape| landscape.service(&service_id))
            .cloned()
        else {
            return;
        };
        self.edit_name.update(cx, |state, cx| {
            state.set_value(service.name, window, cx);
        });
        self.edit_client.update(cx, |state, cx| {
            state.set_value(service.client, window, cx);
        });
        self.edit_user.update(cx, |state, cx| {
            state.set_value(service.user, window, cx);
        });
        self.edit_language.update(cx, |state, cx| {
            state.set_value(service.language, window, cx);
        });
        self.edit_workdir.update(cx, |state, cx| {
            state.set_value(service.work_dir, window, cx);
        });
        self.editing = Some(service_id);
        self.open_edit_dialog(window, cx);
    }

    /// 编辑参数：标准 Dialog，五个输入框与保存/取消按钮。
    fn open_edit_dialog(&self, window: &mut Window, cx: &mut Context<Self>) {
        let this = cx.entity().clone();
        let name = self.edit_name.clone();
        let client = self.edit_client.clone();
        let user = self.edit_user.clone();
        let language = self.edit_language.clone();
        let workdir = self.edit_workdir.clone();
        let muted = cx.theme().muted_foreground;

        window.open_dialog(cx, move |dialog, _window, _cx| {
            let confirm = this.clone();
            let row = |label: String, input: &Entity<InputState>| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .pb(px(10.))
                    .child(
                        div()
                            .w(px(72.))
                            .text_size(px(12.))
                            .text_color(muted)
                            .child(label),
                    )
                    .child(div().flex_1().child(Input::new(input)))
            };
            dialog
                .title(t!("app.edit_params").to_string())
                .w(px(560.))
                .child(row(t!("app.name").to_string(), &name))
                .child(row(t!("app.client").to_string(), &client))
                .child(row(t!("app.user").to_string(), &user))
                .child(row(t!("app.language").to_string(), &language))
                .child({
                    let pick_this = this.clone();
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .pb(px(10.))
                        .child(
                            div()
                                .w(px(72.))
                                .text_size(px(12.))
                                .text_color(muted)
                                .child(t!("app.work_dir").to_string()),
                        )
                        .child(div().flex_1().child(Input::new(&workdir)))
                        .child(
                            Button::new("pick-edit-workdir")
                                .label("…")
                                .on_click(move |_, window, cx| {
                                    pick_this.update(cx, |view, cx| {
                                        view.pick_workdir(true, window, cx);
                                    });
                                }),
                        )
                })
                .footer(
                    div()
                        .flex()
                        .flex_row()
                        .gap(px(8.))
                        .child(
                            Button::new("edit-cancel")
                                .label(t!("app.cancel").to_string())
                                .on_click(|_, window, cx| {
                                    window.close_dialog(cx);
                                }),
                        )
                        .child(
                            Button::new("edit-save")
                                .primary()
                                .label(t!("app.save").to_string())
                                .on_click(move |_, window, cx| {
                                    let saved =
                                        confirm.update(cx, |view, cx| view.confirm_edit(window, cx));
                                    if saved {
                                        window.close_dialog(cx);
                                    }
                                }),
                        ),
                )
        });
    }

    /// 保存编辑：只改用户能改的那几个属性，走与删除相同的写回路径。
    fn confirm_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(service_id) = self.editing.clone() else {
            return false;
        };
        let Some(mut service) = self
            .landscape
            .as_ref()
            .and_then(|landscape| landscape.service(&service_id))
            .cloned()
        else {
            return false;
        };

        service.name = self.edit_name.read(cx).value().trim().to_string();
        service.client = self.edit_client.read(cx).value().trim().to_string();
        service.user = self.edit_user.read(cx).value().trim().to_string();
        service.language = self.edit_language.read(cx).value().trim().to_string();
        service.work_dir = self.edit_workdir.read(cx).value().trim().to_string();

        if service.name.is_empty() || service.client.is_empty() {
            self.status = t!("app.title_required").to_string();
            self.show_error_dialog(t!("app.title_required").to_string(), window, cx);
            return false;
        }

        let Some(path) = crate::landscape::locate::landscape_path() else {
            self.status = t!("app.landscape_not_found").to_string();
            self.show_error_dialog(t!("app.landscape_not_found").to_string(), window, cx);
            return false;
        };
        if crate::landscape::write::sap_logon_running() {
            self.status = t!("app.sap_logon_running").to_string();
            self.show_error_dialog(t!("app.sap_logon_running").to_string(), window, cx);
            return false;
        }

        let result = std::fs::read_to_string(&path)
            .map_err(anyhow::Error::from)
            .and_then(|text| crate::landscape::write::update_shortcut(&text, &service))
            .and_then(|updated| {
                crate::landscape::write::save_atomic(&path, &updated).map(|_| ())
            });

        match result {
            Ok(()) => {
                self.editing = None;
                self.status = t!("app.saved_restart").to_string();
                self.toast(self.status.clone(), NotificationType::Success, window, cx);
                self.reload_data(cx);
                true
            }
            Err(error) => {
                let message = t!("app.save_failed", error = error.to_string()).to_string();
                self.status = message.clone();
                self.show_error_dialog(message, window, cx);
                false
            }
        }
    }

    fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let current = self.settings.general.log_dir.clone();
        self.settings_logdir.update(cx, |state, cx| {
            state.set_value(current, window, cx);
        });
        self.settings_target = true;
        self.open_settings_dialog(window, cx);
    }

    /// 设置：标准 Dialog。
    fn open_settings_dialog(&self, window: &mut Window, cx: &mut Context<Self>) {
        let this = cx.entity().clone();
        let logdir = self.settings_logdir.clone();
        let material = self.settings_material_select.clone();
        let language = self.settings_language_select.clone();
        let loglevel = self.settings_loglevel_select.clone();
        let autostart = self.set_autostart.clone();
        let logging = self.set_logging.clone();
        let uuid = self.set_uuid.clone();
        // 「组窗口」这一整套只在 8.10+ 存在；是否已就绪要两项都满足才算。
        let taskbar_supported = crate::platform::system::supports_taskbar_group();
        let taskbar_ready = crate::platform::system::taskbar_grouped_by_system()
            && crate::platform::system::additional_title_info_enabled();
        let landscape_path = crate::landscape::locate::landscape_path()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "—".into());

        window.open_dialog(cx, move |dialog, _window, cx| {
            let muted = cx.theme().muted_foreground;
            let theme = this.read(cx).settings.general.theme.clone();
            let autostart_value = *autostart.read(cx);
            let logging_value = *logging.read(cx);
            let uuid_value = *uuid.read(cx);
            let autostart_click = autostart.clone();
            let logging_click = logging.clone();
            let uuid_click = uuid.clone();
            let save_this = this.clone();

            let row = |label: String, content: AnyElement| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .pb(px(10.))
                    .child(
                        div()
                            .w(px(72.))
                            .text_size(px(12.))
                            .text_color(muted)
                            .child(label),
                    )
                    .child(div().flex_1().child(content))
            };
            let theme_button = |value: &'static str, label: String, active: bool| {
                let target = this.clone();
                let button = Button::new(element_id("theme", value)).label(label);
                let button = if active { button.primary() } else { button };
                button.on_click(move |_, window, cx| {
                    target.update(cx, |view, cx| {
                        view.settings.general.theme = value.to_string();
                        let _ = view.settings.save();
                        view.apply_theme(window, cx);
                        cx.notify();
                    });
                })
            };

            dialog
                .title(t!("app.settings").to_string())
                .w(px(560.))
                .child(row(
                    t!("app.appearance").to_string(),
                    div()
                        .flex()
                        .flex_row()
                        .gap(px(8.))
                        .child(theme_button("system", t!("app.theme_system").to_string(), theme.is_empty() || theme == "system"))
                        .child(theme_button("light", t!("app.theme_light").to_string(), theme == "light"))
                        .child(theme_button("dark", t!("app.theme_dark").to_string(), theme == "dark"))
                        .into_any_element(),
                ))
                .child(row(
                    t!("app.material").to_string(),
                    Select::new(&material).small().into_any_element(),
                ))
                .child(row(
                    t!("app.language").to_string(),
                    Select::new(&language).small().into_any_element(),
                ))
                .child(row(
                    t!("app.startup").to_string(),
                    Checkbox::new("set-autostart")
                        .checked(autostart_value)
                        .label(t!("app.autostart").to_string())
                        .on_click(move |value: &bool, _window, cx| {
                            autostart_click.update(cx, |state, cx| {
                                *state = *value;
                                cx.notify();
                            });
                        })
                        .into_any_element(),
                ))
                .child(row(
                    t!("app.logging").to_string(),
                    Checkbox::new("set-logging")
                        .checked(logging_value)
                        .label(t!("app.log_to_file").to_string())
                        .on_click(move |value: &bool, _window, cx| {
                            logging_click.update(cx, |state, cx| {
                                *state = *value;
                                cx.notify();
                            });
                        })
                        .into_any_element(),
                ))
                .child(row(
                    t!("app.log_level").to_string(),
                    Select::new(&loglevel).small().into_any_element(),
                ))
                .child(row(
                    t!("app.log_dir").to_string(),
                    Input::new(&logdir).into_any_element(),
                ))
                .child(row(
                    t!("app.sap_section").to_string(),
                    Checkbox::new("set-uuid")
                        .checked(uuid_value)
                        .label(t!("app.uuid_mode").to_string())
                        .on_click(move |value: &bool, _window, cx| {
                            uuid_click.update(cx, |state, cx| {
                                *state = *value;
                                cx.notify();
                            });
                        })
                        .into_any_element(),
                ))
                .child(row(
                    t!("app.taskbar_group").to_string(),
                    {
                        let group_this = this.clone();
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(8.))
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(muted)
                                    .child(t!("app.taskbar_group_hint").to_string()),
                            )
                            .child(if !taskbar_supported {
                                div()
                                    .text_size(px(11.))
                                    .text_color(muted)
                                    .child(t!("app.taskbar_group_unsupported").to_string())
                                    .into_any_element()
                            } else if taskbar_ready {
                                div()
                                    .text_size(px(11.))
                                    .child(t!("app.taskbar_group_ready").to_string())
                                    .into_any_element()
                            } else {
                                Button::new("enable-taskbar-group")
                                    .label(t!("app.taskbar_group_enable").to_string())
                                    .on_click(move |_, window, cx| {
                                        group_this.update(cx, |view, cx| {
                                            view.enable_taskbar_group(window, cx);
                                        });
                                    })
                                    .into_any_element()
                            })
                            .into_any_element()
                    },
                ))
                .child(row(
                    t!("app.check_update").to_string(),
                    {
                        let check_this = this.clone();
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(muted)
                                    .child(format!("v{}", env!("CARGO_PKG_VERSION"))),
                            )
                            .child(
                                Button::new("settings-check-update")
                                    .label(t!("app.check_update").to_string())
                                    .on_click(move |_, window, cx| {
                                        check_this.update(cx, |view, cx| {
                                            view.check_update(window, cx);
                                        });
                                    }),
                            )
                            .into_any_element()
                    },
                ))
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(muted)
                        .child(t!("app.landscape_path", path = landscape_path.clone()).to_string()),
                )
                .footer(
                    div()
                        .flex()
                        .flex_row()
                        .gap(px(8.))
                        .child(
                            Button::new("settings-cancel")
                                .label(t!("app.cancel").to_string())
                                .on_click(|_, window, cx| {
                                    window.close_dialog(cx);
                                }),
                        )
                        .child(
                            Button::new("settings-save")
                                .primary()
                                .label(t!("app.save").to_string())
                                .on_click(move |_, window, cx| {
                                    save_this.update(cx, |view, cx| view.save_settings(cx));
                                    window.close_dialog(cx);
                                }),
                        ),
                )
        });
    }

    /// 检查更新：网络请求放后台，结果回到 UI 再决定弹什么。
    fn check_update(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.update_busy {
            return;
        }
        self.update_busy = true;
        cx.notify();

        let this = cx.entity().clone();
        cx.spawn_in(window, async move |_weak, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { crate::services::update::check() })
                .await;
            let _ = this.update_in(cx, |view, window, cx| {
                view.update_busy = false;
                match result {
                    Ok(Some(release)) => {
                        view.update_release = Some(release);
                        view.open_update_dialog(window, cx);
                    }
                    Ok(None) => view.toast(
                        t!("app.update_latest").to_string(),
                        NotificationType::Success,
                        window,
                        cx,
                    ),
                    Err(error) => view.show_error_dialog(
                        t!("app.update_failed", error = error.to_string()).to_string(),
                        window,
                        cx,
                    ),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn open_update_dialog(&self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(release) = self.update_release.clone() else {
            return;
        };
        let this = cx.entity().clone();
        window.open_dialog(cx, move |dialog, _window, cx| {
            let muted = cx.theme().muted_foreground;
            let install = this.clone();
            let notes = if release.notes.is_empty() {
                t!("app.update_no_notes").to_string()
            } else {
                release
                    .notes
                    .lines()
                    .take(12)
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            dialog
                .title(
                    t!("app.update_title", version = release.version.clone()).to_string(),
                )
                .w(px(460.))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(8.))
                        .child(
                            div()
                                .text_size(px(12.))
                                .text_color(muted)
                                .child(
                                    t!(
                                        "app.current_version",
                                        version = env!("CARGO_PKG_VERSION")
                                    )
                                    .to_string(),
                                ),
                        )
                        .child(div().text_size(px(12.)).child(notes)),
                )
                .footer(
                    div()
                        .flex()
                        .flex_row()
                        .gap(px(8.))
                        .child(
                            Button::new("update-later")
                                .label(t!("app.cancel").to_string())
                                .on_click(|_, window, cx| window.close_dialog(cx)),
                        )
                        .child(
                            Button::new("update-install")
                                .primary()
                                .label(t!("app.update_install").to_string())
                                .on_click(move |_, window, cx| {
                                    window.close_dialog(cx);
                                    install.update(cx, |view, cx| view.install_update(window, cx));
                                }),
                        ),
                )
        });
    }

    /// 下载并交给安装器。安装器会结束本进程，所以成功路径上直接退出。
    fn install_update(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(release) = self.update_release.clone() else {
            return;
        };
        if self.update_busy {
            return;
        }
        self.update_busy = true;
        self.toast(
            t!("app.update_downloading").to_string(),
            NotificationType::Info,
            window,
            cx,
        );

        let this = cx.entity().clone();
        cx.spawn_in(window, async move |_weak, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { crate::services::update::download(&release) })
                .await;
            let _ = this.update_in(cx, |view, window, cx| {
                view.update_busy = false;
                match result.and_then(|path| crate::services::update::run_installer(&path)) {
                    Ok(()) => {
                        // 安装脚本会 taskkill 旧进程；主动退出让它安静地覆盖文件。
                        log::info!("更新：已拉起安装器，退出当前进程");
                        cx.quit();
                    }
                    Err(error) => view.show_error_dialog(
                        t!("app.update_failed", error = error.to_string()).to_string(),
                        window,
                        cx,
                    ),
                }
            });
        })
        .detach();
    }

    /// 按设置里的主题档套用；「跟随系统」读窗口外观。
    fn apply_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mode = theme_mode(&self.settings.general.theme, window.appearance());
        gpui_kit::component::Theme::change(mode, Some(window), cx);
    }

    /// 「跟随系统」时窗口外观一变就跟上；固定浅色/深色是用户的明确选择，不响应系统。
    fn follow_system_appearance(
        &mut self,
        appearance: gpui_kit::WindowAppearance,
        cx: &mut Context<Self>,
    ) {
        if matches!(self.settings.general.theme.as_str(), "light" | "dark") {
            return;
        }
        let mode = theme_mode("", appearance);
        if gpui_kit::component::Theme::global(cx).mode == mode {
            return;
        }
        log::info!("主题跟随系统：{mode:?}");
        gpui_kit::component::Theme::change(mode, None, cx);
        cx.notify();
    }

    fn save_settings(&mut self, cx: &mut Context<Self>) {
        self.settings.general.log_dir = self
            .settings_logdir
            .read(cx)
            .value()
            .trim()
            .to_string();
        self.settings.general.autostart = *self.set_autostart.read(cx);
        self.settings.general.logging = *self.set_logging.read(cx);
        self.settings.sap.use_uuid = *self.set_uuid.read(cx);
        if let Err(error) = crate::platform::autostart::apply(self.settings.general.autostart) {
            log::warn!("设置开机自启失败：{error}");
        }
        let _ = self.settings.save();
        self.settings_target = false;
        self.status = t!("app.settings_saved").to_string();
        cx.notify();
    }

    /// 一键开启「组窗口」+ 窗口标题里的系统信息。
    ///
    /// 两个键都只对**新开的**会话窗口生效；而且 SAP GUI 运行时会拿内存里的旧值
    /// 回写 Appearance，所以必须先让它完全退出，否则写进去的会在它关掉时被抹掉。
    fn enable_taskbar_group(&self, window: &mut Window, cx: &mut Context<Self>) {
        if crate::platform::process::is_running("saplogon.exe")
            || crate::platform::process::is_running("sapgui.exe")
        {
            self.toast(
                t!("app.taskbar_group_need_exit").to_string(),
                NotificationType::Warning,
                window,
                cx,
            );
            return;
        }

        let mut failed: Option<String> = None;
        if !crate::platform::system::taskbar_grouped_by_system() {
            if let Err(error) = crate::platform::system::set_taskbar_grouping_grouped() {
                failed = Some(format!("TaskbarGrouping: {error}"));
            }
        }
        if failed.is_none() && !crate::platform::system::additional_title_info_enabled() {
            if let Err(error) = crate::platform::system::enable_additional_title_info() {
                failed = Some(format!("ShowAdditionalTitleInfo: {error}"));
            }
        }
        if let Some(error) = failed {
            log::warn!("开启组窗口失败：{error}");
            self.toast(
                t!("app.taskbar_group_failed").to_string(),
                NotificationType::Error,
                window,
                cx,
            );
            return;
        }

        // 回读确认：组策略可能把这个键锁死，写的时候不报错但值没变。
        let written = crate::platform::system::taskbar_grouped_by_system()
            && crate::platform::system::additional_title_info_enabled();
        let (message, kind) = if written {
            (t!("app.taskbar_group_done").to_string(), NotificationType::Success)
        } else {
            (t!("app.taskbar_group_failed").to_string(), NotificationType::Error)
        };
        self.toast(message, kind, window, cx);
    }

    fn ask_delete(&mut self, service_id: String, window: &mut Window, cx: &mut Context<Self>) {
        self.delete_target = Some(service_id);
        self.open_delete_dialog(window, cx);
    }

    /// 删除确认：标准 Dialog，遮罩与焦点由组件接管。
    fn open_delete_dialog(&self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self
            .delete_target
            .as_ref()
            .and_then(|uuid| self.landscape.as_ref()?.service(uuid))
            .map(|service| service.name.clone())
            .unwrap_or_default();
        let this = cx.entity().clone();

        window.open_dialog(cx, move |dialog, _window, cx| {
            let confirm = this.clone();
            dialog
                .title(t!("app.delete_question", name = name).to_string())
                .w(px(460.))
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(cx.theme().muted_foreground)
                        .child(t!("app.delete_hint").to_string()),
                )
                .footer(
                    div()
                        .flex()
                        .flex_row()
                        .gap(px(8.))
                        .child(
                            Button::new("delete-cancel")
                                .label(t!("app.cancel").to_string())
                                .on_click(|_, window, cx| {
                                    window.close_dialog(cx);
                                }),
                        )
                        .child(
                            Button::new("delete-ok")
                                .primary()
                                .label(t!("app.delete").to_string())
                                .on_click(move |_, window, cx| {
                                    let removed =
                                        confirm.update(cx, |view, cx| view.confirm_delete(window, cx));
                                    if removed {
                                        window.close_dialog(cx);
                                    }
                                }),
                        ),
                )
        });
    }

    /// 删除写回：同时删 `Item` 与 `Reference`，写前备份并检查 SAP Logon 是否在运行。
    fn confirm_delete(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(service_uuid) = self.delete_target.clone() else {
            return false;
        };
        // 只允许删快捷方式（Reference）：基础系统是 SAP 自己的配置，误删会丢连接。
        let Some(service) = self
            .landscape
            .as_ref()
            .and_then(|landscape| landscape.service(&service_uuid))
            .cloned()
        else {
            self.status = t!("app.shortcut_gone").to_string();
            self.show_error_dialog(self.status.clone(), window, cx);
            return false;
        };
        if service.kind != ServiceKind::Reference {
            let message = t!("app.delete_reference_only").to_string();
            self.status = message.clone();
            self.show_error_dialog(message, window, cx);
            return false;
        }
        let Some(path) = crate::landscape::locate::landscape_path() else {
            self.status = t!("app.landscape_not_found").to_string();
            self.show_error_dialog(self.status.clone(), window, cx);
            return false;
        };
        if crate::landscape::write::sap_logon_running() {
            self.status = t!("app.sap_logon_running").to_string();
            self.show_error_dialog(t!("app.sap_logon_running").to_string(), window, cx);
            return false;
        }

        let result = std::fs::read_to_string(&path)
            .map_err(anyhow::Error::from)
            .and_then(|text| crate::landscape::write::remove_shortcut(&text, &service_uuid))
            .and_then(|updated| {
                crate::landscape::write::save_atomic(&path, &updated).map(|_| ())
            });

        match result {
            Ok(()) => {
                self.delete_target = None;
                self.settings.window.recent.retain(|id| id != &service_uuid);
                self.settings.window.pinned.retain(|id| id != &service_uuid);
                let _ = self.settings.save();
                self.status = t!("app.deleted_restart").to_string();
                self.toast(self.status.clone(), NotificationType::Success, window, cx);
                self.reload_data(cx);
                true
            }
            Err(error) => {
                let message = t!("app.delete_failed", error = error.to_string()).to_string();
                self.status = message.clone();
                self.show_error_dialog(message, window, cx);
                false
            }
        }
    }

    /// 删除确认弹框。
    fn begin_set_password(&mut self, service_id: String, window: &mut Window, cx: &mut Context<Self>) {        let user = self
            .landscape
            .as_ref()
            .and_then(|landscape| landscape.service(&service_id))
            .map(|service| service.user.clone())
            .unwrap_or_default();
        self.username_input.update(cx, |state, cx| {
            state.set_value(user, window, cx);
        });
        self.password_input.update(cx, |state, cx| {
            state.set_value(String::new(), window, cx);
        });
        self.password_target = Some(service_id);
        self.status = t!("app.set_password").to_string();
        self.open_password_dialog(window, cx);
    }

    /// 设置别名：只改本地显示名，不写回 SAP GUI 的 Landscape。
    fn begin_set_alias(&mut self, service_id: String, window: &mut Window, cx: &mut Context<Self>) {
        let current = self
            .settings
            .aliases
            .get(&service_id)
            .cloned()
            .unwrap_or_default();
        self.alias_input.update(cx, |state, cx| {
            state.set_value(current, window, cx);
        });
        self.alias_target = Some(service_id);
        let this = cx.entity().clone();
        let input = self.alias_input.clone();

        window.open_dialog(cx, move |dialog, _window, cx| {
            let confirm = this.clone();
            dialog
                .title(t!("app.set_alias").to_string())
                .w(px(420.))
                .child(
                    div()
                        .pb(px(10.))
                        .text_size(px(11.))
                        .text_color(cx.theme().muted_foreground)
                        .child(t!("app.alias_hint").to_string()),
                )
                .child(Input::new(&input))
                .footer(
                    div()
                        .flex()
                        .flex_row()
                        .gap(px(8.))
                        .child(
                            Button::new("alias-cancel")
                                .label(t!("app.cancel").to_string())
                                .on_click(|_, window, cx| {
                                    window.close_dialog(cx);
                                }),
                        )
                        .child(
                            Button::new("alias-save")
                                .primary()
                                .label(t!("app.save").to_string())
                                .on_click(move |_, window, cx| {
                                    confirm.update(cx, |view, cx| view.confirm_alias(cx));
                                    window.close_dialog(cx);
                                }),
                        ),
                )
        });
    }

    fn confirm_alias(&mut self, cx: &mut Context<Self>) {
        let Some(uuid) = self.alias_target.clone() else {
            return;
        };
        let value = self.alias_input.read(cx).value().trim().to_string();
        if value.is_empty() {
            self.settings.aliases.remove(&uuid);
        } else {
            self.settings.aliases.insert(uuid, value);
        }
        let _ = self.settings.save();
        self.alias_target = None;
        cx.refresh_windows();
        cx.notify();
    }

    /// 启动进度弹框：Spinner + 说明，启动后由 login 里的定时器自动关闭。
    fn open_launching_dialog(&self, name: String, window: &mut Window, cx: &mut Context<Self>) {
        window.open_dialog(cx, move |dialog, _window, cx| {
            dialog
                .title(t!("app.launching", name = name).to_string())
                .w(px(360.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .pb(px(6.))
                        .child(Spinner::new())
                        .child(
                            div()
                                .text_size(px(12.))
                                .text_color(cx.theme().muted_foreground)
                                .child(t!("app.launching_hint").to_string()),
                        ),
                )
        });
    }

    /// 双击卡片 / 右键「登录」：有密码直接登录，没有就弹框输入。
    fn login(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(service_id) = self.selected.clone() else {
            self.status = t!("app.select_first").to_string();
            return;
        };
        let (service_uuid, service_name, service_user) = {
            let Some(landscape) = &self.landscape else {
                self.status = t!("app.landscape_not_found").to_string();
                return;
            };
            let Some(service) = landscape.service(&service_id) else {
                self.status = t!("app.shortcut_gone").to_string();
                return;
            };
            (
                service.uuid.clone(),
                service.name.clone(),
                service.user.clone(),
            )
        };

        match crate::core::cred::load(&crate::core::cred::target_for(&service_uuid)) {
            Some((user, password)) => {
                // sapshcut 拉起 SAP Logon 要几秒，先给一个进度弹框，约 1 秒后自动关闭。
                self.open_launching_dialog(service_name.clone(), window, cx);
                self.do_login(service_uuid, user, password, window, cx);
                cx.spawn(async move |_this, cx| {
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(900))
                        .await;
                    cx.update(|cx| {
                        for handle in cx.windows() {
                            handle
                                .update(cx, |_root, window, cx| {
                                    window.close_dialog(cx);
                                })
                                .ok();
                        }
                    });
                })
                .detach();
            }
            None => {
                // 没有保存密码：先弹框要密码，确认后继续登录。
                self.username_input.update(cx, |state, cx| {
                    state.set_value(service_user, window, cx);
                });
                self.password_input.update(cx, |state, cx| {
                    state.set_value(String::new(), window, cx);
                });
                self.status = t!("app.first_login", name = service_name).to_string();
                self.password_target = Some(service_uuid);
                self.open_password_dialog(window, cx);
            }
        }
    }

    /// 密码弹框确认：保存到凭据管理器，然后继续登录。
    fn confirm_password(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(service_uuid) = self.password_target.clone() else {
            return;
        };
        let user = self
            .username_input
            .read(cx)
            .value()
            .to_string()
            .trim()
            .to_string();
        let password = self.password_input.read(cx).value().to_string();
        if user.is_empty() || password.is_empty() {
            self.status = t!("app.need_credentials").to_string();
            cx.notify();
            return;
        }
        let target = crate::core::cred::target_for(&service_uuid);
        if !crate::core::cred::save(&target, &user, &password) {
            self.status = t!("app.save_password_failed").to_string();
            cx.notify();
            return;
        }
        // 刚存过凭据，直接进缓存：下次启动立刻点亮，不必等后台扫描。
        if let Some(path) = crate::landscape::locate::landscape_path() {
            let mut ready: Vec<String> = self.password_ready.iter().cloned().collect();
            if !ready.contains(&service_uuid) {
                ready.push(service_uuid.clone());
            }
            crate::core::cache::save_password_ready(&path, ready);
        }
        self.password_ready.insert(service_uuid.clone());
        self.password_target = None;
        self.do_login(service_uuid, user, password, window, cx);
    }

    fn do_login(
        &mut self,
        service_uuid: String,
        user: String,
        password: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(landscape) = &self.landscape else {
            self.status = t!("app.landscape_not_found").to_string();
            return;
        };
        let Some(service) = landscape.service(&service_uuid) else {
            self.status = "快捷方式已不存在".into();
            return;
        };
        let Some(base) = landscape.base_of(service) else {
            self.status = t!("app.base_gone").to_string();
            return;
        };
        let client = service.client.trim().to_string();
        if client.is_empty() {
            self.status = t!("app.no_client").to_string();
            return;
        }
        let language = if service.language.trim().is_empty() {
            self.settings.sap.default_language.clone()
        } else {
            service.language.clone()
        };
        let request = LoginRequest {
            client,
            user,
            password,
            language,
            use_uuid: self.settings.sap.use_uuid,
        };
        let args = shcut::build_args(landscape, base, &request);
        let service_name = service.name.clone();

        let Some(exe) = shcut::find_sapshcut(&self.settings.sap.sapshcut_path) else {
            self.status = t!("app.sapshcut_missing").to_string();
            return;
        };
        log::info!("launch: {}", shcut::masked_command(&exe, &args));
        match shcut::launch(&exe, &args) {
            Ok(()) => {
                self.remember_recent(&service_uuid);
                self.last_result.insert(service_uuid.clone(), true);
                self.status = t!("app.launched", name = service_name).to_string();
                self.toast(self.status.clone(), NotificationType::Success, window, cx);
                // SAP GUI 拉窗口要几秒，等它出现再补任务栏图标；只补这一次，不轮询。
                cx.spawn(async move |_this, cx| {
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(4000))
                        .await;
                    crate::platform::taskbar::apply_session_icons();
                })
                .detach();
            }
            Err(error) => {
                self.last_result.insert(service_uuid.clone(), false);
                self.status = t!("app.launch_failed", error = error.to_string()).to_string();
            }
        }
        cx.notify();
    }

    fn header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let border = cx.theme().border;
        let control = move |id: &'static str, area: WindowControlArea, glyph: &'static str| {
            div()
                .id(id)
                .w(px(34.))
                .h(px(30.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .text_size(px(13.))
                .text_color(muted)
                .hover(|style| style.bg(muted.opacity(0.12)))
                .window_control_area(area)
                .occlude()
                .child(glyph)
        };
        // 工具栏按钮走标准 Button（ghost 变体）；窗口控制按钮必须自绘才能挂
        // window_control_area，那两个保持手写。
        let tool = move |id: &'static str, glyph: &'static str| {
            Button::new(id).ghost().small().label(glyph)
        };

        // 顶栏是浮层：不占布局，左半透明可拖，右半是工具与窗口按钮。
        div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .h(px(TITLE_BAR))
            .flex()
            .items_center()
            .px(px(8.))
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .occlude()
                    .window_control_area(WindowControlArea::Drag),
            )
            .child(
                tool("new-shortcut", "+").on_click(cx.listener(
                    |this, _: &ClickEvent, window, cx| {
                        this.begin_new_shortcut(window, cx);
                    },
                )),
            )
            .child(
                tool("open-settings", "⚙").on_click(cx.listener(
                    |this, _: &ClickEvent, window, cx| {
                        this.open_settings(window, cx);
                    },
                )),
            )
            .child(
                div()
                    .w(px(1.))
                    .h(px(18.))
                    .mx(px(6.))
                    .bg(border),
            )
            .child(control("win-min", WindowControlArea::Min, "–"))
            .child(control("win-max", WindowControlArea::Max, "□"))
            .child(
                // 关闭不退出进程：收进任务栏（托盘里仍可恢复），退出走托盘菜单。
                div()
                    .id("win-close")
                    .w(px(34.))
                    .h(px(30.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.))
                    .text_size(px(13.))
                    .text_color(muted)
                    .hover(move |style| style.bg(muted.opacity(0.12)))
                    .occlude()
                    .on_click(cx.listener(|_this, _: &ClickEvent, _window, _cx| {
                        // 收进托盘：ShowWindow(SW_HIDE)，任务栏也一起消失。
                        crate::platform::window::hide();
                    }))
                    .child("✕"),
            )
            .child(div().w(px(4.)))
    }

    fn begin_new_shortcut(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let last_base = self.settings.sap.last_base_service.clone();
        let default_language = self.settings.sap.default_language.clone();

        // 先把选项与默认值算齐，结束对 landscape 的借用，后面要改 self。
        let (folders, bases, work_dir) = {
            let Some(landscape) = &self.landscape else {
                self.show_error_dialog(t!("app.landscape_not_found").to_string(), window, cx);
                return;
            };
            // 倒序：配置里越晚出现的是越新添加的，排在最前面方便找。
            let mut folders = folder_options(landscape);
            folders.reverse();
            // 工作区允许把条目直接挂在根下（SAP 自己就这么存），所以永远留一个「顶层」：
            // 一条文件夹都没有的 Landscape 不该让新建按钮点了毫无反应。
            folders.push((String::new(), t!("app.root_folder").to_string()));
            let mut bases: Vec<(String, String, String, String)> = landscape
                .services
                .iter()
                .filter(|service| service.kind == ServiceKind::SapGui)
                .map(|service| {
                    (
                        service.uuid.clone(),
                        format!("{} ({})", service.name, service.systemid),
                        service.name.clone(),
                        service.user.clone(),
                    )
                })
                .collect();
            bases.reverse();
            let work_dir = landscape
                .services
                .iter()
                .find(|service| service.kind == ServiceKind::Reference && !service.work_dir.is_empty())
                .map(|service| service.work_dir.clone())
                .unwrap_or_default();
            (folders, bases, work_dir)
        };

        // 没有基础系统的 Landscape 建不出快捷方式：说清原因，别让按钮看起来像坏了。
        if bases.is_empty() {
            self.show_error_dialog(t!("app.no_base").to_string(), window, cx);
            return;
        }

        let base_index = bases
            .iter()
            .position(|(uuid, _, _, _)| uuid == &last_base)
            .unwrap_or(0);
        let (base_uuid, _, base_name, base_user) = bases[base_index].clone();
        let base_label = bases[base_index].1.clone();

        self.new_folder_map = folders
            .iter()
            .map(|(uuid, label)| (label.clone(), uuid.clone()))
            .collect();
        self.new_base_map = bases
            .iter()
            .map(|(uuid, label, _, _)| (label.clone(), uuid.clone()))
            .collect();

        let folder_items: Vec<String> = folders.iter().map(|(_, label)| label.clone()).collect();
        let base_items: Vec<String> = bases.iter().map(|(_, label, _, _)| label.clone()).collect();

        self.creating = Some(NewShortcut {
            folder_id: folders[0].0.clone(),
            base_id: base_uuid.clone(),
        });
        self.new_base_name = base_name.clone();
        self.auto_name = base_name.clone();

        // 选项随 landscape 变化，所以每次打开都重建 SelectState 并重新订阅。
        self.new_folder_select = cx.new(|cx| {
            SelectState::new(folder_items, Some(IndexPath::default()), window, cx)
        });
        self.new_base_select = cx.new(|cx| {
            SelectState::new(base_items, Some(IndexPath::default()), window, cx)
        });

        let folder_select = self.new_folder_select.clone();
        cx.subscribe_in(
            &folder_select,
            window,
            |this, _state, event: &SelectEvent<Vec<String>>, _window, cx| {
                if let SelectEvent::Confirm(Some(value)) = event {
                    let picked = this
                        .new_folder_map
                        .iter()
                        .find(|(label, _)| label == value)
                        .map(|(_, uuid)| uuid.clone());
                    if let (Some(uuid), Some(creating)) = (picked, this.creating.as_mut()) {
                        creating.folder_id = uuid;
                    }
                    cx.notify();
                }
            },
        )
        .detach();

        let base_select = self.new_base_select.clone();
        cx.subscribe_in(
            &base_select,
            window,
            |this, _state, event: &SelectEvent<Vec<String>>, window, cx| {
                if let SelectEvent::Confirm(Some(value)) = event {
                    let picked = this
                        .new_base_map
                        .iter()
                        .find(|(label, _)| label == value)
                        .map(|(_, uuid)| uuid.clone());
                    if let Some(uuid) = picked {
                        if let Some(name) = this
                            .landscape
                            .as_ref()
                            .and_then(|landscape| landscape.service(&uuid))
                            .map(|service| service.name.clone())
                        {
                            this.new_base_name = name;
                        }
                        if let Some(creating) = this.creating.as_mut() {
                            creating.base_id = uuid;
                        }
                        this.refresh_new_name(window, cx);
                    }
                    cx.notify();
                }
            },
        )
        .detach();

        let _ = base_label;
        self.new_name.update(cx, |state, cx| {
            state.set_value(base_name, window, cx);
        });
        self.new_client.update(cx, |state, cx| {
            state.set_value(String::new(), window, cx);
        });
        self.new_user.update(cx, |state, cx| {
            state.set_value(base_user, window, cx);
        });
        self.new_language_code = default_language.clone();
        self.new_language.update(cx, |state, cx| {
            state.set_value(default_language.clone(), window, cx);
        });
        // 语言下拉：按当前界面语言重建选项（名称要国际化），并选中默认语言。
        let locale_en = rust_i18n::locale().to_string().starts_with("en");
        let language_options = language_options(locale_en);
        let default_language_index = language_options
            .iter()
            .position(|option| option.starts_with(&default_language))
            .unwrap_or(0);
        self.new_language_select = cx.new(|cx| {
            SelectState::new(
                language_options,
                Some(IndexPath::new(default_language_index)),
                window,
                cx,
            )
        });
        let language_select = self.new_language_select.clone();
        cx.subscribe_in(
            &language_select,
            window,
            |this, _state, event: &SelectEvent<Vec<String>>, window, cx| {
                if let SelectEvent::Confirm(Some(value)) = event {
                    let code = value.split(' ').next().unwrap_or_default().to_string();
                    this.new_language_code = code.clone();
                    this.new_language.update(cx, |state, cx| {
                        state.set_value(code, window, cx);
                    });
                    this.refresh_new_name(window, cx);
                    cx.notify();
                }
            },
        )
        .detach();
        self.new_workdir.update(cx, |state, cx| {
            state.set_value(work_dir, window, cx);
        });
        // 打开时就把基础系统 + 语言拼进标题，用户之后改哪项都会重算。
        self.refresh_new_name(window, cx);
        self.new_reuse.update(cx, |value, cx| {
            *value = true;
            cx.notify();
        });
        self.new_winmax = true;
        self.new_trace = "无跟踪".into();
        self.status = t!("app.new_shortcut").to_string();
        self.open_new_dialog(window, cx);
    }

    /// 新建快捷方式：标准 Dialog，下拉用 Select。
    /// 失败提示：用 Dialog 而不是只写状态栏，状态栏在弹框后面看不明显。
    fn show_error_dialog(&self, message: String, window: &mut Window, cx: &mut Context<Self>) {
        window.open_dialog(cx, move |dialog, _window, cx| {
            dialog
                .title(t!("app.error_title").to_string())
                .w(px(420.))
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(cx.theme().foreground)
                        .child(message.clone()),
                )
                .footer(
                    Button::new("error-ok")
                        .primary()
                        .label(t!("app.ok").to_string())
                        .on_click(|_, window, cx| {
                            window.close_dialog(cx);
                        }),
                )
        });
    }

    /// 操作反馈：窗口右上角浮出几秒自动消失，不占布局。
    fn toast(
        &self,
        message: String,
        kind: NotificationType,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.push_notification((kind, message), cx);
    }

    /// 标题自动拼接：`基础系统_客户端_语言`。用户手动改过标题就不再覆盖。
    /// 选工作目录：后台线程弹系统对话框，主线程每 100ms 轮询结果。
    /// 不能在回调里同步等待——会触发 gpui 的 App 借用冲突直接 panic。
    fn pick_workdir(&mut self, edit: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.folder_picker_rx.is_some() {
            return;
        }
        self.folder_picker_rx = Some(crate::platform::folder_picker::pick_folder_async(
            "选择工作目录",
        ));
        self.folder_picker_edit = edit;
        let this = cx.entity().clone();
        cx.spawn_in(window, async move |_weak_view, cx| loop {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(100))
                .await;
            let done = cx
                .update(|window, cx| {
                    let mut done = false;
                    let _ = this.update(cx, |view, cx| {
                        let Some(rx) = &view.folder_picker_rx else {
                            done = true;
                            return;
                        };
                        match rx.try_recv() {
                            Ok(picked) => {
                                view.folder_picker_rx = None;
                                if let Some(path) = picked {
                                    let input = if view.folder_picker_edit {
                                        view.edit_workdir.clone()
                                    } else {
                                        view.new_workdir.clone()
                                    };
                                    input.update(cx, |state, cx| {
                                        state.set_value(path, window, cx);
                                    });
                                }
                                cx.notify();
                                done = true;
                            }
                            Err(std::sync::mpsc::TryRecvError::Empty) => {}
                            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                                view.folder_picker_rx = None;
                                done = true;
                            }
                        }
                    });
                    done
                })
                .unwrap_or(true);
            if done {
                break;
            }
        })
        .detach();
    }

    fn refresh_new_name(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let current = self.new_name.read(cx).value().to_string();
        if !current.is_empty() && current != self.auto_name {
            return;
        }
        let client = self.new_client.read(cx).value().trim().to_string();
        let language = self.new_language.read(cx).value().trim().to_string();
        let mut parts = vec![self.new_base_name.clone()];
        if !client.is_empty() {
            parts.push(client);
        }
        if !language.is_empty() {
            parts.push(language);
        }
        let next = parts.join("_");
        if next == current {
            return;
        }
        self.auto_name = next.clone();
        self.new_name.update(cx, |state, cx| {
            state.set_value(next, window, cx);
        });
    }

    fn open_new_dialog(&self, window: &mut Window, cx: &mut Context<Self>) {
        let this = cx.entity().clone();
        let name = self.new_name.clone();
        let client = self.new_client.clone();
        let user = self.new_user.clone();
        let workdir = self.new_workdir.clone();
        let folder_select = self.new_folder_select.clone();
        let base_select = self.new_base_select.clone();
        let trace_select = self.new_trace_select.clone();
        let winmax_select = self.new_winmax_select.clone();
        let language_select = self.new_language_select.clone();
        let reuse = self.new_reuse.clone();
        let muted = cx.theme().muted_foreground;

        window.open_dialog(cx, move |dialog, _window, cx| {
            let confirm = this.clone();
            let reuse_value = *reuse.read(cx);
            let reuse_click = reuse.clone();
            let row = |label: String, content: AnyElement| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .pb(px(10.))
                    .child(
                        div()
                            .w(px(72.))
                            .text_size(px(12.))
                            .text_color(muted)
                            .child(label),
                    )
                    .child(div().flex_1().child(content))
            };

            dialog
                .title(t!("app.new_shortcut").to_string())
                .w(px(560.))
                .child(row(
                    t!("app.folder").to_string(),
                    Select::new(&folder_select).small().into_any_element(),
                ))
                .child(row(
                    t!("app.base_system").to_string(),
                    Select::new(&base_select).small().into_any_element(),
                ))
                .child(row(
                    t!("app.client").to_string(),
                    Input::new(&client).into_any_element(),
                ))
                .child(row(
                    t!("app.name").to_string(),
                    Input::new(&name).into_any_element(),
                ))
                .child(row(
                    t!("app.user").to_string(),
                    Input::new(&user).into_any_element(),
                ))
                .child(row(
                    t!("app.language").to_string(),
                    Select::new(&language_select).small().into_any_element(),
                ))
                .child(row(
                    t!("app.work_dir").to_string(),
                    {
                        let pick_this = this.clone();
                        div()
                            .flex()
                            .gap(px(8.))
                            .child(div().flex_1().child(Input::new(&workdir)))
                            .child(
                                Button::new("pick-new-workdir")
                                    .label("…")
                                    .on_click(move |_, window, cx| {
                                        pick_this.update(cx, |view, cx| {
                                            view.pick_workdir(false, window, cx);
                                        });
                                    }),
                            )
                            .into_any_element()
                    },
                ))
                .child(row(
                    t!("app.connection").to_string(),
                    Checkbox::new("new-reuse")
                        .checked(reuse_value)
                        .label(t!("app.reuse").to_string())
                        .on_click(move |checked: &bool, _window, cx| {
                            reuse_click.update(cx, |value, cx| {
                                *value = *checked;
                                cx.notify();
                            });
                        })
                        .into_any_element(),
                ))
                .child(row(
                    t!("app.trace_level").to_string(),
                    Select::new(&trace_select).small().into_any_element(),
                ))
                .child(row(
                    t!("app.window_size").to_string(),
                    Select::new(&winmax_select).small().into_any_element(),
                ))
                .footer(
                    div()
                        .flex()
                        .flex_row()
                        .gap(px(8.))
                        .child(
                            Button::new("new-cancel")
                                .label(t!("app.cancel").to_string())
                                .on_click(|_, window, cx| {
                                    window.close_dialog(cx);
                                }),
                        )
                        .child(
                            Button::new("new-save")
                                .primary()
                                .label(t!("app.save").to_string())
                                .on_click(move |_, window, cx| {
                                    // 失败（客户端为空 / SAP GUI 在跑 / 写文件失败）时保留弹框，
                                    // 让用户看到状态栏的原因。
                                    let saved = confirm
                                        .update(cx, |view, cx| view.confirm_new_shortcut(window, cx));
                                    if saved {
                                        window.close_dialog(cx);
                                    }
                                }),
                        ),
                )
        });
    }

    /// 空状态：一条快捷方式都没有时引导用户新建。
    fn empty_state(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(10.))
            .py(px(80.))
            .w_full()
            .child(
                Icon::new(IconName::Plus)
                    .size_8()
                    .text_color(muted),
            )
            .child(
                div()
                    .text_size(px(14.))
                    .text_color(cx.theme().foreground)
                    .child(t!("app.empty_title").to_string()),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(muted)
                    .child(t!("app.empty_hint").to_string()),
            )
            .child(
                Button::new("empty-new-shortcut")
                    .primary()
                    .label(t!("app.new_shortcut").to_string())
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.begin_new_shortcut(window, cx);
                    })),
            )
    }

    fn cards_panel(&self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        // 顶栏浮层不占布局，所以内容从窗口顶端开始，靠 padding 让开按钮。
        let mut content = div()
            .flex()
            .flex_col()
            .gap(px(2.))
            .pt(px(2.))
            .px(px(14.))
            .pb(px(40.))
            .w_full();

        if let Some(landscape) = &self.landscape {
            // 一条快捷方式都没有：直接给空状态，别让用户对着空白发呆。
            if !landscape
                .services
                .iter()
                .any(|service| service.is_shortcut())
            {
                return div()
                    .id("cards")
                    .flex_1()
                    .h_full()
                    .min_w_0()
                    .overflow_y_scroll()
                    .child(content.child(self.empty_state(cx)));
            }

            // 最近登录 = 固定项 + 最近记录（去重、跳过已不存在的）
            let mut recent_ids: Vec<String> = Vec::new();
            for id in self
                .settings
                .window
                .pinned
                .iter()
                .chain(self.settings.window.recent.iter())
            {
                if !recent_ids.contains(id) && landscape.service(id).is_some() {
                    recent_ids.push(id.clone());
                }
            }
            if !recent_ids.is_empty() {
                content = content
                    .child(section_title("最近登录", muted))
                    .child(card_grid(
                        recent_ids
                            .iter()
                            .filter_map(|id| landscape.service(id))
                            // 只展示快捷方式：工作区里直接挂的基础系统与未配置的
                            // 裸 Reference 都不是可登录的快捷方式。
                            .filter(|service| service.is_shortcut())
                            .map(|service| self.shortcut_card(service, landscape, true, cx))
                            .collect(),
                    ));
            }

            for group in card_groups(landscape) {
                content = content
                    .child(section_title(&group.name, muted))
                    .child(card_grid(
                        group
                            .entries
                            .iter()
                            .filter_map(|id| landscape.service(id))
                            // 只展示快捷方式：工作区里直接挂的基础系统与未配置的
                            // 裸 Reference 都不是可登录的快捷方式。
                            .filter(|service| service.is_shortcut())
                            .map(|service| self.shortcut_card(service, landscape, false, cx))
                            .collect(),
                    ));
            }
        }

        div()
            .id("cards")
            .flex_1()
            .h_full()
            .min_w_0()
            .overflow_y_scroll()
            .child(content)
    }

    fn shortcut_card(
        &self,
        service: &Service,
        landscape: &Landscape,
        in_recent: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = self.selected.as_deref() == Some(service.uuid.as_str());
        let muted = cx.theme().muted_foreground;

        let base = landscape.base_of(service);
        let sid = base.map(|base| base.systemid.clone()).unwrap_or_default();
        let server = base
            .map(|base| landscape.server_label(base))
            .unwrap_or_default();
        let host = server.split(':').next().unwrap_or(&server).to_string();
        let instance = base
            .map(|base| landscape.instance_label(base))
            .filter(|instance| !instance.is_empty())
            .unwrap_or_else(|| "—".into());
        let user = if service.user.is_empty() {
            "—".to_string()
        } else {
            service.user.clone()
        };
        let language = if service.language.is_empty() {
            self.settings.sap.default_language.clone()
        } else {
            service.language.clone()
        };
        let client = if service.client.trim().is_empty() {
            "—".to_string()
        } else {
            service.client.clone()
        };

        let dark = cx.theme().is_dark();
        let sid_index = match sid.chars().next() {
            Some('D') | Some('d') => 0,
            Some('Q') | Some('q') => 2,
            Some('P') | Some('p') => 3,
            _ => 5,
        };
        let client_index = self.client_colors.get(client.trim()).copied().unwrap_or(0);
        let pinned = self.settings.window.pinned.contains(&service.uuid);

        let uuid = service.uuid.clone();
        let uuid_hover = uuid.clone();
        let uuid_click = uuid.clone();
        let uuid_double = uuid.clone();
        let name = self
            .settings
            .aliases
            .get(&service.uuid)
            .cloned()
            .unwrap_or_else(|| service.name.clone());

        // 状态灯：上次结果优先，其次是「有没有保存密码」。
        let light = if let Some(ok) = self.last_result.get(&service.uuid) {
            if *ok {
                cx.theme().success
            } else {
                cx.theme().danger
            }
        } else if self.password_ready.contains(&service.uuid) {
            cx.theme().info
        } else {
            muted.opacity(0.4)
        };

        let card = div()
            .id(element_id("card", &uuid))
            .w(px(200.))
            .h(px(96.))
            .cursor_pointer()
            .flex()
            .flex_col()
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.select(uuid_click.clone(), window, cx)
            }))
            .on_double_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.select(uuid_double.clone(), window, cx);
                this.login(window, cx);
            }))
            .on_hover(cx.listener(move |this, hovered: &bool, _window, cx| {
                let target = if *hovered { Some(uuid_hover.clone()) } else { None };
                if this.hovered_card != target {
                    this.hovered_card = target;
                    cx.notify();
                }
            }))
            .child(
                Card::new(element_id("card-surface", &uuid))
                    .selected(selected)
                    .hovered(self.hovered_card.as_deref() == Some(uuid.as_str()))
                    .flex_1()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(
                                div()
                                    .w(px(6.))
                                    .h(px(6.))
                                    .flex_shrink_0()
                                    .rounded_full()
                                    .bg(light),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(px(14.))
                                    .child(name),
                            )
                            .child(if pinned {
                                Icon::new(IconName::Star)
                                    .size_3()
                                    .text_color(muted)
                                    .into_any_element()
                            } else {
                                div().into_any_element()
                            }),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(10.))
                            .text_color(muted)
                            .child(format!("{language} · {instance} · {host}")),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(px(10.))
                                    .text_color(muted)
                                    .child(user),
                            )
                            .child(soft_tag(sid.clone(), sid_index, dark))
                            .child(soft_tag(client.clone(), client_index, dark)),
                    ),
            );

        // 右键菜单：最近区与分组区的「删除」语义不同。
        let pinned = self.settings.window.pinned.contains(&service.uuid);
        let this = cx.entity().clone();
        let uuid_menu = uuid.clone();
        card.context_menu(move |menu, _window, _cx| {
            if in_recent {
                let login_this = this.clone();
                let password_this = this.clone();
                let edit_this = this.clone();
                let alias_this = this.clone();
                let pin_this = this.clone();
                let up_this = this.clone();
                let down_this = this.clone();
                let recent_this = this.clone();
                let uuid_password = uuid_menu.clone();
                let uuid_edit = uuid_menu.clone();
                let uuid_alias = uuid_menu.clone();
                let uuid_pin = uuid_menu.clone();
                let uuid_up = uuid_menu.clone();
                let uuid_down = uuid_menu.clone();
                let uuid_recent = uuid_menu.clone();

                let uuid_login = uuid_menu.clone();
                menu.item(
                    PopupMenuItem::new(t!("app.login").to_string()).on_click(move |_, window, cx| {
                        let uuid = uuid_login.clone();
                        login_this.update(cx, |view, cx| {
                            view.select(uuid, window, cx);
                            view.login(window, cx);
                        });
                    }),
                )
                .item(
                    PopupMenuItem::new(t!("app.set_password_menu").to_string()).on_click(move |_, window, cx| {
                        let uuid = uuid_password.clone();
                        password_this.update(cx, |view, cx| {
                            view.begin_set_password(uuid, window, cx);
                        });
                    }),
                )
                .item(
                    PopupMenuItem::new(t!("app.edit_params").to_string()).on_click(move |_, window, cx| {
                        let uuid = uuid_edit.clone();
                        edit_this.update(cx, |view, cx| {
                            view.begin_edit(uuid, window, cx);
                        });
                    }),
                )
                .item(
                    PopupMenuItem::new(t!("app.set_alias").to_string()).on_click(move |_, window, cx| {
                        let uuid = uuid_alias.clone();
                        alias_this.update(cx, |view, cx| {
                            view.begin_set_alias(uuid, window, cx);
                        });
                    }),
                )
                .separator()
                .item(
                    PopupMenuItem::new(if pinned { t!("app.unpin").to_string() } else { t!("app.pin").to_string() }).on_click(
                        move |_, _window, cx| {
                            pin_this.update(cx, |view, cx| {
                                view.toggle_pin(&uuid_pin);
                                cx.notify();
                            });
                        },
                    ),
                )
                .separator()
                .item(PopupMenuItem::new(t!("app.move_up").to_string()).on_click(move |_, _window, cx| {
                    up_this.update(cx, |view, cx| {
                        view.move_recent(&uuid_up, -1);
                        cx.notify();
                    });
                }))
                .item(PopupMenuItem::new(t!("app.move_down").to_string()).on_click(move |_, _window, cx| {
                    down_this.update(cx, |view, cx| {
                        view.move_recent(&uuid_down, 1);
                        cx.notify();
                    });
                }))
                .separator()
                .item(
                    PopupMenuItem::new(t!("app.remove_recent").to_string()).on_click(move |_, _window, cx| {
                        recent_this.update(cx, |view, cx| {
                            view.remove_recent(&uuid_recent);
                            cx.notify();
                        });
                    }),
                )
            } else {
                let login_this = this.clone();
                let alias_this = this.clone();
                let password_this = this.clone();
                let edit_this = this.clone();
                let pin_this = this.clone();
                let delete_this = this.clone();
                let uuid_password = uuid_menu.clone();
                let uuid_alias = uuid_menu.clone();
                let uuid_edit = uuid_menu.clone();
                let uuid_pin = uuid_menu.clone();
                let uuid_delete = uuid_menu.clone();

                let uuid_login = uuid_menu.clone();
                menu.item(
                    PopupMenuItem::new(t!("app.login").to_string()).on_click(move |_, window, cx| {
                        let uuid = uuid_login.clone();
                        login_this.update(cx, |view, cx| {
                            view.select(uuid, window, cx);
                            view.login(window, cx);
                        });
                    }),
                )
                .item(
                    PopupMenuItem::new(t!("app.set_password_menu").to_string()).on_click(move |_, window, cx| {
                        let uuid = uuid_password.clone();
                        password_this.update(cx, |view, cx| {
                            view.begin_set_password(uuid, window, cx);
                        });
                    }),
                )
                .item(
                    PopupMenuItem::new(t!("app.edit_params").to_string()).on_click(move |_, window, cx| {
                        let uuid = uuid_edit.clone();
                        edit_this.update(cx, |view, cx| {
                            view.begin_edit(uuid, window, cx);
                        });
                    }),
                )
                .item(
                    PopupMenuItem::new(t!("app.set_alias").to_string()).on_click(move |_, window, cx| {
                        let uuid = uuid_alias.clone();
                        alias_this.update(cx, |view, cx| {
                            view.begin_set_alias(uuid, window, cx);
                        });
                    }),
                )
                .separator()
                .item(
                    PopupMenuItem::new(if pinned { t!("app.unpin").to_string() } else { t!("app.pin").to_string() }).on_click(
                        move |_, _window, cx| {
                            pin_this.update(cx, |view, cx| {
                                view.toggle_pin(&uuid_pin);
                                cx.notify();
                            });
                        },
                    ),
                )
                .separator()
                .item(
                    PopupMenuItem::new(t!("app.delete").to_string()).on_click(move |_, window, cx| {
                        let uuid = uuid_delete.clone();
                        delete_this.update(cx, |view, cx| {
                            view.ask_delete(uuid, window, cx);
                        });
                    }),
                )
            }
        })
        .into_any_element()
    }

    /// 首次登录的密码框：标准 Dialog，遮罩、焦点与 Esc 关闭都由组件接管。
    fn open_password_dialog(&self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self
            .password_target
            .as_ref()
            .and_then(|uuid| self.landscape.as_ref()?.service(uuid))
            .map(|service| service.name.clone())
            .unwrap_or_default();
        let this = cx.entity().clone();
        let username_input = self.username_input.clone();
        let password_input = self.password_input.clone();

        window.open_dialog(cx, move |dialog, _window, cx| {
            let confirm = this.clone();
            dialog
                .title(name.clone())
                .w(px(420.))
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(cx.theme().muted_foreground)
                        .child(t!("app.first_login_hint").to_string()),
                )
                .child(div().mt(px(8.)).child(Input::new(&username_input)))
                .child(div().mt(px(8.)).child(Input::new(&password_input).mask_toggle()))
                .footer(
                    div()
                        .flex()
                        .flex_row()
                        .gap(px(8.))
                        .child(
                            Button::new("password-cancel")
                                .label(t!("app.cancel").to_string())
                                .on_click(|_, window, cx| {
                                    window.close_dialog(cx);
                                }),
                        )
                        .child(
                            Button::new("password-ok")
                                .primary()
                                .label(t!("app.login").to_string())
                                .on_click(move |_, window, cx| {
                                    confirm.update(cx, |view, cx| {
                                        view.confirm_password(window, cx);
                                    });
                                    window.close_dialog(cx);
                                }),
                        ),
                )
        });
    }
}

/// 语言下拉选项：代码 名称，名称随界面语言切换（T002 的 LAISO 代码 + T002T 名称）。
fn language_options(locale_en: bool) -> Vec<String> {
    const NAMES: [(&str, &str, &str); 20] = [
        ("ZH", "中文", "Chinese"),
        ("EN", "英语", "English"),
        ("DE", "德语", "German"),
        ("FR", "法语", "French"),
        ("JA", "日语", "Japanese"),
        ("KO", "韩语", "Korean"),
        ("RU", "俄语", "Russian"),
        ("ES", "西班牙语", "Spanish"),
        ("PT", "葡萄牙语", "Portuguese"),
        ("IT", "意大利语", "Italian"),
        ("NL", "荷兰语", "Dutch"),
        ("PL", "波兰语", "Polish"),
        ("TR", "土耳其语", "Turkish"),
        ("CS", "捷克语", "Czech"),
        ("HU", "匈牙利语", "Hungarian"),
        ("SV", "瑞典语", "Swedish"),
        ("DA", "丹麦语", "Danish"),
        ("FI", "芬兰语", "Finnish"),
        ("NO", "挪威语", "Norwegian"),
        ("ZF", "繁体中文", "Traditional Chinese"),
    ];
    NAMES
        .iter()
        .map(|(code, zh, en)| {
            let name = if locale_en { en } else { zh };
            format!("{code} {name}")
        })
        .collect()
}

fn folder_options(landscape: &Landscape) -> Vec<(String, String)> {
    fn walk(nodes: &[TreeNode], path: &str, out: &mut Vec<(String, String)>) {
        for node in nodes {
            if let TreeNode::Folder {
                uuid,
                name,
                children,
            } = node
            {
                let full = if path.is_empty() {
                    name.clone()
                } else {
                    format!("{path} / {name}")
                };
                out.push((uuid.clone(), full.clone()));
                walk(children, &full, out);
            }
        }
    }

    let mut out = Vec::new();
    walk(&landscape.tree, "", &mut out);
    out
}

struct CardGroup {
    name: String,
    entries: Vec<String>,
}

/// 卡片分组：一个文件夹里的快捷方式自成一组；子文件夹各成一组，标题带父路径。
fn card_groups(landscape: &Landscape) -> Vec<CardGroup> {
    fn walk(nodes: &[TreeNode], path: &str, out: &mut Vec<CardGroup>) {
        for node in nodes {
            let TreeNode::Folder {
                uuid: _,
                name,
                children,
            } = node
            else {
                continue;
            };
            let full = if path.is_empty() {
                name.clone()
            } else {
                format!("{path} / {name}")
            };
            let entries: Vec<String> = children
                .iter()
                .filter_map(|child| match child {
                    TreeNode::Entry { service_id, .. } => Some(service_id.clone()),
                    TreeNode::Folder { .. } => None,
                })
                .collect();
            if !entries.is_empty() {
                out.push(CardGroup {
                    name: full.clone(),
                    entries,
                });
            }
            walk(children, &full, out);
        }
    }

    // 顶层直接挂的条目（没有文件夹）也要显示：SAP 允许把快捷方式直接放在 Workspace 下，
    // 只遍历文件夹的话，这种布局会一张卡都不显示。
    let loose: Vec<String> = landscape
        .tree
        .iter()
        .filter_map(|node| match node {
            TreeNode::Entry { service_id, .. } => Some(service_id.clone()),
            TreeNode::Folder { .. } => None,
        })
        .collect();

    let mut out = Vec::new();
    if !loose.is_empty() {
        out.push(CardGroup {
            name: "未分类".to_string(),
            entries: loose,
        });
    }
    walk(&landscape.tree, "", &mut out);
    out
}

/// 卡片徽标的柔和配色：浅色主题淡底深字，深色主题深底亮字。
/// 刻意不用主题的语义色——那些是给状态/警告用的，用在几十张卡片上太扎眼。
fn soft_tag_colors(index: usize, dark: bool) -> (gpui_kit::Rgba, gpui_kit::Rgba) {
    const LIGHT: [(u32, u32); 6] = [
        (0xE3EDFB, 0x1F4E8C),
        (0xE2F2E7, 0x1E6B3A),
        (0xFBF0DC, 0x7A5410),
        (0xF6E4E7, 0x8E2F3C),
        (0xEDE7F8, 0x553388),
        (0xE1F1F1, 0x155F5F),
    ];
    const DARK: [(u32, u32); 6] = [
        (0x1B2A3D, 0xA8C7F0),
        (0x1B2E23, 0xA8DDB8),
        (0x322A1A, 0xE0C080),
        (0x352124, 0xE8A0A8),
        (0x2A2238, 0xC0A8E8),
        (0x1A2E2E, 0x90D0D0),
    ];
    let (background, foreground) = if dark { DARK[index % 6] } else { LIGHT[index % 6] };
    (rgb(background), rgb(foreground))
}

fn soft_tag(label: String, index: usize, dark: bool) -> impl IntoElement {
    let (background, foreground) = soft_tag_colors(index, dark);
    div()
        .h(px(15.))
        .px(px(5.))
        .rounded(px(4.))
        .bg(background)
        .text_color(foreground)
        .text_size(px(9.5))
        .flex()
        .items_center()
        .child(label)
}

fn section_title(name: &str, muted: Hsla) -> impl IntoElement {
    div()
        .mt(px(16.))
        .mb(px(8.))
        .px(px(2.))
        .child(
            Label::new(name.to_string())
                .text_size(px(12.5))
                .text_color(muted),
        )
}

/// 设置里的材质名 → 窗口背景外观。Mica 只有 Win11 有，老系统上按不透明处理。
pub fn material_appearance(material: &str) -> gpui_kit::WindowBackgroundAppearance {
    use gpui_kit::WindowBackgroundAppearance;

    match material {
        "mica" if crate::platform::system::supports_mica() => WindowBackgroundAppearance::MicaBackdrop,
        "acrylic" => WindowBackgroundAppearance::Blurred,
        _ => WindowBackgroundAppearance::Opaque,
    }
}

/// 主题偏好 + 系统外观 → 实际档位。空串与认不出的取值一律按跟随系统处理，
/// 这样手工改坏的配置不会把界面卡在某个固定档上。
fn theme_mode(
    preference: &str,
    appearance: gpui_kit::WindowAppearance,
) -> gpui_kit::component::ThemeMode {
    use gpui_kit::WindowAppearance;
    use gpui_kit::component::ThemeMode;

    match preference {
        "light" => ThemeMode::Light,
        "dark" => ThemeMode::Dark,
        // macOS 的 Vibrant 档也是深色，只认 Dark 会让那边少跟一种情况。
        _ if matches!(
            appearance,
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        ) =>
        {
            ThemeMode::Dark
        }
        _ => ThemeMode::Light,
    }
}

/// 每行固定 4 张：窗口拉宽时让卡片变宽，而不是塞进更多列。
fn card_grid(cards: Vec<AnyElement>) -> impl IntoElement {
    // 所有行左对齐，居中靠内容区的左右内边距。
    div()
        .flex()
        .flex_wrap()
        .justify_start()
        .gap(px(10.))
        .children(cards)
}

fn count_entries(nodes: &[TreeNode]) -> usize {
    nodes
        .iter()
        .map(|node| match node {
            TreeNode::Folder { children, .. } => count_entries(children),
            TreeNode::Entry { .. } => 1,
        })
        .sum()
}

fn count_folders(nodes: &[TreeNode]) -> usize {
    nodes
        .iter()
        .map(|node| match node {
            TreeNode::Folder { children, .. } => 1 + count_folders(children),
            TreeNode::Entry { .. } => 0,
        })
        .sum()
}

fn load_landscape() -> Option<Landscape> {
    let path = crate::landscape::locate::landscape_path()?;
    let Some(landscape) = crate::landscape::parse::parse_file(&path) else {
        return None;
    };
    let _ = crate::core::cache::save(&path, &landscape);
    Some(landscape)
}

/// 行 id 必须由领域数据（uuid）派生，不能用下标：过滤/展开后按下标会串行。
fn element_id(prefix: &'static str, value: &str) -> ElementId {
    use std::hash::{Hash, Hasher};

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    ElementId::from((prefix, hasher.finish()))
}

impl Render for RootView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // 深色下外壳用比纯黑浅一档的灰。
        let mut shell: gpui_kit::Hsla = if cx.theme().is_dark() {
            gpui_kit::rgb(0x161618).into()
        } else {
            cx.theme().sidebar
        };
        // 有材质时外壳必须半透明，否则底色把窗口背后的 Mica 全挡住了。
        // 浅色 Mica 的底色本来就接近白，透明度给低了等于没有，所以浅色档压得更狠。
        if !matches!(
            material_appearance(&self.settings.general.material),
            gpui_kit::WindowBackgroundAppearance::Opaque
        ) {
            shell.a = if cx.theme().is_dark() { 0.68 } else { 0.62 };
        }

        // 顶栏浮层不占布局，所以内容从窗口顶端开始，靠 padding 让开按钮。
        let root = div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(shell)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_row()
                    .child(self.cards_panel(window, cx)),
            )
            .child(self.header(cx));

        // 弹框遮罩一律拦下滚轮，避免滚动穿透到卡片区。

        root
    }
}

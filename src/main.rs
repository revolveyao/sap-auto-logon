//! SAP Auto Logon —— 读 SAP GUI 的 Landscape，双击快捷方式直接登录。
//!
//! 启动顺序对次序敏感：日志要在数据目录定下来之后、配置加载之前初始化
//! （日志开关本身就是配置项）。

#![cfg_attr(all(not(test), target_os = "windows"), windows_subsystem = "windows")]

mod core;
mod landscape;
mod platform;
mod services;
mod ui;

use gpui_kit::prelude::*;
use gpui_kit::*;

// 应用文案与组件文案走同一套 rust-i18n；`extend!` 让 gpui-kit 的内置英文
// 在语言切换后逐键被 locales/ui.yml 覆盖。
rust_i18n::i18n!("locales", fallback = "en");

fn main() {
    core::paths::init();
    services::logging::init();

    // 单实例：第二个实例直接退出，避免两个托盘图标抢同一个 Landscape。
    let _instance = match platform::single_instance::acquire() {
        Some(guard) => guard,
        None => {
            log::info!("another instance is running; exiting");
            return;
        }
    };

    let settings = core::settings::AppSettings::load();
    log::info!("SAP Auto Logon {} starting", env!("CARGO_PKG_VERSION"));

    // 配置与注册表可能不一致（搬过目录），启动时对齐一次。
    if let Err(error) = platform::autostart::apply(settings.general.autostart) {
        log::warn!("对齐开机自启失败：{error}");
    }

    // 托盘菜单在创建时就用当前语言，之后再切语言需要重建菜单，不在本轮范围。
    rust_i18n::set_locale(settings.locale());

    let tray = match platform::tray::Tray::create() {
        Ok(tray) => Some(tray),
        Err(error) => {
            log::error!("托盘不可用：{error}");
            None
        }
    };

    gpui_kit::application()
        .with_assets(gpui_kit::assets::AllAssets)
        .run(move |cx: &mut App| {
            // 组件文案的扩展必须在 `init` 之前注册；`extend!` 只收标识符，
            // 所以先给 gpui-kit 的组件层起一个本地别名。
            use gpui_kit::component as gpui_component;
            rust_i18n::extend!(gpui_component);
            gpui_kit::init(cx);

            let locale = settings.locale();
            gpui_kit::component::set_locale(locale);
            rust_i18n::set_locale(locale);

            // 根表面的底色由 gpui-component 的插件铺；有材质时必须先把它摘掉，
            // 否则那层不透明底色会把云母整个盖住。必须在 `init` 之后注册，
            // 才能盖过组件自己注册的插件。
            ui::surface::set_opaque(matches!(
                background_appearance(&settings.general.material),
                WindowBackgroundAppearance::Opaque
            ));
            ui::surface::install(cx);

            let settings_for_view = settings.clone();
            let options = window_options(&settings);
            // 材质生效与否肉眼不好判断（浅色 Mica 本来就淡），留一行日志便于核对。
            log::info!(
                "窗口材质 {} → {:?}（Windows build {:?}）",
                settings.general.material,
                background_appearance(&settings.general.material),
                platform::system::windows_build()
            );
            gpui_kit::open_window(options, cx, move |window, cx| {
                let tray = tray;
                cx.new(|cx| {
                    ui::root::RootView::new(settings_for_view.clone(), tray, window, cx)
                })
            })
            .expect("failed to open the main window");
        });
}

/// 窗口材质：Mica 只有 Win11 有；亚克力在 Win10 可用，但要实时采样，拖动会掉帧。
fn background_appearance(material: &str) -> WindowBackgroundAppearance {
    ui::root::material_appearance(material)
}

/// 主窗口：系统标题栏隐藏，由界面自绘（拖动、窗口按钮都自己做）。
fn window_options(settings: &core::settings::AppSettings) -> WindowOptions {
    let bounds = settings
        .window
        .bounds
        .map(|[x, y, width, height]| {
            Bounds::new(point(px(x), px(y)), size(px(width), px(height)))
        })
        .unwrap_or_else(|| {
            // 刚好一行 4 张卡：4×200 卡片 + 3×10 间距 + 左右各 14 内边距，再留一点滚动条余量。
            Bounds::new(point(px(120.), px(80.)), size(px(866.), px(760.)))
        });

    WindowOptions {
        window_background: background_appearance(&settings.general.material),
        titlebar: Some(TitlebarOptions {
            title: None,
            appears_transparent: true,
            ..Default::default()
        }),
        app_owns_titlebar_drag: true,
        // 最小宽度 = 一张卡片（200）+ 左右内边距（28）+ 滚动条余量。
        window_min_size: Some(size(px(260.), px(420.))),
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        ..WindowOptions::default()
    }
}

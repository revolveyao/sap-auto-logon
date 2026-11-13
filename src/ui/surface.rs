//! 根表面：有窗口材质时把底色摘掉，云母才透得上来。
//!
//! gpui-component 在 `init` 时注册的 `WindowState` 插件会给根表面刷一层主题底色
//! （alpha = 1）。这一层是 `base::Root` 的表面，在我们的视图之下、只压在清屏色
//! 之上——它不透明，整条交换链的 alpha 就被顶成 1，DirectComposition 交给 DWM 的
//! 是一张不透明表面，云母无处可透。界面自己改不到这一层，只能借插件钩子改。

use std::sync::atomic::{AtomicBool, Ordering};

use gpui_kit::gpui::{Div, Refineable as _, Stateful, StyleRefinement, transparent_black};
use gpui_kit::prelude::*;
use gpui_kit::*;

/// 根表面是否铺不透明底色。材质生效时置 `false`。
static OPAQUE: AtomicBool = AtomicBool::new(true);

/// 跟随材质设置切换。改完要让根重新渲染，新样式才会生效。
pub fn set_opaque(opaque: bool) {
    OPAQUE.store(opaque, Ordering::Relaxed);
}

/// 注册到 `base::Root`。必须排在 gpui-component 自己的插件之后，
/// 让这里的 refine 后手覆盖它铺的底色。
pub fn install(cx: &mut App) {
    gpui_kit::base::Root::register_plugin(cx, |_, _| Surface);
}

struct Surface;

impl Render for Surface {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        // 只借它的 style 钩子，自己不出任何东西。
        div()
    }
}

impl gpui_kit::base::RootPlugin for Surface {
    fn style(&self, surface: &mut Stateful<Div>, _window: &mut Window, _cx: &mut App) {
        if OPAQUE.load(Ordering::Relaxed) {
            return;
        }
        surface
            .style()
            .refine(&StyleRefinement::default().bg(transparent_black()));
    }
}

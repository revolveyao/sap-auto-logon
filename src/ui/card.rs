//! 卡片容器：GPUI Kit `GroupBox` 的可选中变体。
//!
//! `GroupBox` 只有视觉（没有 hover / 选中语义），而卡片需要「可选中、可悬停」的
//! 外观；这里照 `GroupBox` 的结构做一个 Card：样式收敛在组件里，点击、双击、
//! 右键菜单等交互仍由外层元素挂载。

use gpui_kit::base::motion::{transition, Transition};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{
    AnyElement, App, ElementId, InteractiveElement as _, IntoElement, ParentElement, RenderOnce,
    StyleRefinement, Styled, Window, div, px,
};

#[derive(IntoElement)]
pub struct Card {
    id: ElementId,
    selected: bool,
    hovered: bool,
    style: StyleRefinement,
    children: Vec<AnyElement>,
}

impl Card {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            selected: false,
            hovered: false,
            style: StyleRefinement::default(),
            children: Vec::new(),
        }
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn hovered(mut self, hovered: bool) -> Self {
        self.hovered = hovered;
        self
    }

    pub fn child(mut self, child: impl IntoElement) -> Self {
        self.children.push(child.into_any_element());
        self
    }
}

impl Styled for Card {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for Card {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for Card {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let selected = self.selected;
        let border = theme.border;
        // 选中用信息蓝，不用 primary——深色主题里 primary 接近纯黑，边框像画框。
        let accent = theme.info;
        // 深色主题的默认背景偏纯黑，用比 Mica 略亮一档的深灰（#2A2A2C）：
        // 窗口底是云母的深灰，卡片再暗就糊在一起了。
        let background = if theme.is_dark() {
            gpui_kit::rgb(0x2A2A2C).into()
        } else {
            theme.background
        };
        let selection = theme.selection;
        let hover = theme.muted;

        // 目标驱动过渡：悬停进/出时边框透明度与底色渐变，160ms。
        let hover_t: f32 = transition(
            self.id.clone(),
            if self.hovered { 1.0 } else { 0.0 },
            Transition::new(std::time::Duration::from_millis(160)),
            window,
            cx,
        );

        let border_color = if selected {
            accent
        } else {
            border.opacity(0.55 + 0.45 * hover_t)
        };
        let surface = if selected {
            selection
        } else {
            background
        };

        div()
            .id(self.id)
            .flex()
            .flex_col()
            .gap(px(6.))
            .px(px(12.))
            .py(px(10.))
            .rounded(px(10.))
            .border_1()
            .border_color(border_color)
            .bg(surface)
            .cursor_pointer()
            .hover(move |style| style.bg(hover.opacity(0.08 * hover_t)))
            .children(self.children)
    }
}

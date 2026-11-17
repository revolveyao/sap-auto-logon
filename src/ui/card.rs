//! 卡片容器：GPUI Kit `GroupBox` 的可选中变体。
//!
//! `GroupBox` 只有视觉（没有 hover / 选中语义），而卡片需要「可选中、可悬停」的
//! 外观；这里照 `GroupBox` 的结构做一个 Card：样式收敛在组件里，点击、双击、
//! 右键菜单等交互仍由外层元素挂载。

use gpui_kit::base::motion::{transition, Transition};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{
    AnyElement, App, ElementId, InteractiveElement as _, IntoElement, ParentElement, RenderOnce,
    StyleRefinement, Styled, Window, div, rems,
};

#[derive(IntoElement)]
pub struct Card {
    id: ElementId,
    selected: bool,
    hovered: bool,
    /// 卡片配色，来自用户可编辑的主题。取不到就退回 gpui-kit 主题的语义色。
    palette: Option<crate::core::colors::CardPalette>,
    style: StyleRefinement,
    children: Vec<AnyElement>,
}

impl Card {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            selected: false,
            hovered: false,
            palette: None,
            style: StyleRefinement::default(),
            children: Vec::new(),
        }
    }

    /// 卡片配色（底色、边框、悬停、选中、文字）。
    pub fn palette(mut self, palette: crate::core::colors::CardPalette) -> Self {
        self.palette = Some(palette);
        self
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
        // 配色以用户主题为准；没给就沿用 gpui-kit 主题原来的语义色。
        let palette = self.palette;
        let border = palette.map_or(theme.border, |p| p.border.into());
        let accent = palette.map_or(theme.info, |p| p.selected_border.into());
        let background = palette.map_or(theme.background, |p| p.background.into());
        let selection = palette.map_or(theme.selection, |p| p.selected_background.into());
        let hover = palette.map_or(theme.muted, |p| p.hover.into());
        let foreground = palette.map_or(theme.foreground, |p| p.foreground.into());

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
            .gap(rems(0.375))
            .px(rems(0.75))
            .py(rems(0.625))
            .rounded(rems(0.625))
            .border_1()
            .border_color(border_color)
            .bg(surface)
            .text_color(foreground)
            .cursor_pointer()
            .hover(move |style| style.bg(hover.opacity(0.08 * hover_t)))
            .children(self.children)
    }
}

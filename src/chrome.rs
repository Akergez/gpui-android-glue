//! The bar across the top of the window.

use gpui_kit::component::{ActiveTheme, TitleBar, h_flex};
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, App, Hsla, Pixels, px};

/// How the bar looks. Everything left out is the theme's title bar.
#[derive(Clone, Copy, Debug, Default)]
pub struct Frame {
    pub background: Option<Hsla>,
    pub border: Option<Hsla>,
    pub foreground: Option<Hsla>,
    /// On a phone, how tall the strip is (48 px if left out).
    pub height: Option<Pixels>,
    /// On a phone, whether the strip leaves room for the status bar above it
    /// itself. Leave it off where something else already does.
    pub below_status_bar: bool,
}

/// `content` in the bar across the top: the toolkit's title bar on a desktop;
/// on a phone, which has no window to drag, minimise or close (and the
/// toolkit's bar draws the buttons for that regardless), a plain strip tall
/// enough for a finger.
pub fn title_frame(content: impl IntoElement, frame: Frame, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let background = frame.background.unwrap_or(theme.title_bar);
    let border = frame.border.unwrap_or(theme.title_bar_border);
    let foreground = frame.foreground.unwrap_or(theme.foreground);
    if cfg!(target_os = "android") {
        let top = if frame.below_status_bar {
            crate::insets::system_bars().0
        } else {
            px(0.)
        };
        h_flex()
            .flex_none()
            .mt(top)
            .h(frame.height.unwrap_or(px(48.)))
            .px_3()
            .border_b_1()
            .bg(background)
            .border_color(border)
            .text_color(foreground)
            .child(content)
            .into_any_element()
    } else {
        TitleBar::new()
            .bg(background)
            .border_color(border)
            .text_color(foreground)
            .child(content)
            .into_any_element()
    }
}

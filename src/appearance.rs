//! Light or dark as the system says, and the status bar to go with it.
//!
//! Android's window starts out calling itself light whatever the phone says,
//! and is not always told when the phone changes, so there the phone is asked.

use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::{App, Window};

/// The system's mode: the phone's on Android, the window's (or, before there
/// is one, the desktop's) elsewhere.
pub fn system_mode(window: Option<&Window>, cx: &App) -> ThemeMode {
    #[cfg(target_os = "android")]
    {
        let _ = (window, cx);
        if gpui_mobile::android::jni::query_night_mode_via_jni() {
            ThemeMode::Dark
        } else {
            ThemeMode::Light
        }
    }
    #[cfg(not(target_os = "android"))]
    {
        window
            .map(|window| window.appearance())
            .unwrap_or_else(|| cx.window_appearance())
            .into()
    }
}

/// Whether the phone has changed between light and dark since the theme
/// last followed it. Ask when the window comes back to the front, or on a
/// change of configuration (`cx.on_keyboard_layout_change`, which is how
/// gpui-mobile announces one). Never off Android: there the window is told.
pub fn behind_the_system(cx: &App) -> bool {
    #[cfg(target_os = "android")]
    {
        system_mode(None, cx).is_dark() != Theme::global(cx).mode.is_dark()
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = cx;
        false
    }
}

/// Tells the system the colour of what is under the status bar in `mode`,
/// so that the clock and the icons stand out against it: light icons on a
/// dark bar, dark on a light one. Call whenever the mode changes.
pub fn set_status_bar_for(mode: ThemeMode) {
    #[cfg(target_os = "android")]
    gpui_mobile::set_system_chrome(&gpui_mobile::SystemChromeStyle {
        status_bar_style: if mode.is_dark() {
            gpui_mobile::StatusBarContentStyle::Light
        } else {
            gpui_mobile::StatusBarContentStyle::Dark
        },
        ..Default::default()
    });
    #[cfg(not(target_os = "android"))]
    let _ = mode;
}

/// Changes the theme to `mode` if it is not in it already, and the status bar
/// with it.
pub fn change_to(mode: ThemeMode, window: Option<&mut Window>, cx: &mut App) {
    set_status_bar_for(mode);
    if Theme::global(cx).mode != mode {
        Theme::change(mode, window, cx);
    }
}

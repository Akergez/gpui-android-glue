//! How much of the top and the bottom of the screen the system draws over:
//! the status bar, and the navigation bar or the on-screen keyboard. The
//! window lies under them edge to edge.

use gpui_kit::{Pixels, px};

/// (top, bottom), in the window's logical pixels. Nothing off Android: a
/// desktop has no bars of the system's over its windows.
pub fn system_bars() -> (Pixels, Pixels) {
    #[cfg(target_os = "android")]
    {
        gpui_mobile::android::jni::platform()
            .and_then(|platform| platform.primary_window())
            .map(|window| {
                let insets = window.safe_area_insets_logical();
                (px(insets.top), px(insets.bottom))
            })
            .unwrap_or((px(0.), px(0.)))
    }
    #[cfg(not(target_os = "android"))]
    {
        (px(0.), px(0.))
    }
}

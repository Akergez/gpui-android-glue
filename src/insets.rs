//! How much of the screen the system draws over: the status bar at the top,
//! the navigation bar at the bottom, and the on-screen keyboard over that.
//! The window lies under all of them edge to edge.
//!
//! `GpuiInsets.java` reports them as the system does, and while the keyboard
//! moves it reports every frame of the move: a layout that reads them in
//! `render` follows the keyboard as it slides, and the window is drawn again
//! on every report, so nothing has to ask for it.

use gpui_kit::Pixels;

/// (top, bottom): the status bar and the navigation bar, in the window's
/// logical pixels. The keyboard is not in it, whether it is up or not; see
/// [`keyboard`]. Nothing off Android: a desktop has no bars of the system's
/// over its windows.
pub fn system_bars() -> (Pixels, Pixels) {
    #[cfg(target_os = "android")]
    {
        let reported = android::reported();
        (reported.top, reported.bars)
    }
    #[cfg(not(target_os = "android"))]
    {
        (Pixels::ZERO, Pixels::ZERO)
    }
}

/// How much of the bottom of the window the on-screen keyboard covers right
/// now, the navigation bar under it included: as tall as it is drawn, so
/// partway while it slides in or out, and zero while it is down.
pub fn keyboard() -> Pixels {
    #[cfg(target_os = "android")]
    {
        android::reported().ime
    }
    #[cfg(not(target_os = "android"))]
    {
        Pixels::ZERO
    }
}

/// Whether the keyboard is up, or on its way up: true from the start of its
/// animation in, false from the start of its animation out.
pub fn keyboard_visible() -> bool {
    #[cfg(target_os = "android")]
    {
        android::reported().visible
    }
    #[cfg(not(target_os = "android"))]
    {
        false
    }
}

/// Draws the windows again whenever the insets change. Called by
/// [`crate::init`].
pub(crate) fn init(cx: &mut gpui_kit::App) {
    #[cfg(target_os = "android")]
    android::init(cx);
    #[cfg(not(target_os = "android"))]
    let _ = cx;
}

#[cfg(target_os = "android")]
mod android {
    use std::sync::{Mutex, OnceLock};

    use gpui_kit::{App, Pixels, px};

    /// What `GpuiInsets.java` reported last, in physical pixels.
    #[derive(Clone, Copy)]
    struct Physical {
        top: i32,
        bars: i32,
        ime: i32,
        visible: bool,
    }

    /// The same in logical pixels.
    pub(super) struct Reported {
        pub top: Pixels,
        pub bars: Pixels,
        pub ime: Pixels,
        pub visible: bool,
    }

    /// Nothing until the first report, which comes with the first layout.
    static REPORTED: Mutex<Option<Physical>> = Mutex::new(None);

    pub(super) fn reported() -> Reported {
        let window = gpui_mobile::android::jni::platform().and_then(|platform| platform.primary_window());
        let reported = *REPORTED.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(physical) = reported else {
            // No report yet: what the platform layer has from the content
            // rect, which tells no keyboard from the bar.
            let insets = window.map(|window| window.safe_area_insets_logical()).unwrap_or_default();
            return Reported { top: px(insets.top), bars: px(insets.bottom), ime: px(0.), visible: false };
        };
        let scale = window.map_or(1., |window| window.scale_factor()).max(1.);
        let logical = |physical: i32| px(physical.max(0) as f32 / scale);
        Reported {
            top: logical(physical.top),
            bars: logical(physical.bars),
            ime: logical(physical.ime),
            visible: physical.visible,
        }
    }

    /// Reports from `GpuiInsets.nativeInsets`, on the system's UI thread, to
    /// the application's. One waiting is as good as many: the windows are
    /// drawn with whatever was reported last.
    fn changes() -> &'static (async_channel::Sender<()>, async_channel::Receiver<()>) {
        static CHANGES: OnceLock<(async_channel::Sender<()>, async_channel::Receiver<()>)> = OnceLock::new();
        CHANGES.get_or_init(|| async_channel::bounded(1))
    }

    pub(super) fn init(cx: &mut App) {
        let changes = changes().1.clone();
        cx.spawn(async move |cx| {
            while changes.recv().await.is_ok() {
                cx.update(|cx| {
                    for window in cx.windows() {
                        let _ = window.update(cx, |_, window, _| window.refresh());
                    }
                });
            }
        })
        .detach();
    }

    /// `GpuiInsets.nativeInsets`: the bars and the keyboard as they are on
    /// screen now.
    #[unsafe(no_mangle)]
    pub extern "C" fn Java_dev_gpui_glue_GpuiInsets_nativeInsets(
        _env: *mut std::ffi::c_void,
        _class: *mut std::ffi::c_void,
        top: i32,
        bars: i32,
        ime: i32,
        visible: u8,
    ) {
        let next = Physical { top, bars, ime, visible: visible != 0 };
        *REPORTED.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(next);
        let _ = changes().0.try_send(());
    }
}

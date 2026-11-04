//! What a gpui-kit application needs on Android beyond what gpui-mobile does,
//! in one place instead of a copy in every application.
//!
//! Every function exists on every platform. Off Android it does what the
//! desktop does, or nothing, so an application calls it without a `cfg`.
//!
//! - [`launch`]: `android_main`, the whole of it;
//! - [`keyboard`]: what a box is for, raising the keyboard on a tap, how tall
//!   it is;
//! - [`back`]: the system's back, as a stack of things to take back, with
//!   the preview of predictive back and a swipe to the right;
//! - [`edit_menu`]: the system's Cut, Copy, Paste and Select all over a
//!   selection made by touch, in place of the toolkit's menu;
//! - [`clipboard`]: the phone's clipboard, in step with the toolkit's;
//! - [`insets`]: what the system's bars and the keyboard cover, frame by
//!   frame while the keyboard moves;
//! - [`appearance`]: light or dark as the phone says, and the status bar's
//!   icons to go with it;
//! - [`open_url`], [`documents`], [`chrome`];
//! - the desktop's font for the interface on Linux, where GPUI draws with
//!   the first of its own list that is installed (see `font.rs`).
//!
//! On Android the crate also makes mimalloc the allocator: GPUI makes and
//! drops thousands of small objects on every frame, and the system's own
//! allocator (scudo) made a frame three to four times as long. An
//! application that sets a `#[global_allocator]` of its own must not depend
//! on this crate on Android.
//!
//! The Java half is the Android library in `android/` of this repository
//! (`com.akergez.gpui:android-glue`). The two halves call each other by name,
//! so they must be of the same version.

pub mod appearance;
pub mod back;
pub mod chrome;
pub mod clipboard;
pub mod documents;
pub mod edit_menu;
mod font;
pub mod insets;
pub mod keyboard;
#[cfg(target_os = "android")]
pub mod launch;
mod open_url;

#[cfg(target_os = "android")]
mod android;

pub use open_url::open_url;

#[cfg(target_os = "android")]
#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

/// Installs what has to listen for the whole run ([`back`], and on Android
/// [`edit_menu`], [`clipboard`] and [`insets`]), and gives the theme the
/// desktop's font on Linux. Call once, after `gpui_kit::init`.
pub fn init(cx: &mut gpui_kit::App) {
    back::init(cx);
    clipboard::init(cx);
    edit_menu::init(cx);
    insets::init(cx);
    font::init(cx);
}

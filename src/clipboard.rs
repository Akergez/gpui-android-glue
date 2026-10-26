//! The phone's clipboard, kept in step with the toolkit's.
//!
//! gpui-mobile keeps the toolkit's clipboard (`cx.write_to_clipboard`,
//! `cx.read_from_clipboard`) inside the process: what the application copied
//! never reached the phone, and what was copied anywhere else could not be
//! pasted in. [`pull`] brings the phone's in before a paste; [`push`] hands
//! the toolkit's out after a copy. [`crate::init`] calls them wherever it can
//! tell that one is due: the system's edit menu ([`crate::edit_menu`]),
//! Ctrl+C, Ctrl+X and Ctrl+V on a keyboard, and the window losing its focus,
//! which is when whatever the application copied some other way (a button
//! that calls `cx.write_to_clipboard`) goes out. An application calls them
//! itself only around a copy or a paste of its own that it wants to be seen
//! at once.
//!
//! The phone's clipboard is read only to paste: Android tells the person
//! every time an application reads it.
//!
//! Off Android the toolkit's clipboard is the desktop's, and both do nothing.

use std::cell::RefCell;

use gpui_kit::{App, Keystroke};

thread_local! {
    /// The text the two clipboards last had in common.
    static AGREED: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Whether the toolkit's text has to go out: it is new since the two last
/// agreed. Anything else the phone has since is newer than it.
fn to_push(ours: Option<&str>, agreed: Option<&str>) -> bool {
    ours.is_some() && ours != agreed
}

/// Brings the phone's clipboard into the toolkit's: call before a paste.
pub fn pull(cx: &mut App) {
    #[cfg(target_os = "android")]
    match gpui_mobile::packages::clipboard::get_text() {
        Ok(Some(text)) => {
            if ours(cx).as_deref() != Some(text.as_str()) {
                cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(text.clone()));
            }
            AGREED.set(Some(text));
        }
        // Nothing there, or no text: the toolkit's stays what it was.
        Ok(None) => {}
        Err(error) => tracing::warn!(%error, "could not read the phone's clipboard"),
    }
    #[cfg(not(target_os = "android"))]
    let _ = cx;
}

/// Hands the toolkit's clipboard to the phone, if it has changed since they
/// last agreed: call after a copy.
pub fn push(cx: &mut App) {
    let ours = ours(cx);
    if !AGREED.with_borrow(|agreed| to_push(ours.as_deref(), agreed.as_deref())) {
        return;
    }
    let Some(text) = ours else { return };
    #[cfg(target_os = "android")]
    match gpui_mobile::packages::clipboard::set_text(&text) {
        Ok(()) => AGREED.set(Some(text)),
        Err(error) => tracing::warn!(%error, "could not write the phone's clipboard"),
    }
    #[cfg(not(target_os = "android"))]
    let _ = text;
}

fn ours(cx: &App) -> Option<String> {
    cx.read_from_clipboard().and_then(|item| item.text())
}

pub(crate) fn init(cx: &mut App) {
    if !cfg!(target_os = "android") {
        return;
    }
    // A paste from a keyboard reads the toolkit's clipboard as it is
    // dispatched: the phone's has to be in it before.
    cx.intercept_keystrokes(|event, _, cx| {
        if is_secondary(&event.keystroke, &["v"]) {
            pull(cx);
        }
    })
    .detach();
    cx.observe_keystrokes(|event, _, cx| {
        if is_secondary(&event.keystroke, &["c", "x"]) {
            push(cx);
        }
    })
    .detach();
    cx.observe_new(|_: &mut gpui_kit::base::Root, window, cx| {
        let Some(window) = window else { return };
        cx.observe_window_activation(window, |_, window, cx| {
            if !window.is_window_active() {
                push(cx);
            }
        })
        .detach();
    })
    .detach();
}

/// Whether `keystroke` is Ctrl (Cmd) with one of `keys`, Shift or not.
fn is_secondary(keystroke: &Keystroke, keys: &[&str]) -> bool {
    let modifiers = &keystroke.modifiers;
    modifiers.secondary() && !modifiers.alt && keys.contains(&keystroke.key.as_str())
}

#[cfg(test)]
mod tests {
    use super::to_push;

    #[test]
    fn a_copy_goes_out() {
        assert!(to_push(Some("new"), Some("old")));
        assert!(to_push(Some("new"), None));
    }

    #[test]
    fn what_both_have_does_not_go_out_again() {
        // The phone may have something newer from another application.
        assert!(!to_push(Some("same"), Some("same")));
    }

    #[test]
    fn nothing_copied_leaves_the_phone_alone() {
        assert!(!to_push(None, Some("theirs")));
    }
}

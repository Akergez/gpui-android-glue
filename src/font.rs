//! The system's font for the interface, in place of the one GPUI picks.
//!
//! The theme asks for `.SystemUIFont`. GPUI on Linux takes that for IBM Plex
//! Sans, which hardly a desktop has, and then draws with the first family of
//! its own list that is installed (Adwaita Sans, Cantarell, Noto Sans, …),
//! whatever the desktop is set to. So on Linux the desktop is asked, through
//! the settings portal (GNOME and KDE both answer it, and so it does in a
//! Flatpak), and the theme is given the family it names when it is
//! installed; so is its monospace font for code. Elsewhere `.SystemUIFont`
//! is already the system's: San Francisco on macOS, Segoe UI on Windows,
//! Roboto on Android.
//!
//! A family the application gave the theme itself is left as it is.

use gpui_kit::App;

/// Gives the theme the desktop's font. Called by [`crate::init`].
pub(crate) fn init(cx: &mut App) {
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    linux::init(cx);
    #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
    let _ = cx;
}

/// The family of a font as GTK and the portal describe it ("Noto Sans 10",
/// "Cantarell Bold 11", "Inter, Noto Sans Medium 10"): the longest start of
/// its words that is an installed family. None when no start is.
#[cfg_attr(not(any(target_os = "linux", target_os = "freebsd", test)), allow(dead_code))]
fn family<'a>(description: &str, installed: &'a [String]) -> Option<&'a str> {
    let first = description.split(',').next()?;
    let words: Vec<&str> = first.split_whitespace().collect();
    (1..=words.len()).rev().find_map(|count| {
        let name = words[..count].join(" ");
        installed
            .iter()
            .find(|family| family.eq_ignore_ascii_case(&name))
            .map(String::as_str)
    })
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
mod linux {
    use std::time::Duration;

    use gpui_kit::component::Theme;
    use gpui_kit::{App, SharedString};

    /// How long the start waits for the portal before drawing with what it
    /// has; an answer that comes later is put on then.
    const WAIT: Duration = Duration::from_millis(250);

    /// What the desktop says of its fonts, as GTK describes a font.
    #[derive(Default)]
    struct Desktop {
        interface: Option<String>,
        monospace: Option<String>,
    }

    pub(super) fn init(cx: &mut App) {
        // The theme's families as gpui-kit left them: GPUI's, before the
        // application has chosen any.
        let theme = Theme::global(cx);
        let gpuis = (theme.font_family.clone(), theme.mono_font_family.clone());
        let ask = async {
            let Ok(settings) = ashpd::desktop::settings::Settings::new().await else {
                return Desktop::default();
            };
            let read = async |key| settings.read::<String>("org.gnome.desktop.interface", key).await.ok();
            Desktop { interface: read("font-name").await, monospace: read("monospace-font-name").await }
        };
        match cx.foreground_executor().block_with_timeout(WAIT, ask) {
            Ok(desktop) => wear(desktop, gpuis, cx),
            Err(ask) => cx
                .spawn(async move |cx| {
                    let desktop = ask.await;
                    cx.update(|cx| wear(desktop, gpuis, cx));
                })
                .detach(),
        }
    }

    fn wear(desktop: Desktop, (interface, monospace): (SharedString, SharedString), cx: &mut App) {
        let installed = cx.text_system().all_font_names();
        let pick = |description: Option<String>, what: &str| {
            let description = description?;
            let family = super::family(&description, &installed);
            if family.is_none() {
                tracing::info!("The desktop's {what} font {description:?} is not installed; GPUI's stays.");
            }
            family.map(|family| SharedString::from(family.to_string()))
        };
        let wanted = (pick(desktop.interface, "interface"), pick(desktop.monospace, "monospace"));
        let theme = Theme::global(cx);
        // A family the application has put on the theme since is its choice.
        let interface = wanted.0.filter(|family| theme.font_family == interface && *family != interface);
        let monospace = wanted.1.filter(|family| theme.mono_font_family == monospace && *family != monospace);
        if interface.is_none() && monospace.is_none() {
            return;
        }
        tracing::info!("The desktop's fonts: {interface:?} for the interface, {monospace:?} for code.");
        Theme::update(cx, |theme| {
            if let Some(family) = interface {
                theme.font_family = family;
            }
            if let Some(family) = monospace {
                theme.mono_font_family = family;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::family;

    fn installed() -> Vec<String> {
        ["Noto Sans", "Noto Sans Mono", "Cantarell", "Inter Display", "Adwaita Sans"].map(String::from).to_vec()
    }

    #[test]
    fn the_size_and_the_style_are_not_the_family() {
        let installed = installed();
        assert_eq!(family("Noto Sans  10", &installed), Some("Noto Sans"));
        assert_eq!(family("Cantarell Bold 11", &installed), Some("Cantarell"));
        assert_eq!(family("Adwaita Sans 11", &installed), Some("Adwaita Sans"));
    }

    #[test]
    fn the_longest_installed_family_wins() {
        let installed = installed();
        assert_eq!(family("Noto Sans Mono 10", &installed), Some("Noto Sans Mono"));
        assert_eq!(family("Inter Display Medium 10", &installed), Some("Inter Display"));
    }

    #[test]
    fn of_a_list_the_first_is_taken_and_case_does_not_matter() {
        let installed = installed();
        assert_eq!(family("noto sans, Cantarell 10", &installed), Some("Noto Sans"));
    }

    #[test]
    fn a_family_that_is_not_installed_is_none() {
        let installed = installed();
        assert_eq!(family("Ubuntu 11", &installed), None);
        assert_eq!(family("", &installed), None);
    }
}

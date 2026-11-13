//! The system's font for the interface, in place of the one GPUI picks.
//!
//! The theme asks for `.SystemUIFont`. GPUI on Linux takes that for IBM Plex
//! Sans, which hardly a desktop has, and then draws with the first family of
//! its own list that is installed (Adwaita Sans, Cantarell, Noto Sans, …),
//! whatever the desktop is set to. So on Linux the desktop is asked, through
//! the settings portal (GNOME and KDE both answer it, and so it does in a
//! Flatpak), for the font its own text is drawn with: the family, the size
//! and the weight it names, and how much larger the person asked all text to
//! be. The theme is given the family when it is installed and has not been
//! chosen already; the size and the weight are kept in [`DesktopText`], which
//! the design system's frame draws its text at and an application that sizes
//! its interface in rems can calibrate its own by. So is its monospace font
//! for code. On macOS and Windows `.SystemUIFont` is already the system's:
//! San Francisco, Segoe UI.
//!
//! On Android it is gpui-mobile's text system that takes the system's font,
//! asking this crate's Java half (`dev.gpui.mobile.GpuiFonts`) which files
//! the device draws its own text with (Google Sans on a Pixel, the maker's or
//! the person's font elsewhere), and with it the "bold text" setting; the
//! font size setting is in the window's scale. Nothing is left to do here.
//!
//! A family the application gave the theme itself is left as it is. The size
//! and the weight are the desktop's alone, so they are put on whether the
//! family is ours or not.

use gpui_kit::{App, FontWeight, Global, Pixels};

/// What the desktop's settings said about its text: the size its own text is
/// drawn at, and the weight its font is named with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DesktopText {
    /// The interface font's size in logical pixels: the points it names, at
    /// 96 to the inch, times the desktop's text scaling.
    pub size: Option<Pixels>,
    /// The monospace font's size, the same way.
    pub mono_size: Option<Pixels>,
    /// The weight the interface font is named with, when it names one.
    pub weight: Option<FontWeight>,
}

impl Global for DesktopText {}

/// The desktop's text, when its settings named it. `None` off Linux, or when
/// the desktop did not answer.
pub fn text(cx: &App) -> Option<DesktopText> {
    cx.try_global::<DesktopText>().copied()
}

/// Gives the theme the desktop's font. Called by [`crate::init`].
pub(crate) fn init(cx: &mut App) {
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    linux::init(cx);
    #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
    let _ = cx;
}

/// A font as GTK describes one ("Noto Sans 10", "Cantarell Bold 11",
/// "Inter, Noto Sans Medium 10"): the longest start of its words that is an
/// installed family, the size at its end, and the weight its words name.
#[derive(Debug, Default, PartialEq)]
#[cfg_attr(
    not(any(target_os = "linux", target_os = "freebsd", test)),
    allow(dead_code)
)]
struct Named<'a> {
    family: Option<&'a str>,
    size: Option<f32>,
    /// Whether `size` is in pixels rather than points (the description said
    /// "12px").
    in_pixels: bool,
    weight: Option<FontWeight>,
}

#[cfg_attr(
    not(any(target_os = "linux", target_os = "freebsd", test)),
    allow(dead_code)
)]
fn named<'a>(description: &str, installed: &'a [String]) -> Named<'a> {
    let first = description.split(',').next().unwrap_or_default();
    let mut words: Vec<&str> = first.split_whitespace().collect();
    // The size is the last word, when it is a number.
    let (size, in_pixels) = match words.last().copied().and_then(size_of) {
        Some((size, in_pixels)) => {
            words.pop();
            (Some(size), in_pixels)
        }
        None => (None, false),
    };
    let family = (1..=words.len()).rev().find_map(|count| {
        let name = words[..count].join(" ");
        installed
            .iter()
            .find(|family| family.eq_ignore_ascii_case(&name))
            .map(String::as_str)
    });
    let style = family.map_or(0, |family| family.split_whitespace().count());
    Named {
        family,
        size,
        in_pixels,
        weight: named_weight(&words[style..]),
    }
}

/// The size at the end of a font description: points, or pixels with a `px`
/// suffix.
#[cfg_attr(
    not(any(target_os = "linux", target_os = "freebsd", test)),
    allow(dead_code)
)]
fn size_of(word: &str) -> Option<(f32, bool)> {
    let (number, in_pixels) = match word.strip_suffix("px") {
        Some(number) => (number, true),
        None => (word, false),
    };
    let size = number.parse::<f32>().ok()?;
    (size > 0.0).then_some((size, in_pixels))
}

/// The weight the style words of a font description name: "Bold" is 700,
/// "Semi-Bold" 600, "Medium" 500, and so on.
#[cfg_attr(
    not(any(target_os = "linux", target_os = "freebsd", test)),
    allow(dead_code)
)]
fn named_weight(words: &[&str]) -> Option<FontWeight> {
    let mut weight = None;
    let mut index = 0;
    while index < words.len() {
        let mut word = words[index].to_ascii_lowercase();
        // Pango names these with a hyphen ("Semi-Bold"), and a hand-written
        // description may leave a space in their place ("Semi Bold").
        if matches!(word.as_str(), "ultra" | "extra" | "semi" | "demi") && index + 1 < words.len() {
            word.push('-');
            word.push_str(&words[index + 1].to_ascii_lowercase());
            index += 2;
        } else {
            index += 1;
        }
        weight = match word.as_str() {
            "thin" => Some(FontWeight::THIN),
            "ultra-light" | "extra-light" | "ultralight" | "extralight" => {
                Some(FontWeight::EXTRA_LIGHT)
            }
            "light" => Some(FontWeight::LIGHT),
            "regular" | "normal" | "book" => Some(FontWeight::NORMAL),
            "medium" => Some(FontWeight::MEDIUM),
            "semi-bold" | "semibold" | "demi-bold" | "demibold" => Some(FontWeight::SEMIBOLD),
            "bold" => Some(FontWeight::BOLD),
            "ultra-bold" | "extra-bold" | "ultrabold" | "extrabold" => Some(FontWeight::EXTRA_BOLD),
            "heavy" | "black" => Some(FontWeight::BLACK),
            _ => weight,
        };
    }
    weight
}

/// A named size as logical pixels: points at 96 to the inch, or pixels as
/// they are, times the desktop's text scaling.
#[cfg_attr(
    not(any(target_os = "linux", target_os = "freebsd", test)),
    allow(dead_code)
)]
fn pixels(size: f32, in_pixels: bool, scaling: f32) -> Pixels {
    let pixels = if in_pixels { size } else { size * 96.0 / 72.0 };
    gpui_kit::px(pixels * scaling)
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
mod linux {
    use std::time::Duration;

    use gpui_kit::component::Theme;
    use gpui_kit::{App, SharedString};

    use super::DesktopText;

    /// How long the start waits for the portal before drawing with what it
    /// has; an answer that comes later is put on then.
    const WAIT: Duration = Duration::from_millis(250);

    /// What the desktop says of its fonts, as GTK describes a font.
    #[derive(Default)]
    struct Desktop {
        interface: Option<String>,
        monospace: Option<String>,
        /// How much larger than 100% the person asked text to be.
        scaling: Option<f64>,
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
            let read = async |key| {
                settings
                    .read::<String>("org.gnome.desktop.interface", key)
                    .await
                    .ok()
            };
            Desktop {
                interface: read("font-name").await,
                monospace: read("monospace-font-name").await,
                scaling: settings
                    .read::<f64>("org.gnome.desktop.interface", "text-scaling-factor")
                    .await
                    .ok(),
            }
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

    fn wear(
        desktop: Desktop,
        (interface_family, monospace_family): (SharedString, SharedString),
        cx: &mut App,
    ) {
        let installed = cx.text_system().all_font_names();
        let interface = super::named(desktop.interface.as_deref().unwrap_or_default(), &installed);
        let monospace = super::named(desktop.monospace.as_deref().unwrap_or_default(), &installed);
        // A scaling outside reason is not taken: it is a percentage the
        // desktop writes as a fraction, not a factor of the size.
        let scaling = desktop
            .scaling
            .map(|scaling| scaling as f32)
            .filter(|scaling| (0.5..=4.0).contains(scaling))
            .unwrap_or(1.0);
        let size = interface
            .size
            .map(|size| super::pixels(size, interface.in_pixels, scaling));
        let mono_size = monospace
            .size
            .map(|size| super::pixels(size, monospace.in_pixels, scaling));
        let theme = Theme::global(cx);
        // A family the application has put on the theme since is its choice.
        let wanted_interface = interface
            .family
            .filter(|family| theme.font_family == interface_family && interface_family != *family);
        let wanted_monospace = monospace.family.filter(|family| {
            theme.mono_font_family == monospace_family && monospace_family != *family
        });
        if wanted_interface.is_none()
            && wanted_monospace.is_none()
            && size.is_none()
            && mono_size.is_none()
            && interface.weight.is_none()
        {
            return;
        }
        tracing::info!(
            "The desktop's text: {wanted_interface:?} {size:?}, {wanted_monospace:?} {mono_size:?}, weight {:?}",
            interface.weight
        );
        Theme::update(cx, |theme| {
            if let Some(family) = wanted_interface {
                theme.font_family = family.into();
            }
            if let Some(family) = wanted_monospace {
                theme.mono_font_family = family.into();
            }
        });
        if size.is_some() || mono_size.is_some() || interface.weight.is_some() {
            cx.set_global(DesktopText {
                size,
                mono_size,
                weight: interface.weight,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Named, named, pixels, size_of};
    use gpui_kit::{FontWeight, px};

    fn installed() -> Vec<String> {
        [
            "Noto Sans",
            "Noto Sans Mono",
            "Cantarell",
            "Inter Display",
            "Adwaita Sans",
        ]
        .map(String::from)
        .to_vec()
    }

    #[test]
    fn the_size_and_the_style_are_not_the_family() {
        let installed = installed();
        assert_eq!(named("Noto Sans  10", &installed).family, Some("Noto Sans"));
        assert_eq!(
            named("Cantarell Bold 11", &installed).family,
            Some("Cantarell")
        );
        assert_eq!(
            named("Adwaita Sans 11", &installed).family,
            Some("Adwaita Sans")
        );
    }

    #[test]
    fn the_longest_installed_family_wins() {
        let installed = installed();
        assert_eq!(
            named("Noto Sans Mono 10", &installed).family,
            Some("Noto Sans Mono")
        );
        assert_eq!(
            named("Inter Display Medium 10", &installed).family,
            Some("Inter Display")
        );
    }

    #[test]
    fn of_a_list_the_first_is_taken_and_case_does_not_matter() {
        let installed = installed();
        assert_eq!(
            named("noto sans, Cantarell 10", &installed).family,
            Some("Noto Sans")
        );
    }

    #[test]
    fn a_family_that_is_not_installed_is_none() {
        let installed = installed();
        assert_eq!(named("Ubuntu 11", &installed).family, None);
        assert_eq!(named("", &installed).family, None);
    }

    #[test]
    fn the_size_is_the_last_word_in_points_or_pixels() {
        assert_eq!(
            named("Noto Sans  10", &installed()),
            Named {
                family: Some("Noto Sans"),
                size: Some(10.0),
                in_pixels: false,
                weight: None,
            }
        );
        assert_eq!(
            named("Adwaita Sans 12px", &installed()),
            Named {
                family: Some("Adwaita Sans"),
                size: Some(12.0),
                in_pixels: true,
                weight: None,
            }
        );
        assert_eq!(named("Noto Sans", &installed()).size, None);
        assert_eq!(size_of("10"), Some((10.0, false)));
        assert_eq!(size_of("12.5"), Some((12.5, false)));
        assert_eq!(size_of("12px"), Some((12.0, true)));
        assert_eq!(size_of("bold"), None);
        assert_eq!(size_of("0"), None);
    }

    #[test]
    fn the_style_words_are_its_weight() {
        let installed = installed();
        assert_eq!(
            named("Cantarell Bold 11", &installed).weight,
            Some(FontWeight::BOLD)
        );
        assert_eq!(
            named("Inter Display Medium 10", &installed).weight,
            Some(FontWeight::MEDIUM)
        );
        assert_eq!(
            named("Noto Sans Semi-Bold 10", &installed).weight,
            Some(FontWeight::SEMIBOLD)
        );
        assert_eq!(
            named("Noto Sans Extra Light 10", &installed).weight,
            Some(FontWeight::EXTRA_LIGHT)
        );
        assert_eq!(named("Noto Sans Italic 10", &installed).weight, None);
    }

    #[test]
    fn a_size_in_points_is_pixels_at_ninety_six_to_the_inch() {
        assert_eq!(pixels(10.0, false, 1.0), px(10.0 * 96.0 / 72.0));
        assert_eq!(pixels(12.0, true, 1.0), px(12.0));
        assert_eq!(pixels(10.0, false, 1.25), px(10.0 * 96.0 / 72.0 * 1.25));
    }
}

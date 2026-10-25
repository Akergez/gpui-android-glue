use gpui_kit::App;

/// Hands a link to whatever the system opens links with.
///
/// Everything should go through here rather than `cx.open_url`: the Android
/// platform's own `open_url` does nothing at all.
pub fn open_url(url: &str, cx: &mut App) {
    #[cfg(not(target_os = "android"))]
    cx.open_url(url);
    #[cfg(target_os = "android")]
    {
        let _ = cx;
        match gpui_mobile::packages::url_launcher::launch_url(url) {
            Ok(true) => {}
            Ok(false) => tracing::error!("nothing on this phone opens {url}"),
            Err(error) => tracing::error!(%error, "could not open {url}"),
        }
    }
}

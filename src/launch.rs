//! `android_main`, the whole of it.
//!
//! ```ignore
//! #[cfg(target_os = "android")]
//! #[unsafe(no_mangle)]
//! fn android_main(app: android_activity::AndroidApp) {
//!     gpui_android_glue::launch::run(
//!         app,
//!         Config::new("aiddiction").filter("warn,aiddiction=info"),
//!         gpui_kit::assets::Assets,
//!         start,
//!     );
//! }
//! ```

use std::sync::atomic::{AtomicBool, Ordering};

use android_activity::AndroidApp;
use gpui_kit::{App, AssetSource};
use gpui_mobile::android::jni;

/// What differs between applications at start.
pub struct Config {
    tag: String,
    filter: String,
    cache: bool,
}

impl Config {
    /// `tag` is what logcat shows the application's log under.
    pub fn new(tag: impl Into<String>) -> Self {
        Config {
            tag: tag.into(),
            filter: "warn".into(),
            cache: false,
        }
    }

    /// Which of the log is kept, in `env_logger`'s terms: `warn,my_app=info`.
    pub fn filter(mut self, filter: impl Into<String>) -> Self {
        self.filter = filter.into();
        self
    }

    /// Points `XDG_CACHE_HOME` into the application's private directory too.
    pub fn with_cache(mut self) -> Self {
        self.cache = true;
        self
    }
}

/// Starts the toolkit on gpui-mobile's platform and runs `start` in it, as
/// `Application::run` does on a desktop. Returns never: when the activity is
/// destroyed, the process ends with it.
///
/// Before `start`, in this order: the log goes to logcat (`tracing` hands its
/// events to `log` when nothing subscribes), a panic is logged rather than
/// lost, the XDG directories point into the application's private directory
/// (an application has no home directory here), and with the
/// `platform-verifier` feature rustls is given the activity to ask about
/// certificates. [`crate::init`] runs first thing in the toolkit; `start`
/// must still call `gpui_kit::init`.
pub fn run(
    app: AndroidApp,
    config: Config,
    assets: impl AssetSource,
    start: impl FnOnce(&mut App) + 'static,
) -> ! {
    // The system may create the activity again in a process it has kept, and
    // that calls `android_main` again, on another thread. The toolkit belongs
    // to the thread it first ran on and cannot be started twice, so one
    // process is one run: a second call ends the process, and the system
    // starts the activity over in a fresh one.
    static STARTED: AtomicBool = AtomicBool::new(false);
    if STARTED.swap(true, Ordering::SeqCst) {
        std::process::exit(0);
    }

    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Debug)
            .with_tag(config.tag.as_str())
            .with_filter(android_logger::FilterBuilder::new().parse(&config.filter).build()),
    );
    jni::install_panic_hook();

    if let Some(files) = app.internal_data_path() {
        // SAFETY: no other thread of the application's exists yet to read the
        // environment.
        unsafe {
            std::env::set_var("XDG_CONFIG_HOME", files.join("config"));
            std::env::set_var("XDG_DATA_HOME", files.join("data"));
            if config.cache {
                std::env::set_var("XDG_CACHE_HOME", files.join("cache"));
            }
        }
    }

    jni::init_platform(&app);
    let Some(platform) = jni::shared_platform() else {
        tracing::error!("no Android platform to run on");
        std::process::exit(1);
    };

    // Certificates are checked by the system, which is Java: the verifier has
    // to be given the activity before the first connection, or that
    // connection panics.
    #[cfg(feature = "platform-verifier")]
    if let Err(error) = jni::with_env(|env| {
        let activity = jni::activity(env)?;
        rustls_platform_verifier::android::init_with_env(env, activity).map_err(|error| error.to_string())
    }) {
        tracing::error!(%error, "no certificate verifier: nothing will connect over TLS");
    }

    gpui_kit::gpui::Application::with_platform(platform.into_rc())
        .with_assets(assets)
        .run(move |cx| {
            crate::init(cx);
            start(cx);
        });

    // The activity is gone. Nothing of the application's can be used by the
    // next one (see above), so the process goes with it.
    std::process::exit(0);
}

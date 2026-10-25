//! Saving a document where the person chooses.
//!
//! On a desktop that is `cx.prompt_for_new_path` and a write. On Android
//! there is no path to write to: the Storage Access Framework's save dialog
//! grants write access to the one document chosen, for as long as its
//! activity lives, and [`save`] writes it through `ContentResolver` in that
//! time. No storage permission is asked for or needed.

/// Asks where to save `bytes`, offering `name` as a document of type `mime`,
/// and writes them there. `Ok(false)` if the person cancelled.
///
/// It waits for the person: call it on a worker thread (a background task),
/// never on the UI's.
#[cfg(target_os = "android")]
pub fn save(bytes: &[u8], mime: &str, name: &str) -> Result<bool, String> {
    use jni::objects::JValue;

    crate::android::with_class("dev.gpui.mobile.GpuiPathPicker", |env, activity, class| {
        let bytes = env.byte_array_from_slice(bytes)?;
        let mime = env.new_string(mime)?;
        let name = env.new_string(name)?;
        env.call_static_method(
            class,
            jni::jni_str!("saveDocument"),
            jni::jni_sig!("(Landroid/app/Activity;[BLjava/lang/String;Ljava/lang/String;)Z"),
            &[
                JValue::Object(activity),
                JValue::Object(bytes.as_ref()),
                JValue::Object(mime.as_ref()),
                JValue::Object(name.as_ref()),
            ],
        )?
        .z()
    })
    .map_err(|error| format!("Could not save {name}: {error}"))
}

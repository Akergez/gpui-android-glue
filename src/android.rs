//! Calling Java: the activity, and the classes of the library's Java half.

use gpui_mobile::android::jni as bridge;
use jni::objects::{JClass, JObject};

/// Runs `call` with the activity (a `GpuiActivity`), turning a Java
/// exception into an error and clearing it, as every caller must.
pub(crate) fn with_activity<T>(
    call: impl FnOnce(&mut jni::Env, &JObject) -> jni::errors::Result<T>,
) -> Result<T, String> {
    bridge::with_env(|env| {
        let activity = bridge::activity(env)?;
        call(env, &activity).map_err(|error| {
            env.exception_clear();
            error.to_string()
        })
    })
}

/// Like [`with_activity`], with one of the application's classes as well:
/// `name` is dotted, `dev.gpui.glue.GpuiBack`. Application classes are only
/// found through the activity's class loader, never from a native thread's.
pub(crate) fn with_class<T>(
    name: &str,
    call: impl FnOnce(&mut jni::Env, &JObject, &JClass) -> jni::errors::Result<T>,
) -> Result<T, String> {
    bridge::with_env(|env| {
        let activity = bridge::activity(env)?;
        let class = bridge::find_app_class(env, name)?;
        call(env, &activity, &class).map_err(|error| {
            env.exception_clear();
            error.to_string()
        })
    })
}

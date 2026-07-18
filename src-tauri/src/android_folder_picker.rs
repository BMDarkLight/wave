//! Android folder picker using Storage Access Framework (SAF) via JNI.

use tauri::AppHandle;

/// Result from the folder picker.
#[derive(Debug, Clone, serde::Serialize)]
pub struct FolderPickerResult {
    pub uri: String,
    pub display_name: Option<String>,
}

/// Opens the Android Storage Access Framework folder picker.
/// Returns a content:// URI with persistable URI permission.
#[cfg(target_os = "android")]
pub fn pick_folder(_app: &AppHandle) -> Result<FolderPickerResult, String> {
    use jni::objects::{JObject, JString, JThrowable, JValue, JValueOwned};
    use jni::JavaVM;
    use ndk_context::android_context;

    fn jstring_to_owned(env: &mut jni::JNIEnv<'_>, obj: JObject<'_>) -> Option<String> {
        if obj.is_null() {
            return None;
        }
        let js: JString = obj.into();
        env.get_string(&js)
            .ok()
            .map(|s| s.to_string_lossy().into_owned())
            .filter(|s| !s.is_empty())
    }

    fn throwable_message(
        env: &mut jni::JNIEnv<'_>,
        ex: JThrowable<'_>,
        depth: u8,
    ) -> Option<String> {
        if depth > 6 {
            return Some("Folder picker: nested Java exception".into());
        }
        // Unwrap ExecutionException / InvocationTargetException causes first.
        match env.call_method(&ex, "getCause", "()Ljava/lang/Throwable;", &[]) {
            Ok(cause_v) => {
                if let Ok(cause) = cause_v.l() {
                    if !cause.is_null() {
                        let cause_t: JThrowable = cause.into();
                        if let Some(inner) = throwable_message(env, cause_t, depth + 1) {
                            return Some(inner);
                        }
                    }
                }
            }
            Err(_) => {
                let _ = env.exception_clear();
            }
        }

        match env.call_method(&ex, "getMessage", "()Ljava/lang/String;", &[]) {
            Ok(msg_v) => {
                if let Ok(obj) = msg_v.l() {
                    if let Some(msg) = jstring_to_owned(env, obj) {
                        return Some(msg);
                    }
                }
            }
            Err(_) => {
                let _ = env.exception_clear();
            }
        }

        match env.call_method(&ex, "toString", "()Ljava/lang/String;", &[]) {
            Ok(v) => v.l().ok().and_then(|obj| jstring_to_owned(env, obj)),
            Err(_) => {
                let _ = env.exception_clear();
                None
            }
        }
    }

    fn jni_error(env: &mut jni::JNIEnv<'_>, fallback: &str) -> String {
        match env.exception_occurred() {
            Ok(ex) if !ex.is_null() => {
                let _ = env.exception_clear();
                throwable_message(env, ex, 0).unwrap_or_else(|| fallback.to_string())
            }
            _ => {
                let _ = env.exception_clear();
                fallback.to_string()
            }
        }
    }

    fn call_checked<'local>(
        env: &mut jni::JNIEnv<'local>,
        call: impl FnOnce(&mut jni::JNIEnv<'local>) -> jni::errors::Result<JValueOwned<'local>>,
        what: &str,
    ) -> Result<JValueOwned<'local>, String> {
        // Clear any stale exception before the call so we attribute failures correctly.
        if env.exception_check().unwrap_or(false) {
            return Err(jni_error(env, what));
        }
        match call(env) {
            Ok(value) => {
                if env.exception_check().unwrap_or(false) {
                    Err(jni_error(env, what))
                } else {
                    Ok(value)
                }
            }
            Err(err) => {
                if env.exception_check().unwrap_or(false) {
                    Err(jni_error(env, &format!("{what} ({err})")))
                } else {
                    Err(format!("{what}: {err}"))
                }
            }
        }
    }

    // Get the JavaVM and Activity from ndk_context.
    let ctx = android_context();
    let vm = unsafe { JavaVM::from_raw(ctx.vm() as *mut _) }
        .map_err(|e| format!("Folder picker: failed to get JavaVM: {e}"))?;
    let activity = ctx.context();
    if activity.is_null() {
        return Err("Folder picker: Android Activity context is null".into());
    }

    let mut env = vm
        .attach_current_thread()
        .map_err(|e| format!("Folder picker: failed to attach JNI thread: {e}"))?;

    let activity_obj = unsafe { JObject::from_raw(activity as *mut _) };

    // static CompletableFuture pick(Activity)
    let future_value = call_checked(
        &mut env,
        |env| {
            env.call_static_method(
                "app/bmdarklight/wave/FolderPickerCallback",
                "pick",
                "(Landroid/app/Activity;)Ljava/util/concurrent/CompletableFuture;",
                &[JValue::Object(&activity_obj)],
            )
        },
        "Folder picker: FolderPickerCallback.pick() failed",
    )?;

    let future = future_value
        .l()
        .map_err(|e| format!("Folder picker: bad CompletableFuture: {e}"))?;
    if future.is_null() {
        return Err("Folder picker: pick() returned null future".into());
    }

    let time_unit_class = env
        .find_class("java/util/concurrent/TimeUnit")
        .map_err(|e| format!("Folder picker: TimeUnit not found: {e}"))?;
    if env.exception_check().unwrap_or(false) {
        return Err(jni_error(&mut env, "Folder picker: TimeUnit lookup failed"));
    }

    let seconds_field = env
        .get_static_field(&time_unit_class, "SECONDS", "Ljava/util/concurrent/TimeUnit;")
        .map_err(|e| format!("Folder picker: TimeUnit.SECONDS missing: {e}"))?;
    if env.exception_check().unwrap_or(false) {
        return Err(jni_error(&mut env, "Folder picker: TimeUnit.SECONDS failed"));
    }
    let seconds = seconds_field
        .l()
        .map_err(|e| format!("Folder picker: bad TimeUnit.SECONDS: {e}"))?;

    // future.get(120, SECONDS) — long enough for the system picker UI.
    let result_value = call_checked(
        &mut env,
        |env| {
            env.call_method(
                &future,
                "get",
                "(JLjava/util/concurrent/TimeUnit;)Ljava/lang/Object;",
                &[JValue::Long(120), JValue::Object(&seconds)],
            )
        },
        "Folder picker failed (cancelled, timed out, or internal error)",
    );

    let result = match result_value {
        Ok(v) => v
            .l()
            .map_err(|e| format!("Folder picker: bad result object: {e}"))?,
        Err(msg) => {
            // Unwrap ExecutionException cause when present for a clearer message.
            return Err(msg);
        }
    };

    // Null => user cancelled.
    if result.is_null() {
        return Err("Folder picker cancelled".into());
    }

    let uri_field = env
        .get_field(&result, "uri", "Ljava/lang/String;")
        .map_err(|e| format!("Folder picker: missing uri field: {e}"))?;
    if env.exception_check().unwrap_or(false) {
        return Err(jni_error(&mut env, "Folder picker: reading uri failed"));
    }
    let uri_obj = uri_field
        .l()
        .map_err(|e| format!("Folder picker: bad uri field: {e}"))?;
    if uri_obj.is_null() {
        return Err("Folder picker returned null URI".into());
    }
    let uri_jstring: JString = uri_obj.into();
    let uri_string = env
        .get_string(&uri_jstring)
        .map_err(|e| format!("Folder picker: uri string convert failed: {e}"))?
        .to_string_lossy()
        .into_owned();

    let display_name_field = env
        .get_field(&result, "displayName", "Ljava/lang/String;")
        .map_err(|e| format!("Folder picker: missing displayName field: {e}"))?;
    if env.exception_check().unwrap_or(false) {
        return Err(jni_error(
            &mut env,
            "Folder picker: reading displayName failed",
        ));
    }
    let display_name = {
        let obj = display_name_field
            .l()
            .map_err(|e| format!("Folder picker: bad displayName field: {e}"))?;
        if obj.is_null() {
            None
        } else {
            let js: JString = obj.into();
            let name = env
                .get_string(&js)
                .map_err(|e| format!("Folder picker: displayName convert failed: {e}"))?
                .to_string_lossy()
                .into_owned();
            Some(name)
        }
    };

    Ok(FolderPickerResult {
        uri: uri_string,
        display_name,
    })
}

/// Fallback for non-Android targets.
#[cfg(not(target_os = "android"))]
pub fn pick_folder(_app: &AppHandle) -> Result<FolderPickerResult, String> {
    Err("Folder picker is only available on Android".to_string())
}

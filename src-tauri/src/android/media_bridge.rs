//! Android native media-action bridge.
//!
//! The media-session plugin dispatches play/pause/next/… into
//! `MediaNativeBridge.dispatch`, which calls this module over JNI.
//! Actions are queued and applied on the GUI tick thread so transport
//! keeps working when the WebView is frozen in the background.

#![cfg(target_os = "android")]

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use jni::objects::{JClass, JObject, JString};
use jni::{JNIEnv, JavaVM, NativeMethod};
use tauri::AppHandle;

use crate::android::jni as android_jni;
use crate::commands;

static PENDING: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());
static NATIVES_READY: AtomicBool = AtomicBool::new(false);

/// Queue a media action from the JNI callback (any thread).
pub fn push_action(action: String) {
    let action = action.trim().to_string();
    if action.is_empty() {
        return;
    }
    if let Ok(mut queue) = PENDING.lock() {
        // Coalesce bursty duplicate transport presses.
        if queue.back().is_some_and(|last| last == &action)
            && matches!(action.as_str(), "play" | "pause" | "stop")
        {
            return;
        }
        queue.push_back(action);
    }
}

/// Apply any pending native media actions. Called from the GUI tick loop.
pub fn drain_actions(app: &AppHandle) {
    // Setup may run before tao has published the Activity — retry here.
    if !NATIVES_READY.load(Ordering::Acquire) {
        try_install();
    }

    let actions: Vec<String> = match PENDING.lock() {
        Ok(mut queue) => queue.drain(..).collect(),
        Err(_) => return,
    };
    for action in actions {
        if let Err(error) = commands::handle_native_media_action(app, &action) {
            tracing::warn!("Android native media action '{action}' failed: {error}");
        }
    }
}

/// Register JNI natives for [`MediaNativeBridge`] (best-effort; retried from tick).
pub fn install(_app: &AppHandle) {
    try_install();
}

fn try_install() {
    if NATIVES_READY.load(Ordering::Acquire) {
        return;
    }

    android_jni::ensure_jni_thread_attached();

    let ctx = match std::panic::catch_unwind(ndk_context::android_context) {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    let vm = match unsafe { JavaVM::from_raw(ctx.vm() as *mut _) } {
        Ok(vm) => vm,
        Err(_) => return,
    };

    let Ok(mut env) = vm.attach_current_thread() else {
        return;
    };

    let activity = unsafe { JObject::from_raw(ctx.context() as *mut _) };
    if activity.is_null() {
        return;
    }

    let class = match load_app_class(&mut env, &activity, "app.bmdarklight.wave.MediaNativeBridge") {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("MediaNativeBridge class load failed: {e}");
            return;
        }
    };

    let natives = [NativeMethod {
        name: "nativeOnMediaAction".into(),
        sig: "(Ljava/lang/String;)V".into(),
        fn_ptr: native_on_media_action as *mut std::ffi::c_void,
    }];

    if let Err(e) = env.register_native_methods(&class, &natives) {
        tracing::warn!("MediaNativeBridge RegisterNatives failed: {e}");
        return;
    }

    NATIVES_READY.store(true, Ordering::Release);
    tracing::info!("MediaNativeBridge natives registered");
}

fn load_app_class<'local>(
    env: &mut JNIEnv<'local>,
    activity: &JObject<'local>,
    binary_name: &str,
) -> Result<JClass<'local>, String> {
    let loader = env
        .call_method(activity, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])
        .map_err(|e| format!("getClassLoader: {e}"))?
        .l()
        .map_err(|e| format!("getClassLoader value: {e}"))?;
    if loader.is_null() {
        return Err("ClassLoader is null".into());
    }
    let name = env
        .new_string(binary_name)
        .map_err(|e| format!("new_string: {e}"))?;
    let class_obj = env
        .call_method(
            &loader,
            "loadClass",
            "(Ljava/lang/String;)Ljava/lang/Class;",
            &[(&name).into()],
        )
        .map_err(|e| format!("loadClass({binary_name}): {e}"))?
        .l()
        .map_err(|e| format!("loadClass value: {e}"))?;
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_describe();
        let _ = env.exception_clear();
        return Err(format!("loadClass threw for {binary_name}"));
    }
    if class_obj.is_null() {
        return Err(format!("loadClass returned null for {binary_name}"));
    }
    Ok(JClass::from(class_obj))
}

extern "system" fn native_on_media_action(mut env: JNIEnv, _class: JClass, action: JString) {
    let Ok(action) = env.get_string(&action) else {
        return;
    };
    push_action(action.to_string_lossy().into_owned());
}

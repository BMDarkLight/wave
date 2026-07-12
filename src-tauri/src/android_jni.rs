//! Android JNI thread attachment helpers.
//!
//! On Android, the `cpal`/`oboe` audio backend requires the calling thread to be
//! attached to the JVM before opening an output stream. Tauri command handlers run
//! on tokio worker threads which are **not** attached by default, so we must
//! explicitly attach before touching any audio APIs.

/// Attach the current thread to the JVM if it isn't already attached.
///
/// The thread will be permanently attached (auto-detached on thread exit).
/// This is a no-op on non-Android targets.
#[cfg(target_os = "android")]
pub(crate) fn ensure_jni_thread_attached() {
    let vm_ptr = unsafe { ndk_context::android_context().vm() };
    if vm_ptr.is_null() {
        tracing::warn!("Cannot attach JNI thread: JavaVM pointer is null");
        return;
    }

    unsafe {
        // `from_raw` wraps the pointer without taking ownership.
        let vm = match jni::JavaVM::from_raw(vm_ptr as *mut jni::sys::JavaVM) {
            Ok(vm) => vm,
            Err(e) => {
                tracing::warn!("Failed to create JavaVM wrapper: {e}");
                return;
            }
        };

        // attach_current_thread_permanently attaches without an auto-detach guard.
        // If already attached, this is a no-op.
        match vm.attach_current_thread_permanently() {
            Ok(_env) => {
                tracing::debug!("JNI thread attached successfully");
            }
            Err(e) => {
                tracing::warn!("JNI AttachCurrentThread failed: {e}");
            }
        }
    }
}

/// No-op on non-Android platforms.
#[cfg(not(target_os = "android"))]
pub(crate) fn ensure_jni_thread_attached() {}

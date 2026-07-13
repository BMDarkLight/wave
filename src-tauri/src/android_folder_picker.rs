//! Android folder picker using Storage Access Framework (SAF) via JNI

use tauri::AppHandle;

/// Result from the folder picker
#[derive(Debug, Clone, serde::Serialize)]
pub struct FolderPickerResult {
    pub uri: String,
    pub display_name: Option<String>,
}

/// Opens the Android Storage Access Framework folder picker.
/// Returns a content:// URI with persistable URI permission.
#[cfg(target_os = "android")]
pub fn pick_folder(app: &AppHandle) -> Result<FolderPickerResult, String> {
    use jni::objects::{JObject, JValue};
    use jni::JavaVM;
    use ndk_context::android_context;

    // Get the JavaVM and Activity from ndk_context
    let ctx = android_context();
    let vm = unsafe { JavaVM::from_raw(ctx.vm() as *mut _) }
        .map_err(|e| format!("Failed to get JavaVM: {}", e))?;
    let activity = ctx.context();

    // Attach current thread to JVM
    let mut env = vm.attach_current_thread()
        .map_err(|e| format!("Failed to attach thread: {}", e))?;

    // Get the activity object
    let activity_obj = unsafe { JObject::from_raw(activity as *mut _) };

    // Find our FolderPickerCallback class
    let callback_class = env.find_class("app/bmdarklight/wave/FolderPickerCallback")
        .map_err(|e| format!("Failed to find FolderPickerCallback class: {}", e))?;

    // Create an instance of FolderPickerCallback
    let callback_obj = env.new_object(&callback_class, "(Landroidx/activity/ComponentActivity;)V", &[JValue::Object(&activity_obj)])
        .map_err(|e| format!("Failed to create FolderPickerCallback instance: {}", e))?;

    // Call pickFolder() to get the CompletableFuture
    let future_obj = env.call_method(&callback_obj, "pickFolder", "()Ljava/util/concurrent/CompletableFuture;", &[])
        .map_err(|e| format!("Failed to call pickFolder: {}", e))?;

    let future = future_obj.l()
        .map_err(|e| format!("Failed to get CompletableFuture object: {}", e))?;

    // Use a channel to receive the result from the callback
    let (tx, rx) = mpsc::channel::<Result<FolderPickerResult, String>>();

    // Create a global reference to the future for the callback
    let future_global = env.new_global_ref(&future)
        .map_err(|e| format!("Failed to create global ref: {}", e))?;

    // Clone for the closure
    let vm_ptr = vm.get_java_vm_pointer();
    
    // Get the future class and the get method with timeout
    let future_class = env.find_class("java/util/concurrent/CompletableFuture")
        .map_err(|e| format!("Failed to find CompletableFuture class: {}", e))?;

    // Use get(long, TimeUnit) with 60 second timeout to avoid blocking forever
    let get_method = env.get_method_id(&future_class, "get", "(JLjava/util/concurrent/TimeUnit;)Ljava/lang/Object;")
        .map_err(|e| format!("Failed to get get method: {}", e))?;

    let time_unit_class = env.find_class("java/util/concurrent/TimeUnit")
        .map_err(|e| format!("Failed to find TimeUnit class: {}", e))?;

    let seconds_field = env.get_static_field_id(&time_unit_class, "SECONDS", "Ljava/util/concurrent/TimeUnit;")
        .map_err(|e| format!("Failed to get SECONDS field: {}", e))?;

    let seconds_obj = env.get_static_field(&time_unit_class, seconds_field, "Ljava/util/concurrent/TimeUnit;")
        .map_err(|e| format!("Failed to get SECONDS object: {}", e))?;

    // Call get with 60 second timeout
    let result_obj = env.call_method(&future, get_method, &[JValue::Long(60), JValue::Object(&seconds_obj.l().unwrap())])
        .map_err(|e| format!("Failed to call get with timeout: {}", e))?;

    let result = result_obj.l()
        .map_err(|e| format!("Failed to get result object: {}", e))?;

    // Clean up global ref
    env.delete_global_ref(future_global).ok();

    // Check if result is null (user cancelled)
    if result.is_null() {
        return Err("User cancelled folder picker".to_string());
    }

    // Result is a FolderPickerResult object
    // Get the uri field
    let result_class = env.get_object_class(&result)
        .map_err(|e| format!("Failed to get result class: {}", e))?;

    let uri_field = env.get_field_id(&result_class, "uri", "Ljava/lang/String;")
        .map_err(|e| format!("Failed to get uri field: {}", e))?;

    let uri_obj = env.get_field(&result, uri_field, "Ljava/lang/String;")
        .map_err(|e| format!("Failed to get uri field value: {}", e))?;

    let uri_string: String = if uri_obj.l().map_or(true, |o| o.is_null()) {
        return Err("Folder picker returned null URI".to_string());
    } else {
        let uri_string_obj: jni::objects::JString = uri_obj.l()
            .map_err(|e| format!("Failed to get uri object: {}", e))?
            .into();
        env.get_string(&uri_string_obj)
            .map_err(|e| format!("Failed to convert uri to string: {}", e))?
            .to_string_lossy()
            .into_owned()
    };

    // Get the displayName field
    let display_name_field = env.get_field_id(&result_class, "displayName", "Ljava/lang/String;")
        .map_err(|e| format!("Failed to get displayName field: {}", e))?;

    let display_name_obj = env.get_field(&result, display_name_field, "Ljava/lang/String;")
        .map_err(|e| format!("Failed to get displayName field value: {}", e))?;

    let display_name = if display_name_obj.l().map_or(true, |o| o.is_null()) {
        None
    } else {
        let display_name_string_obj: jni::objects::JString = display_name_obj.l()
            .map_err(|e| format!("Failed to get displayName object: {}", e))?
            .into();
        Some(env.get_string(&display_name_string_obj)
            .map_err(|e| format!("Failed to convert displayName to string: {}", e))?
            .to_string_lossy()
            .into_owned())
    };

    Ok(FolderPickerResult {
        uri: uri_string,
        display_name,
    })
}

/// Fallback for non-Android targets
#[cfg(not(target_os = "android"))]
pub fn pick_folder(_app: &AppHandle) -> Result<FolderPickerResult, String> {
    Err("Folder picker is only available on Android".to_string())
}
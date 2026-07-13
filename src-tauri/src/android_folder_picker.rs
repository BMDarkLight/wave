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
pub fn pick_folder(_app: &AppHandle) -> Result<FolderPickerResult, String> {
    use jni::objects::{JObject, JValue, JString};
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

    // Call get() on the future
    let result_obj = env.call_method(&future, "get", "()Ljava/lang/Object;", &[])
        .map_err(|e| format!("Failed to call get() on future: {}", e))?;

    let result = result_obj.l()
        .map_err(|e| format!("Failed to get result object: {}", e))?;

    // Check if result is null (user cancelled)
    if result.is_null() {
        return Err("User cancelled folder picker".to_string());
    }

    // Result is a FolderPickerResult object
    // Get the uri field
    let uri_obj = env.get_field(&result, "uri", "Ljava/lang/String;")
        .map_err(|e| format!("Failed to get uri field value: {}", e))?;

    // Check if uri is null and extract string
    let uri_string: String = {
        let uri_obj_l = uri_obj.l()
            .map_err(|e| format!("Failed to get uri object: {}", e))?;
        
        if uri_obj_l.is_null() {
            return Err("Folder picker returned null URI".to_string());
        }
        
        let uri_string_obj: JString = uri_obj_l.into();
        let java_str = env.get_string(&uri_string_obj)
            .map_err(|e| format!("Failed to convert uri to string: {}", e))?;
        java_str.to_string_lossy().into_owned()
    };

    // Get the displayName field
    let display_name_obj = env.get_field(&result, "displayName", "Ljava/lang/String;")
        .map_err(|e| format!("Failed to get displayName field value: {}", e))?;

    // Check if displayName is null and extract string
    let display_name = {
        let display_name_obj_l = display_name_obj.l()
            .map_err(|e| format!("Failed to get displayName object: {}", e))?;
        
        if display_name_obj_l.is_null() {
            None
        } else {
            let display_name_string_obj: JString = display_name_obj_l.into();
            let java_str = env.get_string(&display_name_string_obj)
                .map_err(|e| format!("Failed to convert displayName to string: {}", e))?;
            Some(java_str.to_string_lossy().into_owned())
        }
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
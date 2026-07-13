//! Android folder picker using Storage Access Framework (SAF) via JNI

use std::sync::mpsc;
use tauri::AppHandle;

/// Result from the folder picker
#[derive(Debug, Clone)]
pub struct FolderPickerResult {
    pub uri: String,
    pub display_name: Option<String>,
}

/// Opens the Android Storage Access Framework folder picker.
/// Returns a content:// URI with persistable URI permission.
#[cfg(target_os = "android")]
pub fn pick_folder(app: &AppHandle) -> Result<FolderPickerResult, String> {
    use jni::objects::{GlobalRef, JObject, JString, JValue};
    use jni::sys::{jboolean, jint, jlong};
    use jni::JNIEnv;

    // Create a channel to receive the result from the JNI callback
    let (tx, rx) = mpsc::channel::<Result<FolderPickerResult, String>>();

    // Get the JavaVM and Activity from ndk_context
    let ctx = ndk_context::android_context();
    let vm = unsafe { jni::JavaVM::from_raw(ctx.vm() as *mut _) }
        .map_err(|e| format!("Failed to get JavaVM: {}", e))?;
    let activity = ctx.context_jobject();

    // Clone the sender for use in the callback
    let tx_clone = tx.clone();

    // Get the main activity class
    let mut env = vm.attach_current_thread()
        .map_err(|e| format!("Failed to attach thread: {}", e))?;

    // We'll use a custom class for the folder picker callback
    // First, let's get the activity
    let activity_obj = unsafe { JObject::from_raw(activity as *mut _) };

    // Create an intent for ACTION_OPEN_DOCUMENT_TREE
    let intent_class = env.find_class("android/content/Intent")
        .map_err(|e| format!("Failed to find Intent class: {}", e))?;
    
    let action_open_document_tree = env.new_string("android.intent.action.OPEN_DOCUMENT_TREE")
        .map_err(|e| format!("Failed to create action string: {}", e))?;
    
    let intent = env.new_object(
        &intent_class,
        "(Ljava/lang/String;)V",
        &[JValue::Object(&action_open_document_tree)]
    ).map_err(|e| format!("Failed to create Intent: {}", e))?;

    // Add flags for persistence and read permission
    let flags = 0x00000001 | 0x00000080 | 0x00000100 | 0x00002000; // FLAG_GRANT_READ_URI_PERMISSION | FLAG_GRANT_PERSISTABLE_URI_PERMISSION | FLAG_GRANT_PREFIX_URI_PERMISSION
    env.call_method(
        &intent,
        "addFlags",
        "(I)V",
        &[JValue::Int(flags)]
    ).map_err(|e| format!("Failed to add flags: {}", e))?;

    // Create a callback handler using a custom class
    // We'll use a simpler approach: create an ActivityResultCallback
    let activity_result_callback_class = env.find_class("androidx/activity/result/ActivityResultCallback")
        .map_err(|e| format!("Failed to find ActivityResultCallback: {}", e))?;
    
    // Create a simple callback implementation
    let callback_class_name = "app/bmdarklight/wave/FolderPickerCallback";
    let callback_class = env.find_class(callback_class_name)
        .map_err(|e| format!("Failed to find FolderPickerCallback class: {}. Make sure the Java class exists.", e))?;
    
    // Create an instance of our callback
    let callback = env.new_object(
        &callback_class,
        "(Ljava/lang/Object;)V",
        &[JValue::Object(&tx_clone)]
    ).map_err(|e| format!("Failed to create FolderPickerCallback: {}", e))?;

    // Register for activity result
    let activity_class = env.find_class("androidx/activity/ComponentActivity")
        .map_err(|e| format!("Failed to find ComponentActivity: {}", e))?;
    
    let register_method = env.get_method_id(
        &activity_class,
        "registerForActivityResult",
        "(Landroidx/activity/result/ActivityResultContract;Landroidx/activity/result/ActivityResultCallback;)Landroidx/activity/result/ActivityResultLauncher;"
    ).map_err(|e| format!("Failed to get registerForActivityResult method: {}", e))?;

    // Get the ActivityResultContracts.OpenDocumentTree contract
    let contracts_class = env.find_class("androidx/activity/result/contract/ActivityResultContracts$OpenDocumentTree")
        .map_err(|e| format!("Failed to find ActivityResultContracts$OpenDocumentTree: {}", e))?;
    
    let contract_instance = env.call_static_method(
        &contracts_class,
        "new",
        "()Landroidx/activity/result/contract/ActivityResultContracts$OpenDocumentTree;"
    ).map_err(|e| format!("Failed to create OpenDocumentTree contract: {}", e))?
    .l()
    .map_err(|e| format!("Failed to get contract instance: {}", e))?;

    let launcher = env.call_method(
        &activity_obj,
        "registerForActivityResult",
        "(Landroidx/activity/result/ActivityResultContract;Landroidx/activity/result/ActivityResultCallback;)Landroidx/activity/result/ActivityResultLauncher;",
        &[JValue::Object(&contract_instance), JValue::Object(&callback)]
    ).map_err(|e| format!("Failed to register for activity result: {}", e))?
    .l()
    .map_err(|e| format!("Failed to get launcher: {}", e))?;

    // Launch the picker
    let launch_method = env.get_method_id(
        env.get_object_class(&launcher).as_ref(),
        "launch",
        "(Ljava/lang/Object;)V"
    ).map_err(|e| format!("Failed to get launch method: {}", e))?;

    env.call_method(
        &launcher,
        "launch",
        "(Ljava/lang/Object;)V",
        &[JValue::Object(&intent)]
    ).map_err(|e| format!("Failed to launch folder picker: {}", e))?;

    // Wait for the result with a timeout
    match rx.recv_timeout(std::time::Duration::from_secs(60)) {
        Ok(result) => result,
        Err(_) => Err("Folder picker timed out".to_string()),
    }
}

/// Fallback for non-Android targets
#[cfg(not(target_os = "android"))]
pub fn pick_folder(_app: &AppHandle) -> Result<FolderPickerResult, String> {
    Err("Folder picker is only available on Android".to_string())
}
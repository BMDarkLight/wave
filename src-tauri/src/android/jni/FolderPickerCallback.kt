package app.bmdarklight.wave

import android.app.Activity
import android.content.ContentResolver
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.util.Log
import androidx.activity.result.ActivityResult
import androidx.activity.result.ActivityResultCallback
import androidx.activity.result.ActivityResultLauncher
import androidx.activity.result.contract.ActivityResultContracts
import java.util.concurrent.CompletableFuture

/**
 * Folder picker callback for Android Storage Access Framework (SAF).
 * Uses ACTION_OPEN_DOCUMENT_TREE to let user select a directory.
 */
class FolderPickerCallback(private val activity: Activity) : ActivityResultCallback<ActivityResult> {

    private var launcher: ActivityResultLauncher<Intent>? = null
    private var currentFuture: CompletableFuture<FolderPickerResult?>? = null

    init {
        launcher = activity.registerForActivityResult(
            ActivityResultContracts.StartActivityForResult()
        ) { result ->
            val future = currentFuture
            currentFuture = null
            
            if (result.resultCode == Activity.RESULT_OK) {
                val uri = result.data?.data
                if (uri != null) {
                    // Take persistable URI permission
                    val flags = Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION
                    try {
                        activity.contentResolver.takePersistableUriPermission(uri, flags)
                    } catch (e: SecurityException) {
                        Log.e("FolderPicker", "Failed to persist URI permission", e)
                    }
                    val displayName = getDisplayName(uri)
                    future?.complete(FolderPickerResult(uri.toString(), displayName))
                } else {
                    future?.complete(null)
                }
            } else {
                future?.complete(null)
            }
        }
    }

    private fun getDisplayName(uri: Uri): String? {
        return try {
            val cursor = activity.contentResolver.query(uri, null, null, null, null)
            cursor?.use {
                if (it.moveToFirst()) {
                    val nameIndex = it.getColumnIndex(android.provider.OpenableColumns.DISPLAY_NAME)
                    if (nameIndex >= 0) {
                        it.getString(nameIndex)
                    } else {
                        null
                    }
                } else {
                    null
                }
            }
        } catch (e: Exception) {
            Log.w("FolderPicker", "Failed to get display name", e)
            null
        }
    }

    /**
     * Launches the folder picker and returns a CompletableFuture with the result.
     */
    fun pickFolder(): CompletableFuture<FolderPickerResult?> {
        currentFuture = CompletableFuture()
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT_TREE).apply {
            addFlags(
                Intent.FLAG_GRANT_READ_URI_PERMISSION or
                Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION or
                Intent.FLAG_GRANT_PREFIX_URI_PERMISSION
            )
        }
        launcher?.launch(intent)
        return currentFuture!!
    }

    override fun onActivityResult(result: ActivityResult) {
        // This is called by the ActivityResultCallback interface
        // The actual handling is done in the lambda above
    }

    data class FolderPickerResult(
        val uri: String,
        val displayName: String?
    )
}
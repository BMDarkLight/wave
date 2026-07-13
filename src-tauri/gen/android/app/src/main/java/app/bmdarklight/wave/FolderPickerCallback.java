package app.bmdarklight.wave;

import android.app.Activity;
import android.content.ContentResolver;
import android.content.Intent;
import android.net.Uri;
import android.os.Build;
import android.provider.DocumentsContract;
import android.util.Log;

import androidx.activity.ComponentActivity;
import androidx.activity.result.ActivityResult;
import androidx.activity.result.ActivityResultCallback;
import androidx.activity.result.ActivityResultLauncher;
import androidx.activity.result.contract.ActivityResultContracts;
import androidx.annotation.NonNull;

import java.util.concurrent.CompletableFuture;

/**
 * Folder picker callback using Storage Access Framework (SAF) on Android.
 * Uses ActivityResultContracts.StartActivityForResult for picking directories.
 */
public class FolderPickerCallback implements ActivityResultCallback<ActivityResult> {
    private static final String TAG = "FolderPickerCallback";

    private final CompletableFuture<FolderPickerResult> future = new CompletableFuture<>();
    private ActivityResultLauncher<Intent> launcher;
    private final ComponentActivity activity;

    public static class FolderPickerResult {
        public final String uri;
        public final String displayName;

        public FolderPickerResult(String uri, String displayName) {
            this.uri = uri;
            this.displayName = displayName;
        }
    }

    public FolderPickerCallback(ComponentActivity activity) {
        this.activity = activity;
    }

    /**
     * Launches the folder picker and returns a future that completes when the user selects a folder.
     */
    public CompletableFuture<FolderPickerResult> pickFolder() {
        // Register the activity result launcher if not already registered
        if (launcher == null) {
            launcher = activity.registerForActivityResult(
                new ActivityResultContracts.StartActivityForResult(),
                this
            );
        }

        // Launch the picker with ACTION_OPEN_DOCUMENT_TREE intent
        Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT_TREE);
        intent.addFlags(
            Intent.FLAG_GRANT_READ_URI_PERMISSION |
            Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION |
            Intent.FLAG_GRANT_PREFIX_URI_PERMISSION
        );
        launcher.launch(intent);

        return future;
    }

    @Override
    public void onActivityResult(ActivityResult result) {
        if (result.getResultCode() == Activity.RESULT_OK && result.getData() != null) {
            Uri treeUri = result.getData().getData();
            if (treeUri != null) {
                // Persist the URI permission
                persistUriPermission(treeUri);

                // Get display name
                String displayName = getDisplayName(treeUri);

                FolderPickerResult pickerResult = new FolderPickerResult(
                    treeUri.toString(),
                    displayName
                );

                future.complete(pickerResult);
                return;
            }
        }

        // User cancelled or error
        future.completeExceptionally(new RuntimeException("Folder picker cancelled or failed"));
    }

    /**
     * Persist the URI permission so we can access the folder across app restarts.
     */
    private void persistUriPermission(Uri uri) {
        try {
            final int takeFlags = Intent.FLAG_GRANT_READ_URI_PERMISSION
                                | Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION
                                | Intent.FLAG_GRANT_PREFIX_URI_PERMISSION;

            activity.getContentResolver().takePersistableUriPermission(uri, takeFlags);
            Log.d(TAG, "Persisted URI permission for: " + uri);
        } catch (SecurityException e) {
            Log.w(TAG, "Failed to persist URI permission: " + e.getMessage());
        }
    }

    /**
     * Get the display name of the selected folder.
     */
    private String getDisplayName(Uri uri) {
        try (android.database.Cursor cursor = activity.getContentResolver()
                .query(uri, null, null, null, null)) {
            if (cursor != null && cursor.moveToFirst()) {
                int nameIndex = cursor.getColumnIndex(DocumentsContract.Document.COLUMN_DISPLAY_NAME);
                if (nameIndex >= 0) {
                    return cursor.getString(nameIndex);
                }
            }
        } catch (Exception e) {
            Log.w(TAG, "Failed to get display name: " + e.getMessage());
        }
        return "Selected Folder";
    }
}
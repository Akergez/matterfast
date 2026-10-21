package app.akergez.matterfast;

import android.app.Activity;
import android.content.Intent;
import android.net.Uri;

import androidx.core.content.FileProvider;

import java.io.File;

/**
 * Handing a downloaded attachment to another application.
 *
 * <p>A phone has no "Save as" here (GPUI's prompt for a new path answers
 * nothing on Android), so an attachment is downloaded into the application's
 * own cache and offered to whatever opens its kind of file. A {@code file://}
 * address may not leave the application since Android 7; the file goes out
 * through {@link FileProvider} as a {@code content://} one, readable by the
 * application that was chosen and nobody else.</p>
 *
 * <p>Called from Rust ({@code open_file.rs}), on a thread of its own.</p>
 */
public final class Files {

    /** Must match the provider's authority in the manifest. */
    private static String authority(Activity activity) {
        return activity.getPackageName() + ".files";
    }

    /** Offers the file at {@code path} to the applications that open {@code mime}. */
    public static void open(Activity activity, String path, String mime) {
        Uri uri = FileProvider.getUriForFile(activity, authority(activity), new File(path));
        Intent view = new Intent(Intent.ACTION_VIEW)
                .setDataAndType(uri, mime.isEmpty() ? "application/octet-stream" : mime)
                .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION);
        // The chooser, so that there is a choice even where the person once
        // ticked "always"; and it says so itself when nothing can open it.
        Intent chooser = Intent.createChooser(view, null)
                .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION);
        activity.startActivity(chooser);
    }

    private Files() {}
}

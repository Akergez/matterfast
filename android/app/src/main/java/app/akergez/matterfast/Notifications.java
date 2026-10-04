package app.akergez.matterfast;

import android.app.Activity;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.content.Context;
import android.content.Intent;
import android.net.Uri;
import android.util.Log;

import androidx.core.app.NotificationCompat;
import androidx.core.app.NotificationManagerCompat;
import androidx.core.content.ContextCompat;

import dev.gpui.mobile.GpuiActivity;

/**
 * Message and call notifications, and the switch for {@link SessionService}.
 *
 * <p>Everything public is static and called from Rust
 * ({@code notifications.rs}), on a thread of its own.</p>
 */
public final class Notifications {

    private static final String TAG = "matterfast";

    private static final String MESSAGES = "messages";
    private static final String CALLS = "calls";

    /**
     * What a pressed notification opens the activity with, before its tag.
     * The same text is in {@code notifications.rs}.
     */
    private static final String PRESSED = "matterfast-notice:";

    /** Whether the person lets this application show notifications at all. */
    static boolean allowed(Context context) {
        return NotificationManagerCompat.from(context).areNotificationsEnabled();
    }

    /**
     * Somebody signed in, or out. The service runs for as long as there is a
     * session and notifications are allowed: with nothing to show, there is
     * nothing to stay awake for.
     */
    public static void session(Activity activity, int signedIn) {
        Context context = activity.getApplicationContext();
        Intent service = new Intent(context, SessionService.class);
        if (signedIn != 0 && allowed(context)) {
            try {
                ContextCompat.startForegroundService(context, service);
            } catch (RuntimeException e) {
                // Not allowed from the background; the next time the
                // application is in front and has something to show, it is.
                Log.w(TAG, "could not start the session service: " + e);
            }
        } else {
            context.stopService(service);
            if (signedIn == 0) {
                NotificationManagerCompat.from(context).cancelAll();
            }
        }
    }

    /** Raises a notification, in place of the last one with the same tag. */
    public static void show(Activity activity, String tag, String title, String body, int urgent) {
        Context context = activity.getApplicationContext();
        if (!allowed(context)) {
            // Taken away since the service was started.
            context.stopService(new Intent(context, SessionService.class));
            return;
        }
        NotificationManager manager =
                (NotificationManager) context.getSystemService(Context.NOTIFICATION_SERVICE);
        if (manager == null) {
            return;
        }

        String channel = urgent != 0 ? CALLS : MESSAGES;
        manager.createNotificationChannel(new NotificationChannel(
                channel,
                urgent != 0 ? "Calls" : "Messages",
                NotificationManager.IMPORTANCE_HIGH));

        Intent open = new Intent(context, GpuiActivity.class)
                .setAction(Intent.ACTION_VIEW)
                .setData(Uri.parse(PRESSED + tag))
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_SINGLE_TOP);
        PendingIntent pressed = PendingIntent.getActivity(
                context,
                tag.hashCode(),
                open,
                PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);

        NotificationCompat.Builder builder = new NotificationCompat.Builder(context, channel)
                .setSmallIcon(R.drawable.ic_notification)
                .setContentTitle(title)
                .setContentText(body)
                .setStyle(new NotificationCompat.BigTextStyle().bigText(body))
                .setCategory(urgent != 0
                        ? NotificationCompat.CATEGORY_CALL
                        : NotificationCompat.CATEGORY_MESSAGE)
                .setPriority(NotificationCompat.PRIORITY_HIGH)
                .setContentIntent(pressed)
                .setAutoCancel(true);

        manager.notify(tag, 0, builder.build());
    }

    /** Takes a notification back down, if it is still up. */
    public static void withdraw(Activity activity, String tag) {
        NotificationManagerCompat.from(activity.getApplicationContext()).cancel(tag, 0);
    }

    private Notifications() {}
}

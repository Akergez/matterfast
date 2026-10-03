package io.gitlab.akergez.matterfast;

import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.app.Service;
import android.content.Intent;
import android.content.pm.ServiceInfo;
import android.os.Build;
import android.os.IBinder;

import androidx.core.app.NotificationCompat;
import androidx.core.app.ServiceCompat;

import dev.gpui.mobile.GpuiActivity;

/**
 * Keeps the process awake while somebody is signed in.
 *
 * <p>It does nothing itself: the connection to the server is the native
 * library's, in this same process. What it does is exist — a process with a
 * foreground service is not put to sleep when its activity leaves the screen,
 * so messages go on arriving and can be announced.</p>
 *
 * <p>It runs only while notifications are allowed. Without them there is
 * nothing it could announce, and its own notice could not be shown either.</p>
 */
public final class SessionService extends Service {

    private static final String CHANNEL = "connection";
    private static final int NOTICE = 1;

    @Override
    public int onStartCommand(Intent intent, int flags, int startId) {
        NotificationManager manager =
                (NotificationManager) getSystemService(NOTIFICATION_SERVICE);
        if (manager != null) {
            // Low: it sits in the shade without a sound or an icon up top.
            manager.createNotificationChannel(new NotificationChannel(
                    CHANNEL, "Connection", NotificationManager.IMPORTANCE_LOW));
        }

        PendingIntent open = PendingIntent.getActivity(
                this,
                0,
                new Intent(this, GpuiActivity.class)
                        .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_SINGLE_TOP),
                PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);

        // The system insists on this within seconds of the service being
        // started, allowed or not, so it comes before the check.
        ServiceCompat.startForeground(
                this,
                NOTICE,
                new NotificationCompat.Builder(this, CHANNEL)
                        .setSmallIcon(R.drawable.ic_notification)
                        .setContentTitle("Matterfast")
                        .setContentText("Connected, to notify you of new messages")
                        .setContentIntent(open)
                        .setOngoing(true)
                        .setShowWhen(false)
                        .setPriority(NotificationCompat.PRIORITY_LOW)
                        .build(),
                Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE
                        ? ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE
                        : 0);

        if (!Notifications.allowed(this)) {
            stopSelf();
        }
        // Started again without the activity there would be no native
        // library to keep awake.
        return START_NOT_STICKY;
    }

    /** Swiped out of the recent applications: the session goes with it. */
    @Override
    public void onTaskRemoved(Intent rootIntent) {
        stopSelf();
    }

    @Override
    public IBinder onBind(Intent intent) {
        return null;
    }
}

package space.foid.chord.update

import android.Manifest
import android.annotation.SuppressLint
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.NetworkType
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import space.foid.chord.BuildConfig
import space.foid.chord.MainActivity
import space.foid.chord.R
import space.foid.chord.data.logWarn
import space.foid.chord.ui.settings.PrefsSettingsStore
import space.foid.chord.ui.settings.SettingsStore
import java.util.concurrent.TimeUnit

/** The daily check. It posts a notification once for each new build. */
class UpdateWorker(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {
    override suspend fun doWork(): Result {
        val ctx = applicationContext
        val updater = Updater.get(ctx)
        if (!updater.enabled || !PrefsSettingsStore.get(ctx).prefs.value.autoUpdateCheck) return Result.success()
        val result = withContext(Dispatchers.IO) { updater.checkBlocking() }
        when (result) {
            is CheckResult.Available -> {
                val build = result.manifest.build
                if (shouldNotify(build, updater.lastNotified)) {
                    UpdateNotifications.post(ctx, result.manifest.version)
                    updater.markNotified(build)
                }
            }
            is CheckResult.Failed ->
                if (result.reason == UpdateFailure.Network && runAttemptCount < MAX_RETRIES) return Result.retry()
            else -> Unit
        }
        return Result.success()
    }

    private companion object {
        const val MAX_RETRIES = 3
    }
}

/** Starts and stops the daily check. */
object UpdateScheduler {
    private const val WORK = "chord-update-check"

    /** Follows the "Check automatically" setting. Does nothing in a build without the updater. */
    fun attach(context: Context, store: SettingsStore, scope: CoroutineScope) {
        if (!updaterEnabled(BuildConfig.UPDATER, BuildConfig.CHANNEL)) return
        val app = context.applicationContext
        UpdateNotifications.createChannel(app)
        scope.launch {
            store.prefs.map { it.autoUpdateCheck }.distinctUntilChanged().collect { auto ->
                try {
                    sync(app, auto)
                } catch (e: Exception) {
                    logWarn("UpdateScheduler", "could not schedule the update check", e)
                }
            }
        }
    }

    private fun sync(context: Context, auto: Boolean) {
        val wm = WorkManager.getInstance(context)
        if (auto) {
            val request = PeriodicWorkRequestBuilder<UpdateWorker>(1, TimeUnit.DAYS)
                .setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build())
                .build()
            wm.enqueueUniquePeriodicWork(WORK, ExistingPeriodicWorkPolicy.KEEP, request)
        } else {
            wm.cancelUniqueWork(WORK)
        }
    }
}

/** The "Updates" notification channel and its one notification. */
object UpdateNotifications {
    const val CHANNEL = "updates"
    private const val ID = 3

    /** The action of the MainActivity intent that opens Settings, About. */
    const val ACTION_OPEN_UPDATES = "space.foid.chord.action.OPEN_UPDATES"

    fun createChannel(context: Context) {
        val nm = context.getSystemService(NotificationManager::class.java)
        nm.createNotificationChannel(
            NotificationChannel(CHANNEL, context.getString(R.string.updates_channel_name), NotificationManager.IMPORTANCE_DEFAULT),
        )
    }

    @SuppressLint("MissingPermission")
    fun post(context: Context, version: String) {
        if (Build.VERSION.SDK_INT >= 33 &&
            ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) return
        createChannel(context)
        val open = PendingIntent.getActivity(
            context,
            ID,
            Intent(context, MainActivity::class.java)
                .setAction(ACTION_OPEN_UPDATES)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        val n = NotificationCompat.Builder(context, CHANNEL)
            .setSmallIcon(R.drawable.ic_notification)
            .setColor(ContextCompat.getColor(context, R.color.notification_accent))
            .setContentTitle(context.getString(R.string.updates_notification_title))
            .setContentText(context.getString(R.string.updates_notification_text, version))
            .setCategory(NotificationCompat.CATEGORY_RECOMMENDATION)
            .setAutoCancel(true)
            .setContentIntent(open)
            .build()
        try {
            NotificationManagerCompat.from(context).notify(ID, n)
        } catch (_: SecurityException) {
            // The user revoked the permission just now.
        }
    }
}

/**
 * A request from the update notification to show Settings, About. MainActivity sets it. The nav
 * host opens the settings, and the settings open the About page and clear it.
 */
object OpenUpdatesRequest {
    private val _pending = MutableStateFlow(false)
    val pending: StateFlow<Boolean> = _pending.asStateFlow()

    fun offer() {
        _pending.value = true
    }

    fun consume() {
        _pending.value = false
    }

    /** Whether [intent] came from the update notification. */
    fun matches(intent: Intent?): Boolean = intent?.action == UpdateNotifications.ACTION_OPEN_UPDATES
}

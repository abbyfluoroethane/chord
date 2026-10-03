package space.foid.chord.notify

import android.Manifest
import android.annotation.SuppressLint
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.app.Person
import androidx.core.content.ContextCompat
import space.foid.chord.MainActivity
import space.foid.chord.R

/** Channels, and the notifications for incoming messages. */
object ChordNotifications {
    const val CHANNEL_DM = "dm"
    const val CHANNEL_MENTION = "mention"
    const val CHANNEL_OTHER = "other"
    const val CHANNEL_SERVICE = "service"

    /** Extra on the MainActivity intent: the bare JID of the conversation to open. */
    const val EXTRA_PEER = "space.foid.chord.extra.PEER"

    const val ACTION_MARK_READ = "space.foid.chord.action.MARK_READ"

    const val SERVICE_NOTIFICATION_ID = 1
    private const val SUMMARY_ID = 2
    private const val GROUP = "space.foid.chord.messages"
    private const val MAX_LINES = 8

    /**
     * The conversation that is open on screen, or null. The UI sets it in onResume and clears it
     * in onPause. Messages for this peer do not notify.
     */
    @Volatile
    var activePeer: String? = null

    private class Line(val text: String, val time: Long, val sender: String, val senderName: String)

    private val history = HashMap<String, ArrayDeque<Line>>()

    /** A direct chat is a DM. A room message that mentions us is a mention. All else is "other". */
    fun channelFor(room: String?, mention: Boolean): String = when {
        room == null -> CHANNEL_DM
        mention -> CHANNEL_MENTION
        else -> CHANNEL_OTHER
    }

    /** Whether a notification for [peer] should be skipped, because its chat is on screen. */
    fun isSuppressed(peer: String, active: String?): Boolean = active != null && active == peer

    fun createChannels(context: Context) {
        val nm = context.getSystemService(NotificationManager::class.java)
        nm.createNotificationChannels(
            listOf(
                NotificationChannel(CHANNEL_DM, "Direct messages", NotificationManager.IMPORTANCE_HIGH),
                NotificationChannel(CHANNEL_MENTION, "Mentions", NotificationManager.IMPORTANCE_HIGH),
                NotificationChannel(CHANNEL_OTHER, "Other messages", NotificationManager.IMPORTANCE_DEFAULT),
                NotificationChannel(CHANNEL_SERVICE, "Connection", NotificationManager.IMPORTANCE_LOW).apply {
                    setShowBadge(false)
                },
            ),
        )
    }

    fun openAppIntent(context: Context, peer: String?): PendingIntent {
        val intent = Intent(context, MainActivity::class.java)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP)
        if (peer != null) intent.putExtra(EXTRA_PEER, peer)
        return PendingIntent.getActivity(
            context,
            peer?.hashCode() ?: 0,
            intent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
    }

    fun serviceNotification(context: Context, text: String): Notification =
        NotificationCompat.Builder(context, CHANNEL_SERVICE)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle("Chord")
            .setContentText(text)
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setSilent(true)
            .setCategory(NotificationCompat.CATEGORY_SERVICE)
            .setContentIntent(openAppIntent(context, null))
            .build()

    /** Show a notification for an incoming message, unless its chat is on screen. */
    @Synchronized
    fun post(context: Context, n: uniffi.chord_ffi.Notification) {
        if (isSuppressed(n.peer, activePeer)) return
        val app = context.applicationContext
        val lines = history.getOrPut(n.peer) { ArrayDeque() }
        lines.addLast(Line(n.bodyPreview, System.currentTimeMillis(), n.sender, n.senderName))
        while (lines.size > MAX_LINES) lines.removeFirst()
        if (!canNotify(app)) return

        val me = Person.Builder().setName("You").build()
        val style = NotificationCompat.MessagingStyle(me)
        val isRoom = n.room != null
        if (isRoom) {
            style.setConversationTitle(n.peer).setGroupConversation(true)
        }
        for (l in lines) {
            val person = Person.Builder().setName(l.senderName.ifBlank { l.sender }).setKey(l.sender).build()
            style.addMessage(l.text, l.time, person)
        }

        val markRead = PendingIntent.getBroadcast(
            app,
            n.peer.hashCode(),
            Intent(app, MarkReadReceiver::class.java)
                .setAction(ACTION_MARK_READ)
                .putExtra(EXTRA_PEER, n.peer),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        val notification = NotificationCompat.Builder(app, channelFor(n.room, n.mention))
            .setSmallIcon(R.drawable.ic_notification)
            .setStyle(style)
            .setCategory(NotificationCompat.CATEGORY_MESSAGE)
            .setGroup(GROUP)
            .setAutoCancel(true)
            .setContentIntent(openAppIntent(app, n.peer))
            .setDeleteIntent(null)
            .addAction(0, "Mark as read", markRead)
            .build()

        val nmc = NotificationManagerCompat.from(app)
        notifySafely(nmc, n.peer.hashCode(), notification)
        val summary = NotificationCompat.Builder(app, channelFor(n.room, n.mention))
            .setSmallIcon(R.drawable.ic_notification)
            .setGroup(GROUP)
            .setGroupSummary(true)
            .setAutoCancel(true)
            .setContentIntent(openAppIntent(app, null))
            .build()
        notifySafely(nmc, SUMMARY_ID, summary)
    }

    /** Remove the notification of [peer], and the group summary if nothing else is left. */
    @Synchronized
    fun cancel(context: Context, peer: String) {
        history.remove(peer)
        val nm = context.getSystemService(NotificationManager::class.java)
        nm.cancel(peer.hashCode())
        val others = nm.activeNotifications.any { it.id != SUMMARY_ID && it.id != SERVICE_NOTIFICATION_ID && it.id != peer.hashCode() }
        if (!others) nm.cancel(SUMMARY_ID)
    }

    @SuppressLint("MissingPermission")
    private fun notifySafely(nmc: NotificationManagerCompat, id: Int, n: Notification) {
        try {
            nmc.notify(id, n)
        } catch (_: SecurityException) {
            // The user revoked the permission just now.
        }
    }

    @SuppressLint("InlinedApi")
    private fun canNotify(context: Context): Boolean =
        android.os.Build.VERSION.SDK_INT < 33 ||
            ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) ==
            PackageManager.PERMISSION_GRANTED
}

package space.foid.chord.service

import android.app.NotificationManager
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import androidx.core.app.ServiceCompat
import androidx.lifecycle.DefaultLifecycleObserver
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.LifecycleService
import androidx.lifecycle.ProcessLifecycleOwner
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.lifecycleScope
import kotlinx.coroutines.launch
import space.foid.chord.ChordApp
import space.foid.chord.data.ChordSession
import space.foid.chord.notify.ChordNotifications
import uniffi.chord_ffi.ClientEvent
import uniffi.chord_ffi.ConnectionState

/**
 * Holds the XMPP connection while the user is signed in.
 *
 * Foreground service type: `specialUse`. Android 14+ makes every foreground service declare a
 * type. None of the fixed types describes a chat socket that stays open for days:
 *  - `dataSync` is for finite transfers. On Android 15+ it has a limit of 6 hours in 24, then
 *    the system calls onTimeout and the connection would drop.
 *  - `remoteMessaging` is for moving messages between devices, not for a client-server socket.
 *  - `specialUse` has no time limit. It needs the FOREGROUND_SERVICE_SPECIAL_USE permission and
 *    a manifest property that says what the service does (the subtype). Google Play review
 *    reads that text. This is a v1 design. XEP-0357 push is to replace the long-lived socket.
 */
class ChordConnectionService : LifecycleService() {

    private lateinit var session: ChordSession

    private val processObserver = object : DefaultLifecycleObserver {
        override fun onStart(owner: LifecycleOwner) = setActive(true)

        override fun onStop(owner: LifecycleOwner) = setActive(false)
    }

    override fun onCreate() {
        super.onCreate()
        session = (application as ChordApp).session
        ChordNotifications.createChannels(this)
        enterForeground(statusText(session.connection.value))

        lifecycleScope.launch {
            session.connection.collect { state ->
                getSystemService(NotificationManager::class.java).notify(
                    ChordNotifications.SERVICE_NOTIFICATION_ID,
                    ChordNotifications.serviceNotification(this@ChordConnectionService, statusText(state)),
                )
                // Tell the server again after a reconnect: the new stream starts active.
                if (state is ConnectionState.Connected) {
                    setActive(isAppVisible())
                }
            }
        }
        lifecycleScope.launch {
            session.events.collect { event ->
                if (event is ClientEvent.Notification) {
                    ChordNotifications.post(this@ChordConnectionService, event.notification)
                }
            }
        }
        ProcessLifecycleOwner.get().lifecycle.addObserver(processObserver)
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        super.onStartCommand(intent, flags, startId)
        // After a process restart there is no client yet. Sign in with the saved credentials.
        if (session.client.value == null) {
            lifecycleScope.launch {
                val restored = try {
                    // The same offline-first start as the activity: wait for ChordApp.startup.
                    (application as ChordApp).startup.await()
                } catch (_: Exception) {
                    false
                }
                if (!restored && session.client.value == null) stopSelf()
            }
        }
        return START_STICKY
    }

    override fun onDestroy() {
        ProcessLifecycleOwner.get().lifecycle.removeObserver(processObserver)
        super.onDestroy()
    }

    private fun isAppVisible() =
        ProcessLifecycleOwner.get().lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED)

    private fun setActive(active: Boolean) {
        val client = session.client.value ?: return
        lifecycleScope.launch {
            try {
                client.setClientActive(active)
            } catch (_: Exception) {
                // Not connected. The next Connected state sends it again.
            }
        }
    }

    private fun enterForeground(text: String) {
        val type = if (Build.VERSION.SDK_INT >= 34) ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE else 0
        ServiceCompat.startForeground(
            this,
            ChordNotifications.SERVICE_NOTIFICATION_ID,
            ChordNotifications.serviceNotification(this, text),
            type,
        )
    }

    companion object {
        /** The text of the persistent notification for a connection state. */
        fun statusText(state: ConnectionState?): String = when (state) {
            is ConnectionState.Connected -> "Connected"
            ConnectionState.Connecting, null -> "Connecting…"
            ConnectionState.Suspended -> "Offline, retrying"
            is ConnectionState.AuthFailed -> "Sign-in needed"
            is ConnectionState.LoginFailed ->
                if (state.failure is uniffi.chord_ffi.ConnectFailure.AuthFailed) "Sign-in failed" else "Offline, retrying"
            ConnectionState.Disconnected -> "Offline"
        }
    }
}

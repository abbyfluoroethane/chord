package space.foid.chord.ui.settings

import androidx.lifecycle.DefaultLifecycleObserver
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.ProcessLifecycleOwner
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.launch
import space.foid.chord.data.ChordSession
import space.foid.chord.data.logWarn
import uniffi.chord_ffi.Availability
import uniffi.chord_ffi.ChordClient
import uniffi.chord_ffi.ConnectionState
import uniffi.chord_ffi.OwnPresence

/** [PresenceApi] on a [ChordClient]. */
class ClientPresenceApi(private val client: ChordClient) : PresenceApi {
    override suspend fun ownPresence(): OwnPresence = client.ownPresence()
    override suspend fun setPresence(availability: Availability, status: String?) =
        client.setPresence(availability, status)
    override suspend fun setIdle(since: Long?) = client.setIdle(since)
}

/**
 * Puts the phone settings to work on the session. It lives as long as the process.
 *
 * - After each login of a client, it sets the sign-in availability and status (once per client,
 *   not after each reconnect, so a status that the user picks later stays).
 * - When the app has been in the background for the idle time, it tells the contacts that we
 *   are idle. It clears that when the app comes back.
 */
class SettingsApplier(
    private val session: ChordSession,
    private val store: SettingsStore,
    private val scope: CoroutineScope,
    private val nowSeconds: () -> Long = { System.currentTimeMillis() / 1000 },
) {
    private var appliedFor: ChordClient? = null
    private var idleJob: Job? = null
    private var idleSent = false
    private var inBackground = false

    fun attach(lifecycle: Lifecycle = ProcessLifecycleOwner.get().lifecycle) {
        scope.launch {
            combine(session.client, session.connection) { c, s -> c to s }.collect { (client, state) ->
                if (client == null) {
                    appliedFor = null
                    idleSent = false
                } else if (state is ConnectionState.Connected && appliedFor !== client) {
                    appliedFor = client
                    try {
                        applySignInPresence(ClientPresenceApi(client), store.prefs.value)
                    } catch (e: CancellationException) {
                        throw e
                    } catch (e: Exception) {
                        logWarn("SettingsApplier", "sign-in presence failed", e)
                    }
                }
            }
        }
        scope.launch {
            store.prefs.collect { p ->
                // Turned off while idle: say that we are back. Turned on in the background: start the wait.
                if (!p.shareIdle) {
                    idleJob?.cancel()
                    if (idleSent) sendIdle(null)
                } else if (inBackground && !idleSent) {
                    startIdleWait(p.idleMinutes)
                }
            }
        }
        // Lifecycle observers must be added on the main thread.
        scope.launch(kotlinx.coroutines.Dispatchers.Main) {
            lifecycle.addObserver(object : DefaultLifecycleObserver {
                override fun onStop(owner: LifecycleOwner) {
                    inBackground = true
                    val p = store.prefs.value
                    if (p.shareIdle) startIdleWait(p.idleMinutes)
                }

                override fun onStart(owner: LifecycleOwner) {
                    inBackground = false
                    idleJob?.cancel()
                    if (idleSent) scope.launch { sendIdle(null) }
                }
            })
        }
    }

    private fun startIdleWait(minutes: Int) {
        idleJob?.cancel()
        idleJob = scope.launch {
            val since = nowSeconds()
            delay(minutes * 60_000L)
            if (store.prefs.value.shareIdle) sendIdle(since)
        }
    }

    private suspend fun sendIdle(since: Long?) {
        val client = session.client.value ?: return
        try {
            client.setIdle(since)
            idleSent = since != null
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            logWarn("SettingsApplier", "setIdle failed", e)
        }
    }
}

package space.foid.chord.ui.inbox

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import uniffi.chord_ffi.ChordClient
import uniffi.chord_ffi.ClientEvent

/**
 * Keeps the contact requests and room invitations that arrive before the main screen exists.
 * The core sends the requests that wait on the server right after the login, while the sign-in
 * screen still shows. [ChordSession.events][space.foid.chord.data.ChordSession.events] has no
 * replay, so the inbox would miss them. MainActivity calls [attach] at start. The inbox reads
 * [flow], which replays what arrived since the sign-in. Signing out clears it.
 */
object InboxEvents {
    private val _flow = MutableSharedFlow<ClientEvent>(
        replay = 64, extraBufferCapacity = 64, onBufferOverflow = BufferOverflow.DROP_OLDEST,
    )

    /** Requests and invitations only. */
    val flow: SharedFlow<ClientEvent> = _flow

    private var job: Job? = null
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)

    /** Start to listen. A second call does nothing. */
    @Synchronized
    fun attach(events: SharedFlow<ClientEvent>, client: StateFlow<ChordClient?>) {
        if (job?.isActive == true) return
        job = scope.launch {
            launch { client.collect { if (it == null) _flow.resetReplayCache() } }
            events.collect { e ->
                if (e is ClientEvent.SubscriptionRequest || e is ClientEvent.RoomInvite) _flow.emit(e)
            }
        }
    }
}

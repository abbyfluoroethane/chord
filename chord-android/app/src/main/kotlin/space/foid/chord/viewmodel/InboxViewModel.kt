package space.foid.chord.viewmodel

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.filterIsInstance
import kotlinx.coroutines.flow.merge
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import space.foid.chord.ChordApp
import space.foid.chord.data.ConversationApi
import space.foid.chord.data.conversationApi
import space.foid.chord.data.logWarn
import space.foid.chord.ui.inbox.InboxEvents
import uniffi.chord_ffi.ChordException
import uniffi.chord_ffi.ClientEvent
import uniffi.chord_ffi.PendingSpaceJoin

/** A contact that asks to see our presence. [addBack] says to ask for theirs too when we accept. */
data class RequestItem(val jid: String, val addBack: Boolean = true) {
    val id: String get() = "request/$jid"
}

/** An invitation to a room. */
data class InviteItem(val room: String, val from: String, val reason: String?, val password: String?) {
    val id: String get() = "invite/$room"
}

/** The inbox: what waits for an answer. It lives in memory for the session. */
data class InboxState(
    val requests: List<RequestItem> = emptyList(),
    val invites: List<InviteItem> = emptyList(),
    /** Spaces that we asked to join. The owner has not answered. They do not count in the badge. */
    val pendingSpaces: List<PendingSpaceJoin> = emptyList(),
    /** The ids of the items with a call on the way. */
    val busy: Set<String> = emptySet(),
    val error: String? = null,
) {
    /** The number on the badge: requests and invites. */
    val count: Int get() = requests.size + invites.size
    val isEmpty: Boolean get() = requests.isEmpty() && invites.isEmpty() && pendingSpaces.isEmpty()
}

/**
 * Collects the contact requests and room invites of the session from [events], and answers them.
 * It also forwards the `Notice` events as [JoinEvent.Message] for the snackbar. It must live as
 * long as the main screen, because the core sends each event once.
 */
class InboxViewModel(
    private val api: () -> ConversationApi?,
    events: Flow<ClientEvent>,
    private val scope: CoroutineScope = mainScope(),
) : ViewModel(scope) {
    private val _state = MutableStateFlow(InboxState())
    val state: StateFlow<InboxState> = _state.asStateFlow()

    private val _events = MutableSharedFlow<JoinEvent>(extraBufferCapacity = 16)

    /** [JoinEvent.Message] for each notice, and [JoinEvent.OpenChannel] after an accepted invite. */
    val events: SharedFlow<JoinEvent> = _events.asSharedFlow()

    init {
        scope.launch {
            events.collect { e ->
                when (e) {
                    is ClientEvent.SubscriptionRequest -> onRequest(e.jid)
                    is ClientEvent.RoomInvite -> onInvite(InviteItem(e.room, e.from, e.reason, e.password))
                    is ClientEvent.Notice -> _events.tryEmit(JoinEvent.Message(e.text))
                    else -> Unit
                }
            }
        }
        refreshPending()
    }

    private fun requireApi(): ConversationApi = api() ?: throw ChordException.NotConnected()

    private fun onRequest(jid: String) = _state.update { s ->
        if (s.requests.any { it.jid.equals(jid, ignoreCase = true) }) s else s.copy(requests = s.requests + RequestItem(jid))
    }

    private fun onInvite(invite: InviteItem) = _state.update { s ->
        // A new invite to the same room replaces the old one: it may hold a new reason or password.
        s.copy(invites = s.invites.filterNot { it.room == invite.room } + invite)
    }

    /** Read the spaces that wait for their owner again. Call it when the inbox opens. */
    fun refreshPending() {
        scope.launch {
            try {
                val list = requireApi().pendingSpaceJoins()
                _state.update { it.copy(pendingSpaces = list) }
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                logWarn("InboxViewModel", "pending joins failed", e)
            }
        }
    }

    fun setAddBack(jid: String, addBack: Boolean) = _state.update { s ->
        s.copy(requests = s.requests.map { if (it.jid == jid) it.copy(addBack = addBack) else it })
    }

    fun clearError() = _state.update { it.copy(error = null) }

    fun accept(request: RequestItem) = answer(request.id, onDone = { r -> r.copy(requests = r.requests.filterNot { it.jid == request.jid }) }) {
        it.approveSubscription(request.jid, request.addBack)
    }

    fun deny(request: RequestItem) = answer(request.id, onDone = { r -> r.copy(requests = r.requests.filterNot { it.jid == request.jid }) }) {
        it.denySubscription(request.jid)
    }

    fun accept(invite: InviteItem) {
        answer(invite.id, onDone = { r -> r.copy(invites = r.invites.filterNot { it.room == invite.room }) }, describe = {
            (describeJoinError(it, sentPassword = invite.password != null) as? JoinFailure.Other)?.text
                ?: "This room needs a password that the invite did not give."
        }) {
            it.joinRoom(invite.room, null, invite.password)
            _events.tryEmit(JoinEvent.OpenChannel(invite.room, invite.room.substringBefore('@')))
        }
    }

    fun decline(invite: InviteItem) = answer(invite.id, onDone = { r -> r.copy(invites = r.invites.filterNot { it.room == invite.room }) }) {
        it.declineRoomInvite(invite.room, invite.from)
    }

    private fun answer(
        id: String,
        onDone: (InboxState) -> InboxState,
        describe: (Throwable) -> String = ::describeError,
        call: suspend (ConversationApi) -> Unit,
    ) {
        if (id in _state.value.busy) return
        _state.update { it.copy(busy = it.busy + id, error = null) }
        scope.launch {
            try {
                call(requireApi())
                _state.update { onDone(it).copy(busy = it.busy - id) }
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                logWarn("InboxViewModel", "answer failed", e)
                _state.update { it.copy(busy = it.busy - id, error = describe(e)) }
            }
        }
    }

    companion object {
        val factory: ViewModelProvider.Factory = viewModelFactory {
            initializer {
                val app = this[ViewModelProvider.AndroidViewModelFactory.APPLICATION_KEY] as ChordApp
                InboxViewModel(
                    { app.session.conversationApi() },
                    // Requests and invites come with replay (they can arrive before this screen). Notices do not.
                    merge(InboxEvents.flow, app.session.events.filterIsInstance<ClientEvent.Notice>()),
                )
            }
        }
    }
}

package space.foid.chord.viewmodel

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
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
import space.foid.chord.ui.join.XmppTarget
import space.foid.chord.ui.join.parseXmppUri
import space.foid.chord.ui.join.personAddressOf
import space.foid.chord.ui.join.roomTargetOf
import space.foid.chord.ui.spaces.MuteDuration
import space.foid.chord.ui.spaces.muteUntil
import uniffi.chord_ffi.ChordException
import uniffi.chord_ffi.JoinOutcome
import uniffi.chord_ffi.NotificationLevel
import uniffi.chord_ffi.SpaceInfo

/** The three parts of the new conversation sheet. */
enum class JoinTab { Room, Person, Spaces }

/** The room asked for a password. [wrong] is true when the password that we sent was refused. */
data class PasswordPrompt(val wrong: Boolean)

/** Where a space of the list stands. */
enum class SpaceStatus { Idle, Joining, Requested }

data class SpaceRow(val info: SpaceInfo, val status: SpaceStatus = SpaceStatus.Idle, val error: String? = null) {
    val key: String get() = "${info.service}|${info.node}"
}

/** The list in the "Browse spaces" part. */
sealed interface SpacesState {
    data object Idle : SpacesState
    data object Loading : SpacesState
    data class Failed(val message: String) : SpacesState
    data class Loaded(val rows: List<SpaceRow>) : SpacesState
}

/** The state of the new conversation sheet. */
data class JoinState(
    val visible: Boolean = false,
    val tab: JoinTab = JoinTab.Room,
    val address: String = "",
    val nick: String = "",
    val password: String = "",
    val passwordPrompt: PasswordPrompt? = null,
    val joining: Boolean = false,
    val roomError: String? = null,
    val personJid: String = "",
    val personName: String = "",
    val messaging: Boolean = false,
    val personError: String? = null,
    val spaces: SpacesState = SpacesState.Idle,
) {
    /** The room in the address field, or null while the text is not a room address. */
    val roomTarget: XmppTarget.Room? get() = roomTargetOf(address)
    val canJoin: Boolean
        get() = !joining && roomTarget != null && (passwordPrompt == null || password.isNotEmpty())
    val canMessage: Boolean get() = !messaging && personAddressOf(personJid) != null
}

/** What a channel row stands for in the actions sheet. */
enum class ActionKind { Room, Contact, Occupant }

/** The channel that the actions sheet is about. [jid] is `room/nick` for an occupant. */
data class ChannelActionTarget(val jid: String, val name: String, val kind: ActionKind) {
    /** The address to copy. */
    val address: String get() = jid.substringBefore('/')
}

/** The state of the channel actions sheet. [level] is null until the core answered. */
data class ActionsState(
    val target: ChannelActionTarget,
    val level: NotificationLevel? = null,
    /** The end of a timed mute, Unix time in ms. Null: no timed mute. */
    val muteUntil: Long? = null,
    val busy: Boolean = false,
    val error: String? = null,
)

/** What the main screen must do after the ViewModel finished something. */
sealed interface JoinEvent {
    /** Select this room or chat. */
    data class OpenChannel(val jid: String, val name: String, val direct: Boolean? = null) : JoinEvent

    /** Select this space in the rail. */
    data class OpenSpace(val service: String, val node: String) : JoinEvent

    /** The room or contact is gone. The screen drops its selection if it shows it. */
    data class Left(val jid: String) : JoinEvent

    /** We asked to join a space and the owner has to approve. The inbox lists it. */
    data object SpaceRequested : JoinEvent

    /** A short text for the snackbar. */
    data class Message(val text: String) : JoinEvent
}

/** Why a join failed. */
sealed interface JoinFailure {
    /** The room needs a password, or refused ours. */
    data class Password(val wrong: Boolean) : JoinFailure
    data class Other(val text: String) : JoinFailure
}

/**
 * The XMPP error condition of a server error, for example `not-authorized`. It reads the same
 * text as `ClientError::condition` in the core. Null for any other error.
 */
fun serverCondition(error: Throwable): String? {
    val text = (error as? ChordException.Server)?.detail ?: return null
    val end = text.indexOfFirst { it == ':' || it == ' ' }.let { if (it < 0) text.length else it }
    val condition = text.substring(0, end)
    val rest = text.substring(end)
    val valid = condition.isNotEmpty() && condition.all { it in 'a'..'z' || it == '-' } &&
        (rest.isEmpty() || rest.startsWith(":") || rest.startsWith(" ("))
    return if (valid) condition else null
}

/** Plain-English reason for a failed room join. [sentPassword] is true when we sent one. */
fun describeJoinError(error: Throwable, sentPassword: Boolean = false): JoinFailure = when (serverCondition(error)) {
    "not-authorized" -> JoinFailure.Password(wrong = sentPassword)
    "registration-required" -> JoinFailure.Other("Only members can join this room. Ask an admin to add you.")
    "forbidden" -> JoinFailure.Other("You are banned from this room.")
    "item-not-found" -> JoinFailure.Other("This room was not found. Check the address.")
    "conflict" -> JoinFailure.Other("Someone in this room has your nickname. Choose another nickname and try again.")
    "not-allowed" -> JoinFailure.Other("This server does not let you make this room.")
    "service-unavailable" -> JoinFailure.Other("This room is not available now. Try again later.")
    "remote-server-not-found" -> JoinFailure.Other("The server of this room was not found. Check the address.")
    "remote-server-timeout" -> JoinFailure.Other("The server of this room did not answer. Try again later.")
    "resource-constraint" -> JoinFailure.Other("This room is full.")
    else -> JoinFailure.Other(describeError(error))
}

/**
 * The state of the new conversation sheet (join a room, message someone, browse spaces) and of
 * the channel actions sheet (notifications, mark as read, leave). It calls [ConversationApi].
 *
 * @param api the API of the current client, or null when nobody is signed in
 */
class JoinViewModel(
    private val api: () -> ConversationApi?,
    private val scope: CoroutineScope = mainScope(),
) : ViewModel(scope) {
    private val _state = MutableStateFlow(JoinState())
    val state: StateFlow<JoinState> = _state.asStateFlow()

    private val _actions = MutableStateFlow<ActionsState?>(null)
    val actions: StateFlow<ActionsState?> = _actions.asStateFlow()

    private val _events = MutableSharedFlow<JoinEvent>(extraBufferCapacity = 16)
    val events: SharedFlow<JoinEvent> = _events.asSharedFlow()

    private fun requireApi(): ConversationApi = api() ?: throw ChordException.NotConnected()

    // ---- The sheet ----

    fun show(tab: JoinTab = JoinTab.Room) {
        _state.update { it.copy(visible = true, tab = tab) }
        if (tab == JoinTab.Spaces) loadSpaces()
    }

    fun dismiss() = _state.update { JoinState(spaces = it.spaces) }

    fun selectTab(tab: JoinTab) {
        _state.update { it.copy(tab = tab) }
        if (tab == JoinTab.Spaces) loadSpaces()
    }

    fun onAddress(value: String) = _state.update {
        // A new address is a new room: the password question and the old error go.
        it.copy(address = value, roomError = null, passwordPrompt = null, password = "")
    }

    fun onNick(value: String) = _state.update { it.copy(nick = value, roomError = null) }
    fun onPassword(value: String) = _state.update { it.copy(password = value, roomError = null) }
    fun onPersonJid(value: String) = _state.update { it.copy(personJid = value, personError = null) }
    fun onPersonName(value: String) = _state.update { it.copy(personName = value, personError = null) }

    // ---- Join a room ----

    fun joinRoom() {
        val s = _state.value
        if (s.joining) return
        val target = s.roomTarget
        if (target == null) {
            _state.update { it.copy(roomError = "That is not a room address. Use room@conference.example.org.") }
            return
        }
        val password = (s.password.ifEmpty { target.password }).takeIf { !it.isNullOrEmpty() }
        _state.update { it.copy(joining = true, roomError = null) }
        scope.launch {
            try {
                requireApi().joinRoom(target.jid, s.nick.trim().ifEmpty { null }, password)
                dismiss()
                _events.tryEmit(JoinEvent.OpenChannel(target.jid, target.jid.substringBefore('@')))
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                logWarn("JoinViewModel", "join failed", e)
                when (val f = describeJoinError(e, sentPassword = password != null)) {
                    is JoinFailure.Password -> _state.update {
                        it.copy(
                            joining = false,
                            passwordPrompt = PasswordPrompt(f.wrong),
                            password = "",
                            roomError = if (f.wrong) "That password was not accepted." else null,
                        )
                    }
                    is JoinFailure.Other -> _state.update { it.copy(joining = false, roomError = f.text) }
                }
            }
        }
    }

    // ---- Message someone ----

    fun messagePerson() {
        val s = _state.value
        if (s.messaging) return
        val jid = personAddressOf(s.personJid)
        if (jid == null) {
            _state.update { it.copy(personError = "That is not an address. Use name@server.example.") }
            return
        }
        _state.update { it.copy(messaging = true, personError = null) }
        scope.launch {
            try {
                val a = requireApi()
                if (jid == a.account().substringBefore('/').lowercase()) {
                    _state.update { it.copy(messaging = false, personError = "That is your own address.") }
                    return@launch
                }
                val typed = s.personName.trim().ifEmpty { null }
                val known = a.contacts().firstOrNull { it.jid.equals(jid, ignoreCase = true) }
                if (known == null) a.addContact(jid, typed)
                dismiss()
                _events.tryEmit(JoinEvent.OpenChannel(jid, known?.name ?: typed ?: jid.substringBefore('@'), direct = true))
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                logWarn("JoinViewModel", "message failed", e)
                _state.update { it.copy(messaging = false, personError = describeError(e)) }
            }
        }
    }

    // ---- Browse spaces ----

    /** Load the list of spaces. It does nothing while a list is loading or shown, unless [force]. */
    fun loadSpaces(force: Boolean = false) {
        val cur = _state.value.spaces
        if (!force && (cur is SpacesState.Loading || cur is SpacesState.Loaded)) return
        _state.update { it.copy(spaces = SpacesState.Loading) }
        scope.launch {
            try {
                val list = requireApi().browseSpaces()
                _state.update { it.copy(spaces = SpacesState.Loaded(list.map { s -> SpaceRow(s) })) }
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                logWarn("JoinViewModel", "browse failed", e)
                _state.update { it.copy(spaces = SpacesState.Failed(describeError(e))) }
            }
        }
    }

    fun joinSpace(row: SpaceRow) {
        if (row.status != SpaceStatus.Idle) return
        updateRow(row.key) { it.copy(status = SpaceStatus.Joining, error = null) }
        scope.launch {
            try {
                when (requireApi().joinSpace(row.info.service, row.info.node)) {
                    JoinOutcome.JOINED -> {
                        dismiss()
                        _events.tryEmit(JoinEvent.OpenSpace(row.info.service, row.info.node))
                    }
                    JoinOutcome.PENDING -> {
                        updateRow(row.key) { it.copy(status = SpaceStatus.Requested) }
                        _events.tryEmit(JoinEvent.SpaceRequested)
                    }
                }
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                logWarn("JoinViewModel", "join space failed", e)
                updateRow(row.key) { it.copy(status = SpaceStatus.Idle, error = describeError(e)) }
            }
        }
    }

    private fun updateRow(key: String, change: (SpaceRow) -> SpaceRow) = _state.update { s ->
        val list = s.spaces as? SpacesState.Loaded ?: return@update s
        s.copy(spaces = list.copy(rows = list.rows.map { if (it.key == key) change(it) else it }))
    }

    // ---- Links from outside ----

    /**
     * Act on an `xmpp:` URI. Chord never acts on a link alone: it fills in the sheet and shows
     * it, and the user presses the button. A bad link gives a message.
     */
    fun openXmppUri(uri: String) {
        when (val t = parseXmppUri(uri)) {
            is XmppTarget.Room -> {
                _state.update {
                    it.copy(
                        visible = true, tab = JoinTab.Room, address = t.jid, password = t.password.orEmpty(),
                        passwordPrompt = null, roomError = null, joining = false,
                    )
                }
            }
            is XmppTarget.Chat -> showPerson(t.jid, "")
            is XmppTarget.Contact -> showPerson(t.jid, t.name.orEmpty())
            is XmppTarget.Space -> show(JoinTab.Spaces)
            XmppTarget.Invalid -> _events.tryEmit(JoinEvent.Message("This link is not valid."))
        }
    }

    private fun showPerson(jid: String, name: String) = _state.update {
        it.copy(visible = true, tab = JoinTab.Person, personJid = jid, personName = name, personError = null, messaging = false)
    }

    // ---- Channel actions ----

    fun openActions(target: ChannelActionTarget) {
        _actions.value = ActionsState(target)
        scope.launch {
            try {
                val setting = requireApi().notificationSetting(target.jid)
                _actions.update { cur ->
                    cur?.takeIf { it.target == target }?.copy(level = setting.level, muteUntil = setting.muteUntil) ?: cur
                }
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                logWarn("JoinViewModel", "notification level failed", e)
                _actions.update { cur -> cur?.takeIf { it.target == target }?.copy(error = describeError(e)) ?: cur }
            }
        }
    }

    fun closeActions() {
        _actions.value = null
    }

    fun setLevel(level: NotificationLevel) {
        val cur = _actions.value ?: return
        val before = cur.level
        val beforeUntil = cur.muteUntil
        _actions.value = cur.copy(level = level, muteUntil = null, error = null)
        scope.launch {
            try {
                requireApi().setNotification(cur.target.jid, level, null)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                logWarn("JoinViewModel", "set level failed", e)
                _actions.update { now -> now?.takeIf { it.target == cur.target }?.copy(level = before, muteUntil = beforeUntil, error = describeError(e)) ?: now }
            }
        }
    }

    /**
     * Mute the chat for [duration]. A timed mute keeps the level. "Until I turn it back on" sets
     * the level to Nothing. [now] is the Unix time in ms.
     */
    fun mute(duration: MuteDuration, now: Long = System.currentTimeMillis()) {
        val cur = _actions.value ?: return
        val until = muteUntil(duration, now)
        val level = if (until == null) NotificationLevel.NONE else cur.level?.takeIf { it != NotificationLevel.NONE } ?: NotificationLevel.ALL
        val before = cur
        _actions.value = cur.copy(level = level, muteUntil = until, error = null)
        scope.launch {
            try {
                requireApi().setNotification(cur.target.jid, level, until)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                logWarn("JoinViewModel", "mute failed", e)
                _actions.update { now2 ->
                    now2?.takeIf { it.target == cur.target }?.copy(level = before.level, muteUntil = before.muteUntil, error = describeError(e)) ?: now2
                }
            }
        }
    }

    /** Turn a mute off: the level is All again, with no end time. */
    fun unmute() = setLevel(NotificationLevel.ALL)

    /**
     * Change the topic and the name of the room of the sheet. A null value stays. The topic is a
     * message to the room. Only an owner can change the name. Calls [onDone] on success.
     */
    fun saveChannel(topic: String?, name: String?, onDone: () -> Unit) {
        val cur = _actions.value ?: return
        if (cur.busy || cur.target.kind != ActionKind.Room) return
        _actions.value = cur.copy(busy = true, error = null)
        scope.launch {
            try {
                val a = requireApi()
                if (topic != null) a.setRoomSubject(cur.target.address, topic)
                if (name != null) a.renameRoom(cur.target.address, name)
                _actions.update { now -> now?.copy(busy = false) }
                onDone()
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                logWarn("JoinViewModel", "save channel failed", e)
                _actions.update { now -> now?.copy(busy = false, error = describeError(e)) }
            }
        }
    }

    fun markRead() {
        val target = _actions.value?.target ?: return
        scope.launch {
            try {
                requireApi().markRead(target.jid)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                logWarn("JoinViewModel", "mark read failed", e)
                _events.tryEmit(JoinEvent.Message(describeError(e)))
            }
        }
    }

    /** Leave the room, or remove the contact. The caller asked the user first. */
    fun leave() {
        val cur = _actions.value ?: return
        if (cur.busy) return
        val target = cur.target
        if (target.kind == ActionKind.Occupant) return
        _actions.value = cur.copy(busy = true, error = null)
        scope.launch {
            try {
                val a = requireApi()
                if (target.kind == ActionKind.Room) a.leaveRoom(target.address) else a.removeContact(target.address)
                _actions.value = null
                _events.tryEmit(JoinEvent.Left(target.address))
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                logWarn("JoinViewModel", "leave failed", e)
                _actions.update { now -> now?.copy(busy = false, error = describeError(e)) }
            }
        }
    }

    companion object {
        val factory: ViewModelProvider.Factory = viewModelFactory {
            initializer {
                val app = this[ViewModelProvider.AndroidViewModelFactory.APPLICATION_KEY] as ChordApp
                JoinViewModel({ app.session.conversationApi() })
            }
        }
    }
}

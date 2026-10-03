package space.foid.chord.viewmodel

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import space.foid.chord.ChordApp
import space.foid.chord.data.SpaceApi
import space.foid.chord.data.logWarn
import space.foid.chord.data.spaceApi
import space.foid.chord.ui.spaces.SpaceTarget
import space.foid.chord.ui.spaces.channelSlug
import space.foid.chord.ui.spaces.inviteText
import space.foid.chord.ui.spaces.newRoomJid
import space.foid.chord.ui.spaces.roomServiceFor
import space.foid.chord.ui.spaces.settingsChange
import space.foid.chord.ui.spaces.spaceInviteLink
import space.foid.chord.ui.spaces.spaceLevel
import space.foid.chord.ui.spaces.spaceNick
import uniffi.chord_ffi.ChannelItem
import uniffi.chord_ffi.ChannelKind
import uniffi.chord_ffi.ChannelScope
import uniffi.chord_ffi.ChordException
import uniffi.chord_ffi.Contact
import uniffi.chord_ffi.JoinRequest
import uniffi.chord_ffi.NotificationLevel
import uniffi.chord_ffi.SpaceAccess
import uniffi.chord_ffi.SpaceItem
import uniffi.chord_ffi.SpaceMember

/**
 * What a space menu or dialog shows. It is about one space, [target], and is cleared by [SpaceViewModel.close].
 *
 * @property owner true when the service let us read the members of the space (only the owner can),
 * false when it refused, null until it answered.
 * @property level the level that all channels of the space share, or All.
 */
data class SpaceState(
    val target: SpaceTarget? = null,
    val channels: List<ChannelItem> = emptyList(),
    val channelsLoaded: Boolean = false,
    val owner: Boolean? = null,
    val level: NotificationLevel? = null,
    val busy: Boolean = false,
    val error: String? = null,
    // The settings page.
    val settingsLoaded: Boolean = false,
    val members: List<SpaceMember> = emptyList(),
    val description: String = "",
    val requests: List<JoinRequest> = emptyList(),
    // The invite page.
    val contacts: List<Contact>? = null,
) {
    /** The rooms of the space (no direct chats, no private chats). */
    val rooms: List<ChannelItem> get() = channels.filter { it.kind is ChannelKind.Room }
}

/** Something the main screen reacts to. */
sealed interface SpaceEvent {
    /** We created or joined this space: select it. */
    data class OpenSpace(val service: String, val node: String) : SpaceEvent

    /** Select this room. */
    data class OpenChannel(val jid: String, val name: String) : SpaceEvent

    /** We left or deleted this space. The screen goes Home if it shows it. */
    data class SpaceGone(val service: String, val node: String, val rooms: List<String>) : SpaceEvent

    /** A short text for the snackbar. */
    data class Message(val text: String) : SpaceEvent
}

/**
 * The space menu, the space dialogs, the rail menu and "Add a space". It calls [SpaceApi].
 * Each call that changes something sets [SpaceState.busy], shows a failure in [SpaceState.error],
 * and calls the `onDone` of the caller when it worked.
 */
class SpaceViewModel(
    private val api: () -> SpaceApi?,
    private val scope: CoroutineScope = mainScope(),
) : ViewModel(scope) {
    private val _state = MutableStateFlow(SpaceState())
    val state: StateFlow<SpaceState> = _state.asStateFlow()

    private val _events = MutableSharedFlow<SpaceEvent>(extraBufferCapacity = 16)
    val events: SharedFlow<SpaceEvent> = _events.asSharedFlow()

    private var loading: Job? = null

    private fun requireApi(): SpaceApi = api() ?: throw ChordException.NotConnected()

    /** Run [block] with the busy flag. A failure goes to [SpaceState.error]. */
    private fun work(what: String, onDone: () -> Unit = {}, block: suspend SpaceApi.() -> Unit) {
        if (_state.value.busy) return
        _state.update { it.copy(busy = true, error = null) }
        scope.launch {
            try {
                requireApi().block()
                _state.update { it.copy(busy = false) }
                onDone()
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                logWarn("SpaceViewModel", "$what failed", e)
                _state.update { it.copy(busy = false, error = describeSpaceError(e)) }
            }
        }
    }

    // ---- Open and close ----

    /** Start a menu or dialog for [target]. It reads the channels, the level and whether we own the space. */
    fun open(target: SpaceTarget) {
        if (_state.value.target == target) return
        _state.value = SpaceState(target = target)
        loading?.cancel()
        loading = scope.launch {
            val a = try {
                requireApi()
            } catch (e: ChordException) {
                _state.update { it.copy(error = describeSpaceError(e)) }
                return@launch
            }
            try {
                val channels = a.channels(ChannelScope.Space(target.service, target.node))
                _state.update { cur -> if (cur.target == target) cur.copy(channels = channels, channelsLoaded = true) else cur }
                val levels = channels.filter { it.kind is ChannelKind.Room }.map { a.notificationSetting(it.jid).level }
                _state.update { cur -> if (cur.target == target) cur.copy(level = spaceLevel(levels)) else cur }
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                logWarn("SpaceViewModel", "read channels failed", e)
                _state.update { cur -> if (cur.target == target) cur.copy(channelsLoaded = true, level = cur.level ?: NotificationLevel.ALL) else cur }
            }
            // Only the owner can read the members. So this tells who we are.
            try {
                val members = a.spaceMembers(target.service, target.node)
                _state.update { cur -> if (cur.target == target) cur.copy(owner = true, members = members) else cur }
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                _state.update { cur -> if (cur.target == target) cur.copy(owner = false) else cur }
            }
        }
    }

    fun close() {
        loading?.cancel()
        _state.value = SpaceState()
    }

    fun clearError() = _state.update { it.copy(error = null) }

    // ---- Settings ----

    /** Read what the settings page shows: the description, the members and the join requests. */
    fun loadSettings() {
        val t = _state.value.target ?: return
        scope.launch {
            try {
                val a = requireApi()
                val description = runCatching { a.spaceDescription(t.service, t.node) }.getOrDefault("")
                val members = runCatching { a.spaceMembers(t.service, t.node) }
                val requests = runCatching { a.joinRequests(t.service, t.node) }
                _state.update { cur ->
                    if (cur.target != t) cur else cur.copy(
                        settingsLoaded = true,
                        description = description,
                        members = members.getOrDefault(cur.members),
                        requests = requests.getOrDefault(emptyList()),
                        owner = if (members.isSuccess || requests.isSuccess) true else cur.owner,
                    )
                }
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                _state.update { it.copy(settingsLoaded = true, error = describeSpaceError(e)) }
            }
        }
    }

    /** Save the name and the description. Nothing changed: no call. */
    fun saveSettings(name: String, description: String, onDone: () -> Unit) {
        val s = _state.value
        val t = s.target ?: return
        val change = settingsChange(t.name, s.description, name, description)
        if (change.isEmpty) return onDone()
        work("save space", onDone) { configureSpace(t.service, t.node, change.name, change.description) }
    }

    /** Set the avatar or the banner of the space. The caller read and sized the image. */
    fun setImage(banner: Boolean, mime: String, data: ByteArray, width: Int, height: Int) {
        val t = _state.value.target ?: return
        work("space image", { _events.tryEmit(SpaceEvent.Message(if (banner) "Banner changed." else "Avatar changed.")) }) {
            setSpaceImage(t.service, t.node, banner, mime, data, width, height)
        }
    }

    /** Take the membership of [jid] away, or ban the person. */
    fun removeMember(jid: String, ban: Boolean) {
        val t = _state.value.target ?: return
        work("remove member") {
            if (ban) banSpaceMember(t.service, t.node, jid) else removeSpaceMember(t.service, t.node, jid)
            val members = spaceMembers(t.service, t.node)
            _state.update { cur -> if (cur.target == t) cur.copy(members = members) else cur }
        }
    }

    fun answerRequest(jid: String, approve: Boolean) {
        val t = _state.value.target ?: return
        work("answer request") {
            answerJoinRequest(t.service, t.node, jid, approve)
            _state.update { cur -> if (cur.target == t) cur.copy(requests = cur.requests.filter { it.jid != jid }) else cur }
        }
    }

    /** Take a room out of the space. The room itself stays. */
    fun removeChannel(room: String) {
        val t = _state.value.target ?: return
        work("remove channel") {
            removeRoomFromSpace(t.service, t.node, room)
            _state.update { cur -> if (cur.target == t) cur.copy(channels = cur.channels.filter { it.jid != room }) else cur }
        }
    }

    /** Delete the space for everyone. The caller asked first. */
    fun deleteSpace(onDone: () -> Unit) {
        val s = _state.value
        val t = s.target ?: return
        work("delete space", {
            _events.tryEmit(SpaceEvent.SpaceGone(t.service, t.node, s.rooms.map { it.jid }))
            onDone()
        }) { deleteSpace(t.service, t.node) }
    }

    // ---- Invite ----

    fun loadContacts() {
        if (_state.value.contacts != null) return
        scope.launch {
            try {
                val list = requireApi().contacts()
                _state.update { it.copy(contacts = list) }
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                logWarn("SpaceViewModel", "contacts failed", e)
                _state.update { it.copy(contacts = emptyList(), error = describeSpaceError(e)) }
            }
        }
    }

    /**
     * Make each of [jids] a member, then send a message with the link, as the desktop does. A
     * refusal of the member step does not stop the message: only the owner may add members.
     */
    fun invite(jids: List<String>, onDone: (sent: Int) -> Unit) {
        val t = _state.value.target ?: return
        if (jids.isEmpty() || _state.value.busy) return
        _state.update { it.copy(busy = true, error = null) }
        scope.launch {
            var sent = 0
            try {
                val a = requireApi()
                val text = inviteText(t.name, spaceInviteLink(t.service, t.node))
                for (jid in jids) {
                    try {
                        runCatching { a.addSpaceMember(t.service, t.node, jid) }
                        a.sendMessage(jid, text)
                        sent += 1
                    } catch (e: CancellationException) {
                        throw e
                    } catch (e: Exception) {
                        logWarn("SpaceViewModel", "invite failed", e)
                    }
                }
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                logWarn("SpaceViewModel", "invite failed", e)
            }
            if (sent == jids.size) {
                _state.update { it.copy(busy = false) }
                _events.tryEmit(SpaceEvent.Message(if (sent == 1) "Sent 1 invite." else "Sent $sent invites."))
                onDone(sent)
            } else {
                val failed = jids.size - sent
                _state.update {
                    it.copy(busy = false, error = if (failed == jids.size) "No invite was sent." else "$failed of ${jids.size} invites were not sent.")
                }
            }
        }
    }

    // ---- Channels ----

    /**
     * Make a room in the space, as the desktop does: join it with our nick (this makes it), set
     * its name, add it to the space. Then open it.
     */
    fun createChannel(name: String, onDone: () -> Unit) {
        val s = _state.value
        val t = s.target ?: return
        val slug = channelSlug(name)
        if (slug.isEmpty()) {
            _state.update { it.copy(error = "Use letters or digits in the name.") }
            return
        }
        work("create channel", onDone) {
            val service = roomServiceFor(t.service, s.rooms.map { it.jid })
                ?: throw ChordException.Unsupported("This server has no channel service.")
            val room = newRoomJid(t.node, slug, service)
            joinRoom(room, spaceNick(null, account()))
            // Only an owner can set the name. The room is ready without it.
            val named = runCatching { configureRoom(room, slug) }.isSuccess
            addRoomToSpace(t.service, t.node, room, slug)
            if (!named) _events.tryEmit(SpaceEvent.Message("The channel is ready, but only an owner can set its name."))
            _events.tryEmit(SpaceEvent.OpenChannel(room, slug))
        }
    }

    // ---- Nickname ----

    /** Use [nick] in every room of the space that we joined. */
    fun changeNick(nick: String, onDone: () -> Unit) {
        val s = _state.value
        val clean = nick.trim()
        if (clean.isEmpty() || s.target == null) return
        val rooms = s.rooms.filter { it.joined }
        work("change nick", onDone) {
            var failed = 0
            for (r in rooms) {
                try {
                    changeNick(r.jid.substringBefore('/'), clean)
                } catch (e: CancellationException) {
                    throw e
                } catch (e: Exception) {
                    logWarn("SpaceViewModel", "change nick failed", e)
                    failed += 1
                }
            }
            if (failed > 0 && failed == rooms.size) throw ChordException.Invalid("none")
            if (failed > 0) _events.tryEmit(SpaceEvent.Message("The nickname changed in ${rooms.size - failed} of ${rooms.size} channels."))
        }
    }

    // ---- Notifications and reads ----

    /** Set the level of every room of the space. A timed mute is cleared. */
    fun setLevel(level: NotificationLevel, onDone: () -> Unit) {
        val s = _state.value
        if (s.target == null) return
        work("space level", onDone) {
            for (r in s.rooms) setNotification(r.jid, level, null)
            _state.update { it.copy(level = level) }
        }
    }

    /** Mark every unread room of the space as read. */
    fun markSpaceRead(target: SpaceTarget) = markScopesRead(listOf(ChannelScope.Space(target.service, target.node)))

    /** Mark every unread chat of Home, or of every space and Home, as read. */
    fun markAllRead(spaces: List<SpaceItem>, includeHome: Boolean) {
        val scopes = buildList {
            if (includeHome) add(ChannelScope.Home)
            spaces.forEach { add(ChannelScope.Space(it.service, it.node)) }
        }
        markScopesRead(scopes)
    }

    private fun markScopesRead(scopes: List<ChannelScope>) {
        scope.launch {
            try {
                val a = requireApi()
                scopes.flatMap { a.channels(it) }.filter { it.unread > 0u }.forEach { a.markRead(it.jid) }
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                logWarn("SpaceViewModel", "mark read failed", e)
                _events.tryEmit(SpaceEvent.Message(describeSpaceError(e)))
            }
        }
    }

    // ---- Leave ----

    /** Leave each joined room, then the space. The caller asked first. */
    fun leaveSpace(onDone: () -> Unit) {
        val s = _state.value
        val t = s.target ?: return
        work("leave space", {
            _events.tryEmit(SpaceEvent.SpaceGone(t.service, t.node, s.rooms.map { it.jid }))
            onDone()
        }) {
            var failed = 0
            val rooms = s.rooms.filter { it.joined }
            for (r in rooms) {
                try {
                    leaveRoom(r.jid.substringBefore('/'))
                } catch (e: CancellationException) {
                    throw e
                } catch (e: Exception) {
                    logWarn("SpaceViewModel", "leave room failed", e)
                    failed += 1
                }
            }
            leaveSpace(t.service, t.node)
            if (failed > 0) _events.tryEmit(SpaceEvent.Message("$failed of ${rooms.size} channels could not be left."))
        }
    }

    // ---- Add a space ----

    /** Make a space that we own, then open it. */
    fun createSpace(name: String, description: String, access: SpaceAccess, onDone: () -> Unit) {
        val clean = name.trim()
        if (clean.isEmpty()) return
        work("create space", onDone) {
            val ref = createSpace(clean, description.trim().ifEmpty { null }, access)
            _events.tryEmit(SpaceEvent.OpenSpace(ref.service, ref.node))
        }
    }

    companion object {
        val factory: ViewModelProvider.Factory = viewModelFactory {
            initializer {
                val app = this[ViewModelProvider.AndroidViewModelFactory.APPLICATION_KEY] as ChordApp
                SpaceViewModel({ app.session.spaceApi() })
            }
        }
    }
}

/** Plain text for a failure of a space call. */
fun describeSpaceError(e: Throwable): String = when (serverCondition(e)) {
    "forbidden", "not-authorized" -> "Only the owner of this space can do this."
    "not-allowed" -> "This server does not let you do this."
    "item-not-found" -> "This was not found. It may be gone."
    "conflict" -> "Someone already uses this name."
    "registration-required" -> "Only members can do this."
    else -> if (e is ChordException.Unsupported) "This server has no channel service." else describeError(e)
}

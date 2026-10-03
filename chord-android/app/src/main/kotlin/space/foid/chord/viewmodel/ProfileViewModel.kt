package space.foid.chord.viewmodel

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import space.foid.chord.ChordApp
import space.foid.chord.ui.components.Presence
import uniffi.chord_ffi.ChordClient
import uniffi.chord_ffi.Contact
import uniffi.chord_ffi.MemberItem
import uniffi.chord_ffi.Profile
import uniffi.chord_ffi.SpaceItem
import uniffi.chord_ffi.SpaceMember

/** The calls of the profile screens on the core. A small interface so the ViewModel runs on the JVM. */
interface ProfileApi {
    fun account(): String
    suspend fun profile(jid: String): Profile
    suspend fun contacts(): List<Contact>
    suspend fun blocked(): List<String>
    suspend fun addContact(jid: String, name: String?)
    suspend fun removeContact(jid: String)
    suspend fun renameContact(jid: String, name: String?)
    suspend fun block(jid: String)
    suspend fun unblock(jid: String)
    suspend fun spaceMembers(space: SpaceItem): List<SpaceMember>
    suspend fun inviteToSpace(space: SpaceItem, jid: String)
}

class ClientProfileApi(private val client: ChordClient) : ProfileApi {
    override fun account(): String = client.account()
    override suspend fun profile(jid: String): Profile = client.profile(jid)
    override suspend fun contacts(): List<Contact> = client.contacts()
    override suspend fun blocked(): List<String> = client.blockedContacts()
    override suspend fun addContact(jid: String, name: String?) = client.addContact(jid, name)
    override suspend fun removeContact(jid: String) = client.removeContact(jid)
    override suspend fun renameContact(jid: String, name: String?) = client.renameContact(jid, name)
    override suspend fun block(jid: String) = client.blockContact(jid)
    override suspend fun unblock(jid: String) = client.unblockContact(jid)
    override suspend fun spaceMembers(space: SpaceItem): List<SpaceMember> = client.spaceMembers(space.service, space.node)
    override suspend fun inviteToSpace(space: SpaceItem, jid: String) = client.addSpaceMember(space.service, space.node, jid)
}

/**
 * Who a profile is about. [address] is a bare address, or `room/nick` for an occupant whose real
 * address the room hides. [member] is the room entry, if the person is in the open room.
 */
data class ProfileSubject(val address: String, val name: String, val member: MemberItem? = null)

/** The result of an action, shown as one line. The UI maps it to text. */
enum class ProfileNotice { ContactAdded, ContactRemoved, Renamed, Blocked, Unblocked, Invited, Failed }

data class ProfileState(
    val address: String,
    val name: String,
    val avatarHash: String? = null,
    /** Null when the core knows nothing: then no mark shows. */
    val presence: Presence? = null,
    val status: String? = null,
    val isMe: Boolean = false,
    val isContact: Boolean = false,
    val isBlocked: Boolean = false,
    /** The room hides the real address: there is nothing to add or block. */
    val hidden: Boolean = false,
    val affiliation: String? = null,
    val role: String? = null,
    val fullName: String? = null,
    val nickname: String? = null,
    val sharedSpaces: List<SpaceItem> = emptyList(),
    /** Spaces the person is not (known to be) in. */
    val invitable: List<SpaceItem> = emptyList(),
    val busy: Boolean = false,
)

/** Presence from the `online` and `show` values of the core. */
fun presenceFor(online: Boolean, show: String?): Presence = when {
    !online -> Presence.Offline
    show == "dnd" -> Presence.Dnd
    show == "away" || show == "xa" -> Presence.Away
    else -> Presence.Online
}

internal fun bareAddress(jid: String) = jid.substringBefore('/')

/** The profile of one person. It holds no protocol logic: it asks the core and shows the answer. */
class ProfileViewModel(
    subject: ProfileSubject,
    private val api: ProfileApi?,
    private val scope: CoroutineScope = mainScope(),
) : ViewModel(scope) {
    private val member = subject.member
    private val address = member?.jid ?: subject.address
    private val _state = MutableStateFlow(
        ProfileState(
            address = address,
            name = member?.name?.ifBlank { null } ?: subject.name.ifBlank { address.substringBefore('@') },
            avatarHash = member?.avatar,
            presence = member?.let { presenceFor(it.online, it.show) },
            status = member?.status?.takeIf { it.isNotBlank() },
            hidden = '/' in address,
            affiliation = member?.affiliation?.takeIf { it.isNotBlank() && it != "none" },
            role = member?.role?.takeIf { it.isNotBlank() && it != "none" && it != "participant" },
        ),
    )
    val state: StateFlow<ProfileState> = _state.asStateFlow()

    private val _notice = MutableStateFlow<ProfileNotice?>(null)
    val notice: StateFlow<ProfileNotice?> = _notice.asStateFlow()

    private var spaces: List<SpaceItem> = emptyList()

    init {
        scope.launch { refresh() }
    }

    private suspend fun refresh() {
        val a = api ?: return
        val hidden = '/' in address
        val me = !hidden && runCatching { bareAddress(a.account()) == address }.getOrDefault(false)
        _state.update { it.copy(isMe = me) }
        if (hidden) return
        runCatching { a.contacts() }.getOrNull()?.let { list ->
            val c = list.firstOrNull { it.jid == address }
            _state.update { s ->
                s.copy(
                    isContact = c != null,
                    name = c?.name?.takeIf { n -> n.isNotBlank() } ?: s.name,
                    presence = s.presence ?: c?.let { x -> presenceFor(x.online, x.show) },
                    status = s.status ?: c?.status?.takeIf { t -> t.isNotBlank() },
                )
            }
        }
        runCatching { a.blocked() }.getOrNull()?.let { list ->
            _state.update { it.copy(isBlocked = address in list) }
        }
        if (!me) {
            runCatching { a.profile(address) }.getOrNull()?.let { p ->
                _state.update {
                    it.copy(
                        fullName = p.fullName?.takeIf { n -> n.isNotBlank() },
                        nickname = p.nickname?.takeIf { n -> n.isNotBlank() },
                    )
                }
            }
        }
    }

    /** Give the spaces of the account. The ViewModel works out which ones the person is in. */
    fun setSpaces(list: List<SpaceItem>) {
        if (list == spaces) return
        spaces = list
        val a = api ?: return
        if (_state.value.hidden) return
        scope.launch {
            val shared = ArrayList<SpaceItem>()
            val other = ArrayList<SpaceItem>()
            for (sp in list) {
                val members = runCatching { a.spaceMembers(sp) }.getOrNull()
                if (members != null && members.any { bareAddress(it.jid) == address }) shared += sp else other += sp
            }
            _state.update { it.copy(sharedSpaces = shared, invitable = other) }
        }
    }

    fun addContact() = act(ProfileNotice.ContactAdded) { it.addContact(address, null) }
    fun removeContact() = act(ProfileNotice.ContactRemoved) { it.removeContact(address) }
    fun rename(name: String) = act(ProfileNotice.Renamed) { it.renameContact(address, name.trim().ifEmpty { null }) }
    fun block() = act(ProfileNotice.Blocked) { it.block(address) }
    fun unblock() = act(ProfileNotice.Unblocked) { it.unblock(address) }
    fun invite(space: SpaceItem) = act(ProfileNotice.Invited, refreshAfter = false) { it.inviteToSpace(space, address) }

    fun clearNotice() {
        _notice.value = null
    }

    private fun act(ok: ProfileNotice, refreshAfter: Boolean = true, call: suspend (ProfileApi) -> Unit) {
        val a = api ?: return
        if (_state.value.busy) return
        _state.update { it.copy(busy = true) }
        scope.launch {
            _notice.value = try {
                call(a)
                if (refreshAfter) refresh()
                ok
            } catch (e: Exception) {
                ProfileNotice.Failed
            }
            _state.update { it.copy(busy = false) }
        }
    }

    companion object {
        fun factory(subject: ProfileSubject): ViewModelProvider.Factory = viewModelFactory {
            initializer {
                val app = this[ViewModelProvider.AndroidViewModelFactory.APPLICATION_KEY] as ChordApp
                ProfileViewModel(subject, app.session.client.value?.let(::ClientProfileApi))
            }
        }
    }
}

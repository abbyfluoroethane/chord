package space.foid.chord.viewmodel

import androidx.lifecycle.ViewModel
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import space.foid.chord.data.logWarn
import space.foid.chord.ui.settings.SettingsStore
import space.foid.chord.ui.settings.ThemeMode
import uniffi.chord_ffi.Availability
import uniffi.chord_ffi.ChordClient
import uniffi.chord_ffi.InvisibleMethod
import uniffi.chord_ffi.OwnPresence

/** The calls of the settings screen on the core. [ClientSettingsApi] is the real one. */
interface SettingsApi {
    /** The bare address of the account. */
    fun account(): String
    suspend fun nickname(): String?
    suspend fun setNickname(nickname: String?)

    /** The bytes of our avatar, or null when there is none yet. */
    suspend fun avatar(): ByteArray?
    suspend fun setAvatar(mime: String, data: ByteArray, width: Int, height: Int)
    suspend fun removeAvatar()
    suspend fun ownPresence(): OwnPresence

    /** How the server can hide us, or null. */
    suspend fun invisibleMethod(): InvisibleMethod?
    suspend fun setPresence(availability: Availability, status: String?)
    suspend fun blockedContacts(): List<String>
    suspend fun unblockContact(jid: String)
    suspend fun setShareInfo(share: Boolean)
}

/** [SettingsApi] on a [ChordClient]. */
class ClientSettingsApi(private val client: ChordClient) : SettingsApi {
    override fun account(): String = client.account()
    override suspend fun nickname(): String? = client.profile(client.account()).nickname
    override suspend fun setNickname(nickname: String?) = client.setNickname(nickname)
    override suspend fun avatar(): ByteArray? = client.avatar(client.account())?.data
    override suspend fun setAvatar(mime: String, data: ByteArray, width: Int, height: Int) =
        client.setAvatar(mime, data, width.toUShort(), height.toUShort())
    override suspend fun removeAvatar() = client.removeAvatar()
    override suspend fun ownPresence(): OwnPresence = client.ownPresence()
    override suspend fun invisibleMethod(): InvisibleMethod? = client.invisibleMethod()
    override suspend fun setPresence(availability: Availability, status: String?) =
        client.setPresence(availability, status)
    override suspend fun blockedContacts(): List<String> = client.blockedContacts()
    override suspend fun unblockContact(jid: String) = client.unblockContact(jid)
    override suspend fun setShareInfo(share: Boolean) = client.setShareInfo(share)
}

/** What the settings screen shows. */
data class SettingsState(
    val jid: String = "",
    /** The nickname on the server. */
    val nickname: String = "",
    /** The text in the nickname field. */
    val nicknameDraft: String = "",
    /** The bytes of our avatar, or null. Compared by reference: a new upload is a new array. */
    val avatar: ByteArray? = null,
    val availability: Availability = Availability.AVAILABLE,
    val status: String = "",
    val statusDraft: String = "",
    /** The server can hide us, or we are hidden now: "Invisible" is on offer. */
    val canHide: Boolean = false,
    val blocked: List<String> = emptyList(),
    val shareInfo: Boolean = true,
    val theme: ThemeMode = ThemeMode.System,
    /** Plain-English text of the last failure, or null. */
    val error: String? = null,
    val loaded: Boolean = false,
) {
    val nicknameChanged: Boolean get() = nicknameDraft.trim() != nickname
    val statusChanged: Boolean get() = statusDraft.trim() != status

    /** The availabilities on offer, in the order of the desktop status menu. */
    val availabilities: List<Availability>
        get() = buildList {
            add(Availability.AVAILABLE)
            add(Availability.AWAY)
            add(Availability.DND)
            if (canHide || availability == Availability.INVISIBLE) add(Availability.INVISIBLE)
        }
}

/**
 * The settings screen. It loads the account data once, then changes the account through [api]
 * and the phone settings through [store]. It holds no protocol logic.
 */
class SettingsViewModel(
    private val api: SettingsApi,
    private val store: SettingsStore,
    private val signOutAction: suspend () -> Unit,
    private val scope: CoroutineScope = mainScope(),
) : ViewModel(scope) {
    private val _state = MutableStateFlow(
        SettingsState(jid = api.account(), shareInfo = store.shareInfo.value, theme = store.themeMode.value),
    )
    val state: StateFlow<SettingsState> = _state.asStateFlow()

    /** True after a sign-out. The screen then leaves. */
    private val _signedOut = MutableStateFlow(false)
    val signedOut: StateFlow<Boolean> = _signedOut.asStateFlow()

    init {
        scope.launch { store.themeMode.collect { t -> _state.update { it.copy(theme = t) } } }
        scope.launch { store.shareInfo.collect { s -> _state.update { it.copy(shareInfo = s) } } }
        scope.launch { load() }
    }

    private suspend fun load() {
        // Each read can fail on its own (offline, no server support). The others still show.
        val nick = attempt { api.nickname() }.orEmpty()
        val avatar = attempt { api.avatar() }
        val presence = attempt { api.ownPresence() }
        val hide = attempt { api.invisibleMethod() }
        val blocked = attempt { api.blockedContacts() }.orEmpty()
        _state.update {
            it.copy(
                nickname = nick, nicknameDraft = nick, avatar = avatar,
                availability = presence?.availability ?: it.availability,
                status = presence?.status.orEmpty(), statusDraft = presence?.status.orEmpty(),
                canHide = hide != null, blocked = blocked, loaded = true,
            )
        }
    }

    private suspend fun <T> attempt(block: suspend () -> T): T? = try {
        block()
    } catch (e: CancellationException) {
        throw e
    } catch (e: Exception) {
        logWarn("SettingsViewModel", "read failed", e)
        null
    }

    private fun run(label: String, block: suspend () -> Unit) {
        scope.launch {
            try {
                block()
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                logWarn("SettingsViewModel", "$label failed", e)
                _state.update { it.copy(error = describeError(e)) }
            }
        }
    }

    fun dismissError() = _state.update { it.copy(error = null) }

    fun onNicknameChange(value: String) = _state.update { it.copy(nicknameDraft = value, error = null) }

    /** Publish the nickname. An empty text removes it. */
    fun saveNickname() {
        val s = _state.value
        if (!s.nicknameChanged) return
        val nick = s.nicknameDraft.trim()
        run("setNickname") {
            api.setNickname(nick.ifEmpty { null })
            _state.update { it.copy(nickname = nick, nicknameDraft = nick) }
        }
    }

    /** Set our avatar. [data] is an image that the screen already scaled. */
    fun setAvatar(mime: String, data: ByteArray, width: Int, height: Int) = run("setAvatar") {
        api.setAvatar(mime, data, width, height)
        _state.update { it.copy(avatar = data) }
    }

    fun removeAvatar() = run("removeAvatar") {
        api.removeAvatar()
        _state.update { it.copy(avatar = null) }
    }

    /** The picked image could not be read. */
    fun avatarFailed() = _state.update { it.copy(error = "Chord could not read that picture.") }

    fun onStatusChange(value: String) = _state.update { it.copy(statusDraft = value, error = null) }

    fun setAvailability(availability: Availability) {
        val status = _state.value.status.ifEmpty { null }
        run("setPresence") {
            api.setPresence(availability, status)
            _state.update { it.copy(availability = availability) }
        }
    }

    /** Publish the status text with the current availability. */
    fun saveStatus() {
        val s = _state.value
        if (!s.statusChanged) return
        val text = s.statusDraft.trim()
        run("setStatus") {
            api.setPresence(s.availability, text.ifEmpty { null })
            _state.update { it.copy(status = text, statusDraft = text) }
        }
    }

    fun unblock(jid: String) = run("unblock") {
        api.unblockContact(jid)
        _state.update { it.copy(blocked = it.blocked - jid) }
    }

    fun setShareInfo(share: Boolean) {
        val before = store.shareInfo.value
        store.setShareInfo(share)
        scope.launch {
            try {
                api.setShareInfo(share)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                logWarn("SettingsViewModel", "setShareInfo failed", e)
                store.setShareInfo(before)
                _state.update { it.copy(error = describeError(e)) }
            }
        }
    }

    fun setTheme(mode: ThemeMode) = store.setThemeMode(mode)

    fun signOut() {
        scope.launch {
            try {
                signOutAction()
                _signedOut.value = true
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                logWarn("SettingsViewModel", "sign-out failed", e)
                _state.update { it.copy(error = describeError(e)) }
            }
        }
    }
}

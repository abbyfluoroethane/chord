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
import space.foid.chord.ui.settings.AppPrefs
import space.foid.chord.ui.settings.MAX_STATUS
import space.foid.chord.ui.settings.SettingsStore
import space.foid.chord.ui.settings.SignInShow
import space.foid.chord.ui.settings.ThemeMode
import uniffi.chord_ffi.Availability
import uniffi.chord_ffi.ChordClient
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

    suspend fun blockedContacts(): List<String>
    suspend fun unblockContact(jid: String)
    suspend fun setShareInfo(share: Boolean)
    suspend fun unblockAll()

    /** Change the password of the account on the server. */
    suspend fun changePassword(password: String)
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
    override suspend fun blockedContacts(): List<String> = client.blockedContacts()
    override suspend fun unblockContact(jid: String) = client.unblockContact(jid)
    override suspend fun setShareInfo(share: Boolean) = client.setShareInfo(share)
    override suspend fun unblockAll() = client.unblockAll()
    override suspend fun changePassword(password: String) = client.changePassword(password)
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
    /** The status text on the server. Read only here. */
    val status: String = "",
    val blocked: List<String> = emptyList(),
    /** The settings of the phone. */
    val prefs: AppPrefs = AppPrefs(),
    /** The text in the sign-in status field. */
    val signInStatusDraft: String = "",
    val password: PasswordState = PasswordState(),
    /** Plain-English text of the last failure, or null. */
    val error: String? = null,
    val loaded: Boolean = false,
) {
    val nicknameChanged: Boolean get() = nicknameDraft.trim() != nickname
    val signInStatusChanged: Boolean get() = signInStatusDraft.trim() != prefs.signInStatus

    /** The name to show: the nickname, or the local part of the address. */
    val displayName: String get() = nickname.ifEmpty { jid.substringBefore('@') }
}

/** The state of the change-password page. */
data class PasswordState(
    val busy: Boolean = false,
    val error: PasswordError? = null,
    /** The text of a refusal by the server, for [PasswordError.Server]. */
    val message: String? = null,
    val done: Boolean = false,
)

enum class PasswordError { Empty, Mismatch, SaveFailed, Server }

/**
 * The settings screen. It loads the account data once, then changes the account through [api]
 * and the phone settings through [store]. It holds no protocol logic.
 *
 * [savePassword] stores a new password in the credential store of the app after the server took it.
 */
class SettingsViewModel(
    private val api: SettingsApi,
    private val store: SettingsStore,
    private val signOutAction: suspend () -> Unit,
    private val savePassword: suspend (String) -> Unit = {},
    private val scope: CoroutineScope = mainScope(),
) : ViewModel(scope) {
    private val _state = MutableStateFlow(
        SettingsState(
            jid = api.account(), prefs = store.prefs.value,
            signInStatusDraft = store.prefs.value.signInStatus,
        ),
    )
    val state: StateFlow<SettingsState> = _state.asStateFlow()

    /** True after a sign-out. The screen then leaves. */
    private val _signedOut = MutableStateFlow(false)
    val signedOut: StateFlow<Boolean> = _signedOut.asStateFlow()

    init {
        scope.launch { store.prefs.collect { p -> _state.update { it.copy(prefs = p) } } }
        scope.launch { load() }
    }

    private suspend fun load() {
        // Each read can fail on its own (offline, no server support). The others still show.
        val nick = attempt { api.nickname() }.orEmpty()
        val avatar = attempt { api.avatar() }
        val presence = attempt { api.ownPresence() }
        val blocked = attempt { api.blockedContacts() }.orEmpty()
        _state.update {
            it.copy(
                nickname = nick, nicknameDraft = nick, avatar = avatar,
                availability = presence?.availability ?: it.availability,
                status = presence?.status.orEmpty(),
                blocked = blocked, loaded = true,
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

    /** Put the nickname field back to the saved name. */
    fun resetNickname() = _state.update { it.copy(nicknameDraft = it.nickname) }

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

    /** The availability to set at each sign-in. It is a phone setting, not a live status. */
    fun setSignInShow(show: SignInShow) = store.update { it.copy(signInShow = show) }

    fun onSignInStatusChange(value: String) =
        _state.update { it.copy(signInStatusDraft = value.replace('\n', ' ').take(MAX_STATUS)) }

    fun saveSignInStatus() {
        val text = _state.value.signInStatusDraft.trim()
        store.update { it.copy(signInStatus = text) }
        _state.update { it.copy(signInStatusDraft = text) }
    }

    fun unblock(jid: String) = run("unblock") {
        api.unblockContact(jid)
        _state.update { it.copy(blocked = it.blocked - jid) }
    }

    fun unblockAll() = run("unblockAll") {
        api.unblockAll()
        _state.update { it.copy(blocked = emptyList()) }
    }

    fun setShareInfo(share: Boolean) {
        val before = store.prefs.value.shareInfo
        store.update { it.copy(shareInfo = share) }
        scope.launch {
            try {
                api.setShareInfo(share)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                logWarn("SettingsViewModel", "setShareInfo failed", e)
                store.update { it.copy(shareInfo = before) }
                _state.update { it.copy(error = describeError(e)) }
            }
        }
    }

    fun setTheme(mode: ThemeMode) = store.update { it.copy(theme = mode) }
    fun setShowPresence(show: Boolean) = store.update { it.copy(showPresence = show) }
    fun setShareIdle(share: Boolean) = store.update { it.copy(shareIdle = share) }
    fun setIdleMinutes(minutes: Int) = store.update { it.copy(idleMinutes = minutes) }
    fun setNoticePreview(show: Boolean) = store.update { it.copy(noticePreview = show) }
    fun setQuietHours(on: Boolean) = store.update { it.copy(quietHours = on) }
    fun setQuietFrom(minute: Int) = store.update { it.copy(quietFrom = minute) }
    fun setQuietTo(minute: Int) = store.update { it.copy(quietTo = minute) }

    /** Put every phone setting back to its default. The share-info answer follows on the account. */
    fun resetSettings() {
        store.reset()
        _state.update { it.copy(signInStatusDraft = "") }
        scope.launch {
            try {
                api.setShareInfo(AppPrefs().shareInfo)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                logWarn("SettingsViewModel", "setShareInfo failed", e)
            }
        }
    }

    /** Leave the change-password page: forget the result. */
    fun clearPassword() = _state.update { it.copy(password = PasswordState()) }

    /** Change the password. Both fields must hold the same, non-empty text. */
    fun changePassword(new: String, repeat: String) {
        if (_state.value.password.busy) return
        val problem = when {
            new.isEmpty() -> PasswordError.Empty
            new != repeat -> PasswordError.Mismatch
            else -> null
        }
        if (problem != null) {
            _state.update { it.copy(password = PasswordState(error = problem)) }
            return
        }
        _state.update { it.copy(password = PasswordState(busy = true)) }
        scope.launch {
            try {
                api.changePassword(new)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                logWarn("SettingsViewModel", "changePassword failed", e)
                _state.update {
                    it.copy(password = PasswordState(error = PasswordError.Server, message = describeError(e)))
                }
                return@launch
            }
            // The server has the new password. The phone must keep it for the next sign-in.
            val saved = try {
                savePassword(new)
                true
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                logWarn("SettingsViewModel", "saving the new password failed", e)
                false
            }
            _state.update {
                it.copy(password = if (saved) PasswordState(done = true) else PasswordState(error = PasswordError.SaveFailed))
            }
        }
    }

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

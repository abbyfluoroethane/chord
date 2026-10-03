package space.foid.chord.viewmodel

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import space.foid.chord.ChordApp
import space.foid.chord.data.ChordSession
import space.foid.chord.data.logWarn
import space.foid.chord.ui.forms.credentialsOf
import space.foid.chord.ui.forms.problems
import space.foid.chord.ui.forms.submission
import space.foid.chord.ui.register.SIGN_IN_AFTER_REGISTER_FAILED
import space.foid.chord.ui.register.domainOf
import space.foid.chord.ui.register.legacyAnswer
import space.foid.chord.ui.register.legacyMissing
import space.foid.chord.ui.register.registerErrorText
import uniffi.chord_ffi.ChordClient
import uniffi.chord_ffi.DataForm
import uniffi.chord_ffi.RegistrationAnswer
import uniffi.chord_ffi.RegistrationInfo
import java.io.File
import java.util.UUID

/** The two calls of the core that a registration needs. They work before any login. */
interface RegistrationApi {
    suspend fun form(domain: String, server: String): RegistrationInfo
    suspend fun register(domain: String, server: String, answer: RegistrationAnswer)
}

/**
 * [RegistrationApi] on the core. The core has the calls on `ChordClient`, so each call opens a
 * client with a store in [cacheDir] that it deletes afterwards. Nothing signs in.
 */
class ClientRegistrationApi(private val cacheDir: File) : RegistrationApi {
    override suspend fun form(domain: String, server: String): RegistrationInfo =
        withClient(domain) { it.registrationForm(domain, server) }

    override suspend fun register(domain: String, server: String, answer: RegistrationAnswer) =
        withClient(domain) { it.registerAccount(domain, server, answer) }

    private suspend fun <T> withClient(domain: String, block: suspend (ChordClient) -> T): T {
        val base = File(cacheDir, "register-" + UUID.randomUUID())
        val client = withContext(Dispatchers.IO) {
            cacheDir.mkdirs()
            ChordClient(base.absolutePath, "registration@$domain")
        }
        try {
            return block(client)
        } finally {
            withContext(Dispatchers.IO + kotlinx.coroutines.NonCancellable) {
                runCatching { client.close() }
                cacheDir.listFiles { f -> f.name.startsWith(base.name) }?.forEach { it.delete() }
            }
        }
    }
}

enum class RegisterStep { SERVER, FORM }

/** The state of the registration screen. */
data class RegisterState(
    val step: RegisterStep = RegisterStep.SERVER,
    val domain: String = "",
    /** What the server asked for, after the server step. */
    val info: RegistrationInfo? = null,
    /** The data form with the answers, when [info] has one. */
    val form: DataForm? = null,
    /** The values of the legacy fields, by name. */
    val legacy: Map<String, String> = emptyMap(),
    val busy: Boolean = false,
    /** True while the app signs in with the new account. */
    val signingIn: Boolean = false,
    val error: String? = null,
    /** Show what is wrong under each field. On after a failed send. */
    val showProblems: Boolean = false,
    val signedIn: Boolean = false,
) {
    val canContinue: Boolean get() = !busy && domainOf(domain).isNotEmpty()

    /** The server gives a link and no fields. */
    val linkOnly: Boolean get() = info != null && info.form == null && info.fields.isEmpty()
}

/**
 * The registration of an account (XEP-0077): the server step, the form step, and the sign-in
 * with the new account. [server] is the "Server" field of the sign-in form.
 */
class RegisterViewModel(
    private val api: RegistrationApi,
    private val session: ChordSession,
    address: String,
    private val server: String,
    private val scope: CoroutineScope = mainScope(),
) : ViewModel(scope) {
    private val _state = MutableStateFlow(RegisterState(domain = domainOf(address)))
    val state: StateFlow<RegisterState> = _state.asStateFlow()

    fun onDomainChange(value: String) = _state.update { it.copy(domain = value, error = null) }

    fun onFormChange(form: DataForm) = _state.update { it.copy(form = form, error = null) }

    fun onLegacyChange(name: String, value: String) =
        _state.update { it.copy(legacy = it.legacy + (name to value), error = null) }

    /** Ask the server for its registration form. */
    fun fetchForm() {
        val s = _state.value
        val domain = domainOf(s.domain)
        if (s.busy || domain.isEmpty()) return
        _state.update { it.copy(busy = true, error = null) }
        scope.launch {
            try {
                val info = api.form(domain, server.trim())
                _state.update {
                    it.copy(
                        busy = false, step = RegisterStep.FORM, domain = domain, info = info,
                        form = info.form, legacy = emptyMap(), showProblems = false,
                    )
                }
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                logWarn("RegisterViewModel", "registration form failed", e)
                _state.update { it.copy(busy = false, error = registerErrorText(e)) }
            }
        }
    }

    /** Send the answers, then sign in with the new account. */
    fun submit() {
        val s = _state.value
        val info = s.info ?: return
        if (s.busy || s.signedIn) return
        var username: String
        var password: String
        val answer: RegistrationAnswer
        val form = s.form
        if (form != null) {
            val found = problems(form)
            if (found.isNotEmpty()) {
                _state.update { it.copy(showProblems = true, error = found.first()) }
                return
            }
            credentialsOf(form).let { username = it.first; password = it.second }
            answer = RegistrationAnswer(form = submission(form), fields = emptyList())
        } else {
            val missing = legacyMissing(info.fields, s.legacy)
            if (missing.isNotEmpty()) {
                _state.update { it.copy(error = "Fill in: ${missing.joinToString(", ")}.") }
                return
            }
            username = (s.legacy["username"] ?: "").trim()
            password = s.legacy["password"] ?: ""
            answer = RegistrationAnswer(form = null, fields = legacyAnswer(info.fields, s.legacy))
        }
        _state.update { it.copy(busy = true, error = null) }
        scope.launch {
            try {
                api.register(s.domain, server.trim(), answer)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                logWarn("RegisterViewModel", "registration failed", e)
                _state.update { it.copy(busy = false, error = registerErrorText(e)) }
                return@launch
            }
            if (username.isEmpty() || password.isEmpty()) {
                // The server made the account but the form had no username or password to sign in with.
                _state.update { it.copy(busy = false, error = SIGN_IN_AFTER_REGISTER_FAILED) }
                return@launch
            }
            _state.update { it.copy(signingIn = true) }
            try {
                session.signIn("$username@${s.domain}", password, server.trim())
                _state.update { it.copy(busy = false, signingIn = false, signedIn = true) }
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                logWarn("RegisterViewModel", "sign-in after registration failed", e)
                _state.update { it.copy(busy = false, signingIn = false, error = SIGN_IN_AFTER_REGISTER_FAILED) }
            }
        }
    }

    /** Back from the form step to the server step. False on the server step: the caller leaves. */
    fun back(): Boolean {
        val s = _state.value
        if (s.busy) return true
        if (s.step == RegisterStep.FORM) {
            _state.update { it.copy(step = RegisterStep.SERVER, info = null, form = null, error = null, showProblems = false) }
            return true
        }
        return false
    }

    companion object {
        fun factory(address: String, server: String): ViewModelProvider.Factory = viewModelFactory {
            initializer {
                val app = this[ViewModelProvider.AndroidViewModelFactory.APPLICATION_KEY] as ChordApp
                RegisterViewModel(ClientRegistrationApi(File(app.cacheDir, "register")), app.session, address, server)
            }
        }
    }
}

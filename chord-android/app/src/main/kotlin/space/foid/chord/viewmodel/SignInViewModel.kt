package space.foid.chord.viewmodel

import androidx.lifecycle.ViewModel
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import space.foid.chord.data.ChordSession
import space.foid.chord.data.logWarn
import space.foid.chord.ui.register.hostOf
import uniffi.chord_ffi.ChordException

/** The state of the sign-in form. */
data class SignInState(
    val jid: String = "",
    val password: String = "",
    /** "" for SRV lookup, "host", or "starttls://host:port". The "advanced" field. */
    val server: String = "",
    val submitting: Boolean = false,
    /** Plain-English text of the last failure, or null. Edits of the form clear it. */
    val error: String? = null,
    /** True after a successful sign-in. */
    val signedIn: Boolean = false,
) {
    val canSubmit: Boolean get() = !submitting && jid.isNotBlank() && password.isNotEmpty()

    /** The server to name in the status line: the "Server" field without scheme and port, or the part after "@". */
    val host: String get() = hostOf(server, jid)
}

private val ADDRESS = Regex("""^[^@\s]+@[^@\s]+\.[^@\s]+$""")

/**
 * The text of a failed sign-in, in the words of the desktop app. [host] is the server that
 * the person tried to reach, or "" when unknown.
 */
fun signInErrorText(error: Throwable, host: String): String {
    val h = host.ifEmpty { "the server" }
    return when (error) {
        is ChordException.AuthFailed -> authFailedText(error.detail)
        is ChordException.Unreachable -> "Can't reach $h. Check the address and your connection."
        is ChordException.TlsInvalid -> "The certificate of $h is not valid, so Chord did not connect."
        is ChordException.Timeout -> "$h did not answer in time. Try again."
        is ChordException.NotConnected -> "You are offline. Try again when the connection is back."
        is ChordException.Unsupported -> "Your server does not support this."
        is ChordException.InvalidJid -> "Enter your address like you@example.com."
        is ChordException.InvalidServer -> "The server setting is not valid. Leave it empty, or use host or starttls://host:port."
        is ChordException.Store -> "Chord could not read its data."
        else -> describeError(error)
    }
}

/** The core words an auth failure with its `Display` text (chord-core `AuthFailure`). */
private fun authFailedText(detail: String): String = when {
    detail == "wrong username or password" -> "Wrong address or password."
    detail == "account disabled" -> "This account is disabled. Ask the people who run your server."
    detail == "password expired" -> "Your password has expired. Set a new one on your server, then sign in again."
    detail == "no common SASL mechanism" -> "The server offers no sign-in method that Chord can use."
    detail.startsWith("server rejected the login: ") -> {
        val words = detail.removePrefix("server rejected the login: ")
            .replace(Regex("([a-z])([A-Z])"), "$1 $2").lowercase()
        "The server refused the sign-in ($words)."
    }
    else -> "Sign-in failed. $detail"
}

/** The form to sign in. It calls [ChordSession.signIn] and shows what went wrong. */
class SignInViewModel(
    private val session: ChordSession,
    private val scope: CoroutineScope = mainScope(),
) : ViewModel(scope) {
    private val _state = MutableStateFlow(SignInState())
    val state: StateFlow<SignInState> = _state.asStateFlow()

    fun onJidChange(value: String) = _state.update { it.copy(jid = value, error = null) }
    fun onPasswordChange(value: String) = _state.update { it.copy(password = value, error = null) }
    fun onServerChange(value: String) = _state.update { it.copy(server = value, error = null) }

    /** Sign in with the form. Does nothing while a sign-in runs. */
    fun submit() {
        val s = _state.value
        if (s.submitting || s.signedIn) return
        val jid = s.jid.trim()
        when {
            !ADDRESS.matches(jid) -> {
                _state.update { it.copy(error = "Enter your address like you@example.com.") }
                return
            }
            s.password.isEmpty() -> {
                _state.update { it.copy(error = "Enter your password.") }
                return
            }
        }
        _state.update { it.copy(submitting = true, error = null) }
        scope.launch {
            try {
                session.signIn(jid, s.password, s.server.trim())
                _state.update { it.copy(submitting = false, signedIn = true, password = "") }
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                logWarn("SignInViewModel", "sign-in failed", e)
                _state.update { it.copy(submitting = false, error = signInErrorText(e, s.copy(jid = jid).host)) }
            }
        }
    }
}

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
            jid.isEmpty() || s.password.isEmpty() -> {
                _state.update { it.copy(error = "Enter your address and your password.") }
                return
            }
            !jid.contains('@') -> {
                _state.update { it.copy(error = "Your address must look like name@server.example.") }
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
                _state.update { it.copy(submitting = false, error = describeError(e)) }
            }
        }
    }
}

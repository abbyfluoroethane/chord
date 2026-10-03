package space.foid.chord.ui.status

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import space.foid.chord.ChordApp
import space.foid.chord.data.logWarn
import space.foid.chord.viewmodel.mainScope
import uniffi.chord_ffi.Availability
import uniffi.chord_ffi.ChordClient
import uniffi.chord_ffi.InvisibleMethod
import uniffi.chord_ffi.OwnPresence

/** The calls of the status sheet on the core. [ClientStatusApi] is the real one. */
interface StatusApi {
    suspend fun ownPresence(): OwnPresence
    suspend fun invisibleMethod(): InvisibleMethod?
    suspend fun setPresence(availability: Availability, status: String?)
}

class ClientStatusApi(private val client: ChordClient) : StatusApi {
    override suspend fun ownPresence(): OwnPresence = client.ownPresence()
    override suspend fun invisibleMethod(): InvisibleMethod? = client.invisibleMethod()
    override suspend fun setPresence(availability: Availability, status: String?) =
        client.setPresence(availability, status)
}

/** What the user panel and the status sheet show. */
data class StatusState(
    val availability: Availability = Availability.AVAILABLE,
    /** The saved status, with its emoji, or null. */
    val status: String? = null,
    /** The server can hide us. */
    val canHide: Boolean = false,
)

/**
 * Our own availability and status text. The core has no event for them, so the state is read
 * on start, on [refresh] (the drawer calls it on login and reconnect) and after each change.
 */
class StatusViewModel(
    private val api: StatusApi,
    private val scope: CoroutineScope = mainScope(),
) : ViewModel(scope) {
    private val _state = MutableStateFlow(StatusState())
    val state: StateFlow<StatusState> = _state.asStateFlow()

    init { refresh() }

    fun refresh() {
        scope.launch {
            val own = attempt { api.ownPresence() }
            // Fails offline: then the last answer stays.
            val hide = attempt { api.invisibleMethod() }
            _state.update {
                it.copy(
                    availability = own?.availability ?: it.availability,
                    status = if (own != null) own.status?.takeIf { s -> s.isNotBlank() } else it.status,
                    canHide = hide != null || it.canHide,
                )
            }
        }
    }

    /** Apply an availability at once. The status text stays. */
    fun setAvailability(a: Availability) {
        _state.update { it.copy(availability = a) }
        push(a, _state.value.status)
    }

    /** Save a status text, or clear it with null. The availability stays. */
    fun setStatus(status: String?) {
        val clean = status?.trim()?.let { clipStatus(it) }?.ifEmpty { null }
        _state.update { it.copy(status = clean) }
        push(_state.value.availability, clean)
    }

    private fun push(a: Availability, status: String?) {
        scope.launch {
            attempt { api.setPresence(a, status) }
            refresh()
        }
    }

    private suspend fun <T> attempt(block: suspend () -> T): T? = try {
        block()
    } catch (e: CancellationException) {
        throw e
    } catch (e: Exception) {
        logWarn("StatusViewModel", "call failed", e)
        null
    }

    companion object {
        val factory: ViewModelProvider.Factory = viewModelFactory {
            initializer {
                val app = this[ViewModelProvider.AndroidViewModelFactory.APPLICATION_KEY] as ChordApp
                StatusViewModel(ClientStatusApi(requireNotNull(app.session.client.value) { "not signed in" }))
            }
        }
    }
}

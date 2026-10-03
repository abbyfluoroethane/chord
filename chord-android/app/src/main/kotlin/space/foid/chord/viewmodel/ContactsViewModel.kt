package space.foid.chord.viewmodel

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import space.foid.chord.ChordApp
import space.foid.chord.data.logWarn
import space.foid.chord.ui.join.personAddressOf
import uniffi.chord_ffi.ChordClient
import uniffi.chord_ffi.ChordException
import uniffi.chord_ffi.ClientEvent
import uniffi.chord_ffi.Contact

/**
 * The calls of the contacts page on the core. A small interface over [ChordClient], like
 * [space.foid.chord.data.ChatApi], so that the ViewModel runs in JVM tests. It holds no logic.
 */
interface ContactsApi {
    suspend fun contacts(): List<Contact>
    suspend fun blocked(): List<String>
    suspend fun add(jid: String, name: String?)
    suspend fun remove(jid: String)

    /** A null name removes the name. */
    suspend fun rename(jid: String, name: String?)
    suspend fun block(jid: String)
    suspend fun unblock(jid: String)
    fun account(): String
}

/** [ContactsApi] on a [ChordClient]. */
class ClientContactsApi(private val client: ChordClient) : ContactsApi {
    override suspend fun contacts(): List<Contact> = client.contacts()
    override suspend fun blocked(): List<String> = client.blockedContacts()
    override suspend fun add(jid: String, name: String?) = client.addContact(jid, name)
    override suspend fun remove(jid: String) = client.removeContact(jid)
    override suspend fun rename(jid: String, name: String?) = client.renameContact(jid, name)
    override suspend fun block(jid: String) = client.blockContact(jid)
    override suspend fun unblock(jid: String) = client.unblockContact(jid)
    override fun account(): String = client.account()
}

/** The answer to the "Add contact" form. */
sealed interface AddResult {
    data class Sent(val jid: String) : AddResult
    data class Failed(val text: String) : AddResult
}

/**
 * What the contacts page and the Home list know. [contacts] is the roster as the core gives
 * it. [blocked] holds the addresses of the blocklist. Contact requests are not here: the
 * [InboxViewModel] holds them.
 */
data class ContactsState(
    val contacts: List<Contact> = emptyList(),
    val blocked: List<String> = emptyList(),
    val loaded: Boolean = false,
    /** The addresses with a call on the way. */
    val busy: Set<String> = emptySet(),
    val adding: Boolean = false,
    val addResult: AddResult? = null,
    val error: String? = null,
) {
    /** The contact of an address, ignoring case. */
    fun find(jid: String): Contact? = contacts.firstOrNull { it.jid.equals(jid.substringBefore('/'), ignoreCase = true) }
}

/**
 * The contacts of the account and the blocklist, kept live. The core sends `ContactChanged` for a
 * roster or presence change and `BlockListChanged` for the blocklist: each one reads the lists
 * again. Changes in a quick run share one read.
 *
 * [events] is the event flow of the session. It has no replay, so the ViewModel must live as
 * long as the main screen.
 */
class ContactsViewModel(
    private val api: () -> ContactsApi?,
    events: Flow<ClientEvent>,
    private val scope: CoroutineScope = mainScope(),
    private val settle: Long = SETTLE_MS,
) : ViewModel(scope) {
    private val _state = MutableStateFlow(ContactsState())
    val state: StateFlow<ContactsState> = _state.asStateFlow()

    // A conflated channel keeps one pending read, even before the collector below started.
    private val reads = Channel<Unit>(Channel.CONFLATED)

    init {
        scope.launch {
            for (ignored in reads) {
                load()
                // Presence arrives in bursts: wait a little, then read once for all of them.
                delay(settle)
            }
        }
        scope.launch {
            events.collect { e ->
                when (e) {
                    is ClientEvent.ContactChanged, ClientEvent.BlockListChanged, is ClientEvent.ConnectionState -> refresh()
                    else -> Unit
                }
            }
        }
        refresh()
    }

    private fun requireApi(): ContactsApi = api() ?: throw ChordException.NotConnected()

    /** Read the lists again. */
    fun refresh() {
        reads.trySend(Unit)
    }

    private suspend fun load() {
        try {
            val a = requireApi()
            val contacts = a.contacts()
            val blocked = a.blocked()
            _state.update { it.copy(contacts = contacts, blocked = blocked, loaded = true) }
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            logWarn("ContactsViewModel", "read failed", e)
            _state.update { it.copy(loaded = true) }
        }
    }

    fun clearError() = _state.update { it.copy(error = null) }
    fun clearAddResult() = _state.update { it.copy(addResult = null) }

    /** Ask [input] to be a contact. [input] is an address or an `xmpp:` link. */
    fun add(input: String, name: String? = null) {
        if (_state.value.adding) return
        val jid = personAddressOf(input)?.lowercase()
        if (jid == null) {
            _state.update { it.copy(addResult = AddResult.Failed("That is not an address. Use name@server.example.")) }
            return
        }
        _state.update { it.copy(adding = true, addResult = null) }
        scope.launch {
            try {
                val a = requireApi()
                when {
                    jid == a.account().substringBefore('/').lowercase() ->
                        _state.update { it.copy(adding = false, addResult = AddResult.Failed("That is your own address.")) }
                    _state.value.find(jid) != null ->
                        _state.update { it.copy(adding = false, addResult = AddResult.Failed("$jid is a contact already.")) }
                    else -> {
                        a.add(jid, name?.trim()?.ifEmpty { null })
                        _state.update { it.copy(adding = false, addResult = AddResult.Sent(jid)) }
                        refresh()
                    }
                }
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                logWarn("ContactsViewModel", "add failed", e)
                _state.update { it.copy(adding = false, addResult = AddResult.Failed(describeError(e))) }
            }
        }
    }

    fun remove(jid: String, onDone: () -> Unit = {}) = act(jid, onDone) { it.remove(jid) }
    fun rename(jid: String, name: String?) = act(jid) { it.rename(jid, name?.trim()?.ifEmpty { null }) }
    fun block(jid: String) = act(jid) { it.block(jid) }
    fun unblock(jid: String) = act(jid) { it.unblock(jid) }

    private fun act(jid: String, onDone: () -> Unit = {}, call: suspend (ContactsApi) -> Unit) {
        if (jid in _state.value.busy) return
        _state.update { it.copy(busy = it.busy + jid, error = null) }
        scope.launch {
            try {
                call(requireApi())
                _state.update { it.copy(busy = it.busy - jid) }
                load()
                onDone()
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                logWarn("ContactsViewModel", "action failed", e)
                _state.update { it.copy(busy = it.busy - jid, error = describeError(e)) }
            }
        }
    }

    companion object {
        const val SETTLE_MS = 250L

        val factory: ViewModelProvider.Factory = viewModelFactory {
            initializer {
                val app = this[ViewModelProvider.AndroidViewModelFactory.APPLICATION_KEY] as ChordApp
                ContactsViewModel(
                    { app.session.client.value?.let(::ClientContactsApi) },
                    app.session.events,
                )
            }
        }
    }
}

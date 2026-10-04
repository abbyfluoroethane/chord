package space.foid.chord.data

import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import uniffi.chord_ffi.ChordClient
import uniffi.chord_ffi.ClientEvent
import uniffi.chord_ffi.ConnectionState

/**
 * The signed-in account, shared by the UI and the connection service. One per process.
 * ChordApp creates it. The ViewModels and ChordConnectionService read it.
 *
 * This interface is the contract between the data layer and the service. Keep it small.
 *
 * [ChordSessionImpl] is the implementation. [signIn], [restore] and [signOut] run one at a time.
 * [events] keeps 256 events for slow collectors and drops the oldest after that; the core thread never waits.
 * [restore] and [signIn] throw the error of the core (ChordException) when the login fails.
 */
interface ChordSession {
    /** The client of the signed-in account, or null when nobody is signed in. */
    val client: StateFlow<ChordClient?>

    /** The connection state. [ConnectionState.Disconnected] when nobody is signed in. */
    val connection: StateFlow<ConnectionState>

    /** Every client event of the current client. Subscribed before login, so none are lost. */
    val events: SharedFlow<ClientEvent>

    /**
     * Open the store for [jid] in the app's private files directory and log in.
     * [server] is "" for SRV lookup, or "host", or "starttls://host:port" (see ChordClient.login).
     * On success the credentials are saved, and the connection service starts.
     */
    suspend fun signIn(jid: String, password: String, server: String)

    /**
     * Offline-first start with the saved credentials. It opens the store of the saved account,
     * publishes [client] and starts the service at once, with no network. The login runs in the
     * background and retries after a network failure; [connection] shows the state. Returns false
     * if there are no saved credentials, or true at once if a client is already open.
     * A real auth failure of the background login clears the credentials, closes the client
     * and sets [client] to null: the UI then shows the sign-in screen.
     */
    suspend fun restore(): Boolean

    /** Log out, delete the saved credentials, close the client, and stop the service. */
    suspend fun signOut()
}

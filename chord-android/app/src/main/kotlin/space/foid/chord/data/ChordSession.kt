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

    /** Sign in again with the saved credentials, if there are any. Returns false if none. */
    suspend fun restore(): Boolean

    /** Log out, delete the saved credentials, close the client, and stop the service. */
    suspend fun signOut()
}

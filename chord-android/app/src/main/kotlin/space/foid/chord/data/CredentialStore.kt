package space.foid.chord.data

/** The account data that Chord needs to sign in again without the user. */
data class Credentials(
    val jid: String,
    val password: String,
    /** "" for SRV lookup, "host", or "starttls://host:port" (see ChordClient.login). */
    val server: String,
)

/**
 * Keeps the credentials of the one signed-in account between runs of the app.
 *
 * The production implementation is `secure/KeystoreCredentialStore`: it encrypts with a key
 * in the Android Keystore. [ChordSession] calls it after a successful sign-in, and when
 * the app restores the session. Calls may block on disk and the Keystore, so [ChordSession]
 * calls them from a background dispatcher. An implementation must be safe to call from any thread.
 */
interface CredentialStore {
    /** Replace the saved credentials. */
    fun save(jid: String, password: String, server: String)

    /** The saved credentials, or null when there are none or they cannot be read. */
    fun load(): Credentials?

    /** Delete the saved credentials. Does nothing when there are none. */
    fun clear()
}

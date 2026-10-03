package space.foid.chord.data

/** The saved login of the signed-in account. */
data class Credentials(val jid: String, val password: String, val server: String)

/** Keeps the login across app restarts. The password never leaves the device unencrypted. */
interface CredentialStore {
    fun save(jid: String, password: String, server: String)

    /** The saved login, or null if there is none or it can no longer be read. */
    fun load(): Credentials?

    fun clear()
}

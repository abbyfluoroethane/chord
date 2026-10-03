package space.foid.chord.secure

import android.content.Context
import android.content.SharedPreferences
import android.util.Base64
import space.foid.chord.data.CredentialStore
import space.foid.chord.data.Credentials

/**
 * Saves the login in private preferences. The password is encrypted with a key that stays in
 * the AndroidKeyStore. If the key is lost, for example after a restore to a new device, [load]
 * clears everything and returns null, so the user signs in again.
 */
class KeystoreCredentialStore(
    private val prefs: SharedPreferences,
    private val cipher: PasswordCipher,
) : CredentialStore {

    constructor(context: Context) : this(
        context.applicationContext.getSharedPreferences(FILE, Context.MODE_PRIVATE),
        AndroidKeystoreCipher(),
    )

    override fun save(jid: String, password: String, server: String) {
        val sealed = cipher.encrypt(password.toByteArray(Charsets.UTF_8))
        prefs.edit()
            .putString(JID, jid)
            .putString(SERVER, server)
            .putString(IV, b64(sealed.iv))
            .putString(DATA, b64(sealed.data))
            .commit()
    }

    override fun load(): Credentials? {
        val jid = prefs.getString(JID, null) ?: return null
        val server = prefs.getString(SERVER, null) ?: return null
        val iv = prefs.getString(IV, null) ?: return null
        val data = prefs.getString(DATA, null) ?: return null
        return try {
            val plain = cipher.decrypt(Sealed(unb64(iv), unb64(data)))
            Credentials(jid, String(plain, Charsets.UTF_8), server)
        } catch (e: Exception) {
            // Key lost or data damaged. Nothing can be recovered.
            clear()
            null
        }
    }

    override fun clear() {
        prefs.edit().clear().commit()
        try {
            cipher.deleteKey()
        } catch (_: Exception) {
            // Nothing more to do. The key is useless without the ciphertext.
        }
    }

    private fun b64(b: ByteArray) = Base64.encodeToString(b, Base64.NO_WRAP)

    private fun unb64(s: String) = Base64.decode(s, Base64.NO_WRAP)

    private companion object {
        const val FILE = "chord_credentials"
        const val JID = "jid"
        const val SERVER = "server"
        const val IV = "iv"
        const val DATA = "data"
    }
}

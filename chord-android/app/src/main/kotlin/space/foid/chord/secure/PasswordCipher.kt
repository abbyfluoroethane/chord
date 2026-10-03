package space.foid.chord.secure

import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/** An encrypted value: the GCM nonce and the ciphertext with its tag. */
class Sealed(val iv: ByteArray, val data: ByteArray)

/** The crypto behind [KeystoreCredentialStore]. A fake replaces it in the unit tests. */
interface PasswordCipher {
    fun encrypt(plain: ByteArray): Sealed

    /** Throws if the key is gone or the data was changed. */
    fun decrypt(sealed: Sealed): ByteArray

    /** Delete the key. The next [encrypt] makes a new one. */
    fun deleteKey()
}

/** AES-256-GCM with a key in the AndroidKeyStore. No user authentication is needed. */
class AndroidKeystoreCipher(private val alias: String = "chord_credentials") : PasswordCipher {
    private fun keyStore(): KeyStore = KeyStore.getInstance(PROVIDER).apply { load(null) }

    private fun key(create: Boolean): SecretKey? {
        (keyStore().getKey(alias, null) as? SecretKey)?.let { return it }
        if (!create) return null
        val spec = KeyGenParameterSpec.Builder(
            alias,
            KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
        )
            .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
            .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
            .setKeySize(256)
            .build()
        return KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, PROVIDER)
            .apply { init(spec) }
            .generateKey()
    }

    override fun encrypt(plain: ByteArray): Sealed {
        val cipher = Cipher.getInstance(TRANSFORM)
        cipher.init(Cipher.ENCRYPT_MODE, key(create = true))
        return Sealed(cipher.iv, cipher.doFinal(plain))
    }

    override fun decrypt(sealed: Sealed): ByteArray {
        val k = key(create = false) ?: error("key is gone")
        val cipher = Cipher.getInstance(TRANSFORM)
        cipher.init(Cipher.DECRYPT_MODE, k, GCMParameterSpec(128, sealed.iv))
        return cipher.doFinal(sealed.data)
    }

    override fun deleteKey() {
        val ks = keyStore()
        if (ks.containsAlias(alias)) ks.deleteEntry(alias)
    }

    private companion object {
        const val PROVIDER = "AndroidKeyStore"
        const val TRANSFORM = "AES/GCM/NoPadding"
    }
}

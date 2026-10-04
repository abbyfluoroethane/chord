package space.foid.chord.secure

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** Robolectric has no AndroidKeyStore, so a fake cipher stands in. XOR is enough to test storage. */
private class FakeCipher : PasswordCipher {
    var keyLost = false
    var deleted = 0

    override fun encrypt(plain: ByteArray) = Sealed(byteArrayOf(1, 2, 3), plain.map { (it.toInt() xor 0x5a).toByte() }.toByteArray())

    override fun decrypt(sealed: Sealed): ByteArray {
        check(!keyLost) { "key is gone" }
        return sealed.data.map { (it.toInt() xor 0x5a).toByte() }.toByteArray()
    }

    override fun deleteKey() {
        deleted++
    }
}

@RunWith(RobolectricTestRunner::class)
class KeystoreCredentialStoreTest {
    private lateinit var cipher: FakeCipher
    private lateinit var store: KeystoreCredentialStore
    private val prefs get() = ApplicationProvider.getApplicationContext<Context>()
        .getSharedPreferences("test_creds", Context.MODE_PRIVATE)

    @Before
    fun setUp() {
        prefs.edit().clear().commit()
        cipher = FakeCipher()
        store = KeystoreCredentialStore(prefs, cipher)
    }

    @Test
    fun emptyStoreLoadsNull() = assertNull(store.load())

    @Test
    fun roundTrip() {
        store.save("a@b.example", "s3cret pässword", "starttls://h:5222")
        val c = store.load()
        assertNotNull(c)
        assertEquals("a@b.example", c!!.jid)
        assertEquals("s3cret pässword", c.password)
        assertEquals("starttls://h:5222", c.server)
    }

    @Test
    fun passwordIsNotStoredInPlain() {
        store.save("a@b.example", "s3cret", "")
        assertFalse(prefs.all.values.any { it.toString().contains("s3cret") })
    }

    @Test
    fun clearRemovesEverythingAndKey() {
        store.save("a@b.example", "pw", "")
        store.clear()
        assertNull(store.load())
        assertTrue(prefs.all.isEmpty())
        assertEquals(1, cipher.deleted)
    }

    @Test
    fun lostKeyClearsAndReturnsNull() {
        store.save("a@b.example", "pw", "")
        cipher.keyLost = true
        assertNull(store.load())
        assertTrue(prefs.all.isEmpty())
    }

    @Test
    fun damagedBase64ClearsAndReturnsNull() {
        store.save("a@b.example", "pw", "")
        prefs.edit().putString("iv", "!!not base64!!").commit()
        assertNull(store.load())
        assertTrue(prefs.all.isEmpty())
    }
}

package space.foid.chord.ui.register

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.chord_ffi.ChordException
import uniffi.chord_ffi.FormField

class RegisterLogicTest {
    @Test
    fun domainIsTheServerPartInLowerCase() {
        assertEquals("foid.space", domainOf(" Rin@Foid.Space "))
        assertEquals("foid.space", domainOf("foid.space"))
        assertEquals("", domainOf("  "))
        assertEquals("", domainOf("rin@"))
    }

    @Test
    fun hostOfDropsSchemeAndPort() {
        assertEquals("chat.example.com", hostOf("starttls://chat.example.com:5222", "a@foid.space"))
        assertEquals("chat.example.com", hostOf("chat.example.com:5222", "a@foid.space"))
        assertEquals("foid.space", hostOf("", "a@foid.space"))
        assertEquals("", hostOf("", "a"))
    }

    @Test
    fun legacyFieldsGetLabelsAndSecrets() {
        val f = legacyFields(listOf("username", "password", "odd"))
        assertEquals(listOf("Username", "Password", "odd"), f.map { it.label })
        assertEquals(listOf(false, true, false), f.map { it.secret })
    }

    @Test
    fun legacyMissingNamesEmptyFieldsInOrder() {
        val names = listOf("username", "password")
        assertEquals(listOf("Username", "Password"), legacyMissing(names, emptyMap()))
        assertEquals(listOf("Password"), legacyMissing(names, mapOf("username" to "rin", "password" to "  ")))
        assertTrue(legacyMissing(names, mapOf("username" to "rin", "password" to "x")).isEmpty())
    }

    @Test
    fun legacyAnswerKeepsTheServerOrderAndTrims() {
        assertEquals(
            listOf(FormField("username", "rin"), FormField("password", "pw")),
            legacyAnswer(listOf("username", "password"), mapOf("password" to "pw", "username" to " rin ")),
        )
    }

    @Test
    fun registerErrorsTellWhatToDo() {
        assertTrue(registerErrorText(ChordException.Server("Forbidden")).contains("does not let people create an account"))
        assertTrue(registerErrorText(ChordException.Server("service-unavailable")).contains("invite link"))
        assertEquals("That username is taken. Try another one.", registerErrorText(ChordException.Server("Conflict")))
        assertTrue(registerErrorText(ChordException.Server("NotAcceptable")).contains("password rules"))
        assertEquals("The server refused the request.", registerErrorText(ChordException.Server("other")))
        assertEquals("Can't reach the server. Check the address and your connection.", registerErrorText(ChordException.Unreachable("x")))
        assertEquals("The server's certificate is not valid, so Chord did not connect.", registerErrorText(ChordException.TlsInvalid("x")))
        assertEquals("The server did not answer in time. Try again.", registerErrorText(ChordException.Timeout()))
        assertEquals("Something went wrong. Try again.", registerErrorText(RuntimeException("x")))
    }
}

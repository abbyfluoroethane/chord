package space.foid.chord.ui.forms

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.chord_ffi.DataFieldKind
import uniffi.chord_ffi.DataFormKind
import uniffi.chord_ffi.DataMedia
import uniffi.chord_ffi.DataOption

class DataFormLogicTest {
    @Test
    fun nameIsLabelThenVar() {
        assertEquals("User", fieldName(field(DataFieldKind.TEXT_SINGLE, "username", "User")))
        assertEquals("username", fieldName(field(DataFieldKind.TEXT_SINGLE, "username", " ")))
        assertEquals("", fieldName(field(DataFieldKind.FIXED, null)))
    }

    @Test
    fun booleanReadsOneAndTrue() {
        assertTrue(isOn(field(DataFieldKind.BOOLEAN, "a", values = listOf("1"))))
        assertTrue(isOn(field(DataFieldKind.BOOLEAN, "a", values = listOf(" true "))))
        assertFalse(isOn(field(DataFieldKind.BOOLEAN, "a", values = listOf("0"))))
        assertFalse(isOn(field(DataFieldKind.BOOLEAN, "a")))
        assertEquals(listOf("1"), boolValue(true))
        assertEquals(listOf("0"), boolValue(false))
    }

    @Test
    fun multilineRoundTrip() {
        assertEquals("a\nb", toMultiline(listOf("a", "b")))
        assertEquals(listOf("a", "b"), fromMultiline("a\nb"))
        assertEquals(emptyList<String>(), fromMultiline(""))
    }

    @Test
    fun toggleKeepsTheOrderOfTheOptionsAndForeignValues() {
        val f = field(
            DataFieldKind.LIST_MULTI, "r", values = listOf("b", "x"),
            options = listOf(DataOption(null, "a"), DataOption(null, "b"), DataOption(null, "c")),
        )
        assertEquals(listOf("a", "b", "x"), toggleValue(f, "a", true))
        assertEquals(listOf("x"), toggleValue(f, "b", false))
        assertEquals(listOf("b", "c", "x"), toggleValue(f, "c", true))
    }

    @Test
    fun selectedOptionIsEmptyForAnUnknownValue() {
        val opts = listOf(DataOption("One", "1"), DataOption(null, "2"))
        assertEquals("1", selectedOption(field(DataFieldKind.LIST_SINGLE, "l", values = listOf("1"), options = opts)))
        assertEquals("", selectedOption(field(DataFieldKind.LIST_SINGLE, "l", values = listOf("9"), options = opts)))
        assertEquals("One", optionText(opts[0]))
        assertEquals("2", optionText(opts[1]))
    }

    @Test
    fun jidRule() {
        assertTrue(isJid("a@b.c"))
        assertTrue(isJid("b.c"))
        assertTrue(isJid("a@b.c/res"))
        assertFalse(isJid("a b@c"))
        assertFalse(isJid("a@@c"))
    }

    @Test
    fun problemsListRequiredEmptyFieldsAndBadJids() {
        val f = form(
            field(DataFieldKind.TEXT_SINGLE, "username", "Username", required = true),
            field(DataFieldKind.TEXT_PRIVATE, "password", "Password", required = true, values = listOf("  ")),
            field(DataFieldKind.BOOLEAN, "b", "Flag", required = true),
            field(DataFieldKind.JID_MULTI, "o", "Owners", values = listOf("a@b.c", "bad jid")),
            field(DataFieldKind.HIDDEN, "FORM_TYPE", required = true),
            field(DataFieldKind.FIXED, null),
        )
        assertEquals(
            listOf("Username is required", "Password is required", "Owners: bad jid is not a valid address"),
            problems(f),
        )
        assertEquals("Username is required", fieldProblem(f.fields[0]))
        assertNull(fieldProblem(f.fields[2]))
    }

    @Test
    fun aFilledFormHasNoProblems() {
        assertTrue(problems(allKindsForm()).isEmpty())
    }

    @Test
    fun onlyInlineImagesLoad() {
        assertNotNull(inlineImage(DataMedia(TINY_PNG, "image/png", null, null)))
        assertNull(inlineImage(DataMedia("https://x.example/c.png", "image/png", null, null)))
        assertNull(inlineImage(DataMedia("data:text/html;base64,AAAA", null, null, null)))
        val f = field(
            DataFieldKind.TEXT_SINGLE, "ocr",
            media = listOf(DataMedia(TINY_PNG, null, null, null), DataMedia("https://x.example/c.png", null, null, null)),
        )
        assertEquals(1, otherMedia(f).size)
    }

    @Test
    fun withValuesReplacesOneField() {
        val f = registrationFormWithCaptcha()
        val g = f.withValues(1, listOf("rin"))
        assertEquals(listOf("rin"), g.fields[1].values)
        assertEquals(f.fields[2], g.fields[2])
        assertTrue(f.fields[1].values.isEmpty())
    }

    @Test
    fun submissionIsASubmitCopy() {
        val f = registrationFormWithCaptcha()
        val s = submission(f)
        assertEquals(DataFormKind.SUBMIT, s.kind)
        assertEquals(f.fields, s.fields)
        assertEquals(DataFormKind.FORM, f.kind)
    }

    @Test
    fun credentialsComeFromUsernameAndPassword() {
        val f = registrationFormWithCaptcha().withValues(1, listOf(" rin ")).withValues(2, listOf(" pw "))
        assertEquals("rin" to " pw ", credentialsOf(f))
        assertEquals("" to "", credentialsOf(form()))
    }
}

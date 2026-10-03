package space.foid.chord.ui.forms

import uniffi.chord_ffi.DataField
import uniffi.chord_ffi.DataFieldKind
import uniffi.chord_ffi.DataForm
import uniffi.chord_ffi.DataFormKind
import uniffi.chord_ffi.DataMedia
import uniffi.chord_ffi.DataOption

// The rules of the data form renderer (XEP-0004). Plain functions, so JVM tests can run them.
// They follow chord-desktop/src/lib/ui/forms.ts.

/** The name to show for a field: its label, or its name. */
fun fieldName(f: DataField): String = f.label?.trim()?.takeIf { it.isNotEmpty() } ?: f.`var` ?: ""

fun firstValue(f: DataField): String = f.values.firstOrNull() ?: ""

/** A boolean field is on for "1" or "true". */
fun isOn(f: DataField): Boolean = firstValue(f).trim().let { it == "1" || it == "true" }

fun boolValue(on: Boolean): List<String> = listOf(if (on) "1" else "0")

fun toMultiline(values: List<String>): String = values.joinToString("\n")

fun fromMultiline(text: String): List<String> = if (text.isEmpty()) emptyList() else text.split('\n')

/** Add or remove one value of a list-multi field. The order of the options stays. */
fun toggleValue(f: DataField, value: String, on: Boolean): List<String> {
    val has = LinkedHashSet(f.values)
    if (on) has.add(value) else has.remove(value)
    val known = f.options.map { it.value }.filter { it in has }
    // A value that is not an option (the server allows it) stays at the end.
    val extra = has.filter { v -> f.options.none { it.value == v } }
    return known + extra
}

fun optionText(o: DataOption): String = o.label?.trim()?.takeIf { it.isNotEmpty() } ?: o.value

/** The selected option of a list-single field, or "" when the value is not an option. */
fun selectedOption(f: DataField): String = firstValue(f).takeIf { v -> f.options.any { it.value == v } } ?: ""

private val JID = Regex("""^([^@/\s]+@)?[^@/\s]+(/\S.*)?$""")

fun isJid(s: String): Boolean = JID.matches(s.trim())

/** What is wrong with the answers of one field, or null. */
fun fieldProblem(f: DataField): String? {
    if (f.kind == DataFieldKind.FIXED || f.kind == DataFieldKind.HIDDEN) return null
    val name = fieldName(f)
    val filled = f.values.any { it.isNotBlank() }
    if (f.required && f.kind != DataFieldKind.BOOLEAN && !filled) return "$name is required"
    if (f.kind == DataFieldKind.JID_SINGLE || f.kind == DataFieldKind.JID_MULTI) {
        f.values.firstOrNull { it.isNotBlank() && !isJid(it) }?.let { return "$name: $it is not a valid address" }
    }
    return null
}

/** What is wrong with the answers. Empty when the form can go out. */
fun problems(form: DataForm): List<String> = form.fields.mapNotNull { fieldProblem(it) }

private val IMAGE_URI = Regex("""^data:image/(png|jpeg|gif|webp);base64,[A-Za-z0-9+/=]+$""")

/**
 * The URI of an image that is safe to show. Only an inline image qualifies: a link to a
 * remote image would tell that server the address of the reader.
 */
fun inlineImage(m: DataMedia): String? = m.uri.takeIf { IMAGE_URI.matches(it) }

/** The media of a field that are not inline images. */
fun otherMedia(f: DataField): List<DataMedia> = f.media.filter { inlineImage(it) == null }

/** The form to send back: a copy of the fields, with the kind "submit". */
fun submission(form: DataForm): DataForm =
    form.copy(kind = DataFormKind.SUBMIT, fields = form.fields.map { it.copy(values = it.values.toList()) })

/** A copy of [form] with the values of field [index] replaced. */
fun DataForm.withValues(index: Int, values: List<String>): DataForm =
    copy(fields = fields.mapIndexed { i, f -> if (i == index) f.copy(values = values) else f })

/** The username and the password that a registration form asks for, if it has them. */
fun credentialsOf(form: DataForm): Pair<String, String> {
    val username = form.fields.firstOrNull { it.`var` == "username" }?.values?.firstOrNull()?.trim() ?: ""
    val password = form.fields.firstOrNull { it.`var` == "password" }?.values?.firstOrNull() ?: ""
    return username to password
}

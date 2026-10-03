package space.foid.chord.ui.register

import uniffi.chord_ffi.ChordException
import uniffi.chord_ffi.FormField

// Pure rules of the registration (chord-desktop/src/lib/ui/forms.ts, "registration").

/** A legacy registration field (XEP-0077) as the screen shows it. */
data class LegacyField(val name: String, val label: String, val secret: Boolean)

private val LEGACY_LABELS = mapOf(
    "username" to "Username",
    "password" to "Password",
    "nick" to "Nickname",
    "name" to "Full name",
    "first" to "First name",
    "last" to "Last name",
    "email" to "Email address",
    "address" to "Address",
    "city" to "City",
    "state" to "State",
    "zip" to "Postal code",
    "phone" to "Phone number",
    "url" to "Web page",
    "date" to "Date",
    "misc" to "Other",
    "text" to "Note",
    "key" to "Key",
)

fun legacyFields(names: List<String>): List<LegacyField> =
    names.map { LegacyField(it, LEGACY_LABELS[it] ?: it, it == "password") }

/** The answer of the legacy fields, in the order that the server named them. */
fun legacyAnswer(names: List<String>, values: Map<String, String>): List<FormField> =
    names.map { FormField(it, (values[it] ?: "").trim()) }

/** The labels of the legacy fields that have no value. All of them are required. */
fun legacyMissing(names: List<String>, values: Map<String, String>): List<String> =
    legacyFields(names).filter { (values[it.name] ?: "").isBlank() }.map { it.label }

/** The server part of an address, or the text itself when it has no "@". Lower case. */
fun domainOf(input: String): String {
    val s = input.trim()
    val at = s.lastIndexOf('@')
    return (if (at >= 0) s.substring(at + 1) else s).lowercase()
}

/** The host of the "Server" field of the sign-in form, without a scheme or a port. Used as the status text. */
fun hostOf(server: String, address: String): String {
    val s = server.trim().removePrefix("starttls://").replace(Regex(""":\d+$"""), "")
    return s.ifEmpty { address.substringAfter('@', "").trim() }
}

/**
 * The text for a registration error. A server that keeps registration closed answers with
 * forbidden, not-allowed or service-unavailable. Most servers do, so the text says what to do next.
 */
fun registerErrorText(error: Throwable): String = when (error) {
    is ChordException.Server -> when {
        Regex("Forbidden|NotAllowed|ServiceUnavailable|FeatureNotImplemented|forbidden|not-allowed|service-unavailable")
            .containsMatchIn(error.detail) ->
            "This server does not let people create an account in the app. Ask its owner for an invite link, or use its website."
        Regex("Conflict|conflict").containsMatchIn(error.detail) -> "That username is taken. Try another one."
        Regex("NotAcceptable|not-acceptable").containsMatchIn(error.detail) ->
            "The server did not accept these answers. Check each field, and the password rules of the server."
        else -> "The server refused the request."
    }
    is ChordException.Unreachable -> "Can't reach the server. Check the address and your connection."
    is ChordException.TlsInvalid -> "The server's certificate is not valid, so Chord did not connect."
    is ChordException.Timeout -> "The server did not answer in time. Try again."
    is ChordException.InvalidServer -> "The server setting is not valid. Leave it empty, or use host or starttls://host:port."
    is ChordException.InvalidJid -> "That is not a valid server name."
    else -> "Something went wrong. Try again."
}

/** The text for a sign-in that failed after the account was created. */
const val SIGN_IN_AFTER_REGISTER_FAILED = "Your account was created, but signing in failed. Go back and sign in with the new account."

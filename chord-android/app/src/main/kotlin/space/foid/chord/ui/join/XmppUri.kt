package space.foid.chord.ui.join

import java.net.IDN
import java.net.URLDecoder

// Reads addresses and `xmpp:` URIs (RFC 5122, XEP-0147). It is the small mobile form of
// chord-desktop/src/lib/ui/xmppuri.ts. The functions are pure and never throw.
//
// Forms that Chord knows:
//   room     xmpp:ROOM?join[;password=SECRET]
//   chat     xmpp:USER   and   xmpp:USER?message[;body=TEXT]
//   contact  xmpp:USER?roster[;name=NAME]   and   xmpp:USER?subscribe
//   space    xmpp:SERVICE?;node=NODE   and   xmpp:SERVICE?pubsub;action=subscribe;node=NODE

/** What a link asks for. */
sealed interface XmppTarget {
    data class Room(val jid: String, val password: String? = null) : XmppTarget
    data class Chat(val jid: String, val body: String? = null) : XmppTarget
    data class Contact(val jid: String, val name: String? = null) : XmppTarget
    data class Space(val service: String, val node: String) : XmppTarget

    /** The link is not valid, or asks for something that Chord does not do. */
    data object Invalid : XmppTarget
}

/** The longest link that Chord reads. A longer one is not a real invite. */
const val MAX_URI_LENGTH = 2048
private const val MAX_PART_BYTES = 1023

private val BAD_CHARS = Regex("[\\u0000-\\u001f\\u007f-\\u009f\\u2028\\u2029]")
private val BAD_LOCAL = Regex("[\\s\"&'/:<>@]")
private val LABEL = Regex("^[a-z0-9_](?:[a-z0-9_-]*[a-z0-9_])?$")

/** True when the text starts like an `xmpp:` URI. It does not check the rest. */
fun looksLikeXmppUri(s: String): Boolean = s.trim().startsWith("xmpp:", ignoreCase = true)

private fun bytes(s: String) = s.toByteArray(Charsets.UTF_8).size

private fun domainOf(raw: String): String? {
    val d = raw.trimEnd('.')
    if (d.isEmpty() || bytes(d) > MAX_PART_BYTES || d.any { it in "\\/?#@:%[]<>^| \t\r\n" }) return null
    val ascii = try {
        IDN.toASCII(d, IDN.ALLOW_UNASSIGNED).lowercase()
    } catch (_: Exception) {
        return null
    }
    if (ascii.isEmpty() || ascii.length > 253) return null
    return if (ascii.split('.').all { it.length in 1..63 && LABEL.matches(it) }) ascii else null
}

/** A service address (`host` or `user@host`) with no resource. Null if it is bad. */
private fun serviceOf(s: String): String? {
    if (s.isEmpty() || BAD_CHARS.containsMatchIn(s)) return null
    val noResource = s.substringBefore('/')
    return if ('@' in noResource) parseBareJid(noResource) else domainOf(noResource)
}

/**
 * A bare address `user@host`. A resource is dropped, the local part is lower-cased. Null if the
 * text is not an address.
 */
fun parseBareJid(input: String): String? {
    val s = input.trim()
    if (s.isEmpty() || BAD_CHARS.containsMatchIn(s)) return null
    val bare = s.substringBefore('/')
    if (bare.count { it == '@' } != 1) return null
    val local = bare.substringBefore('@')
    val domain = domainOf(bare.substringAfter('@')) ?: return null
    if (local.isEmpty() || bytes(local) > MAX_PART_BYTES || BAD_LOCAL.containsMatchIn(local)) return null
    return "${local.lowercase()}@$domain"
}

private fun decode(s: String): String? = try {
    val out = URLDecoder.decode(s.replace("+", "%2B"), "UTF-8")
    if (BAD_CHARS.containsMatchIn(out)) null else out
} catch (_: Exception) {
    null
}

/** Parse an `xmpp:` URI. Never throws. */
fun parseXmppUri(input: String): XmppTarget {
    val uri = input.trim()
    if (uri.isEmpty() || uri.length > MAX_URI_LENGTH || !uri.startsWith("xmpp:", ignoreCase = true)) return XmppTarget.Invalid
    if (uri.any { it.isWhitespace() || it.code < 0x20 || it.code == 0x7f }) return XmppTarget.Invalid
    val rest = uri.substring(5).substringBefore('#')
    // `xmpp://authority/path` acts as another account. Chord does not support it.
    if (rest.startsWith("//")) return XmppTarget.Invalid
    val q = rest.indexOf('?')
    val rawPath = if (q < 0) rest else rest.substring(0, q)
    val query = if (q < 0) null else rest.substring(q + 1)
    val path = decode(rawPath)?.takeIf { it.isNotEmpty() } ?: return XmppTarget.Invalid

    val parts = (query ?: "").split(';')
    val action = decode(parts.first())?.lowercase() ?: return XmppTarget.Invalid
    val params = LinkedHashMap<String, String>()
    for (part in parts.drop(1)) {
        if (part.isEmpty()) continue
        val eq = part.indexOf('=')
        val key = decode(if (eq < 0) part else part.substring(0, eq)) ?: return XmppTarget.Invalid
        val value = decode(if (eq < 0) "" else part.substring(eq + 1)) ?: return XmppTarget.Invalid
        params.putIfAbsent(key, value)
    }

    if (action == "pubsub" || (action.isEmpty() && "node" in params)) {
        val given = params["action"]?.lowercase()
        if (action == "pubsub" && given != "subscribe") return XmppTarget.Invalid
        if (action.isEmpty() && given != null && given != "subscribe") return XmppTarget.Invalid
        val service = serviceOf(path) ?: return XmppTarget.Invalid
        val node = params["node"]?.takeIf { it.isNotBlank() && bytes(it) <= MAX_PART_BYTES } ?: return XmppTarget.Invalid
        return XmppTarget.Space(service, node)
    }

    val jid = parseBareJid(path) ?: return XmppTarget.Invalid
    if (query == null) return XmppTarget.Chat(jid)
    return when (action) {
        "join" -> XmppTarget.Room(jid, params["password"]?.ifEmpty { null })
        "message" -> XmppTarget.Chat(jid, params["body"]?.ifEmpty { null })
        "roster" -> XmppTarget.Contact(jid, params["name"]?.trim()?.ifEmpty { null })
        "subscribe" -> XmppTarget.Contact(jid)
        else -> XmppTarget.Invalid
    }
}

/**
 * The room address in what the user typed into the join field: a bare address, or an `xmpp:`
 * URI. A URI with no `?join` still counts: the user chose the "join" field. Null if bad.
 */
fun roomTargetOf(input: String): XmppTarget.Room? {
    if (looksLikeXmppUri(input)) {
        return when (val t = parseXmppUri(input)) {
            is XmppTarget.Room -> t
            is XmppTarget.Chat -> XmppTarget.Room(t.jid)
            else -> null
        }
    }
    return parseBareJid(input)?.let { XmppTarget.Room(it) }
}

/** The address of a person in what the user typed: a bare address, or an `xmpp:` URI. Null if bad. */
fun personAddressOf(input: String): String? {
    if (looksLikeXmppUri(input)) {
        return when (val t = parseXmppUri(input)) {
            is XmppTarget.Chat -> t.jid
            is XmppTarget.Contact -> t.jid
            is XmppTarget.Room -> t.jid
            else -> null
        }
    }
    return parseBareJid(input)
}

package space.foid.chord.ui.composer

import space.foid.chord.ui.sheets.EmojiGroup
import space.foid.chord.ui.timeline.MessageUi
import uniffi.chord_ffi.ChannelItem
import uniffi.chord_ffi.ChannelKind
import uniffi.chord_ffi.MemberItem

// Pure logic of the composer and of the message actions. No Android types, no FFI calls: the
// JVM tests run it as it is. It follows chord-desktop mentions.ts, shortcodes.ts, menus.ts and
// ForwardModal.svelte.

/** A token under the caret: where it starts in the text (the "@" or ":") and what follows it. */
data class TokenMatch(val start: Int, val query: String)

/** The text and the caret after an insert. */
data class TextEdit(val text: String, val caret: Int)

private val MENTION = Regex("(?:^|\\s)@([^\\s@]{0,32})$")
private val SHORTCODE = Regex("(?:^|\\s):([a-z0-9_+-]{2,32})$", RegexOption.IGNORE_CASE)

/** The "@query" that ends at [caret]. The "@" must start the text or follow a space. */
fun findMention(text: String, caret: Int): TokenMatch? {
    val c = caret.coerceIn(0, text.length)
    val m = MENTION.find(text.substring(0, c)) ?: return null
    val q = m.groupValues[1]
    return TokenMatch(c - q.length - 1, q)
}

/** Up to [max] nicks for [query]: names that start with it first, then names that hold it. */
fun suggestNicks(nicks: List<String>, query: String, max: Int = 8): List<String> {
    val q = query.lowercase()
    val seen = HashSet<String>()
    val starts = ArrayList<String>()
    val holds = ArrayList<String>()
    for (nick in nicks) {
        val n = nick.lowercase()
        if (nick.isEmpty() || !seen.add(n)) continue
        if (n.startsWith(q)) starts += nick else if (n.contains(q)) holds += nick
    }
    val byName = compareBy<String, String>(String.CASE_INSENSITIVE_ORDER) { it }
    return (starts.sortedWith(byName) + holds.sortedWith(byName)).take(max)
}

/** Put "@nick " in place of the "@query". */
fun insertMention(text: String, caret: Int, match: TokenMatch, nick: String): TextEdit {
    val insert = "@$nick "
    val c = caret.coerceIn(0, text.length)
    return TextEdit(text.substring(0, match.start) + insert + text.substring(c), match.start + insert.length)
}

/** The ":query" (two letters or more) that ends at [caret]. The ":" must start the text or follow a space. */
fun findShortcode(text: String, caret: Int): TokenMatch? {
    val c = caret.coerceIn(0, text.length)
    val m = SHORTCODE.find(text.substring(0, c)) ?: return null
    val q = m.groupValues[1]
    return TokenMatch(c - q.length - 1, q)
}

/** One suggestion of the shortcode list. */
data class ShortcodeHit(val name: String, val emoji: String)

/**
 * The names that the shortcode search knows. The catalog has labels and tags, not shortcodes, so
 * a name is the label with underscores ("grinning_face") or a one-word tag ("grin").
 * Build it once for a catalog.
 */
class ShortcodeIndex(groups: List<EmojiGroup>, known: List<ShortcodeHit> = emptyList()) {
    private val entries: List<ShortcodeHit>

    init {
        // The real shortcode table (assets/shortcodes.txt, as on the desktop) wins when it is there.
        entries = known.ifEmpty { fromTags(groups) }
    }

    private fun fromTags(groups: List<EmojiGroup>): List<ShortcodeHit> {
        val list = ArrayList<ShortcodeHit>()
        for (g in groups) for (e in g.emoji) {
            list += ShortcodeHit(shortcodeOf(e.label), e.emoji)
            // "words" is the label, then the tags. A one-word tag is a name too.
            val label = e.label.lowercase()
            e.words.removePrefix(label).trim().split(' ')
                .filter { it.length >= 2 && it.all { c -> c.isLetterOrDigit() } }
                .forEach { list += ShortcodeHit(it, e.emoji) }
        }
        return list
    }

    /** Up to [max] names that start with [prefix], best first, one for each emoji. */
    fun suggest(prefix: String, max: Int = 8): List<ShortcodeHit> {
        val q = prefix.lowercase()
        val found = entries.filter { it.name.startsWith(q) }
        // A skin tone version comes after the base emoji, then short names first.
        fun tone(n: String) = if (TONE.containsMatchIn(n)) 1 else 0
        return found.sortedWith(compareBy<ShortcodeHit>({ tone(it.name) }, { it.name.length }, { it.name }))
            .distinctBy { it.emoji }.distinctBy { it.name }.take(max)
    }

    private companion object {
        val TONE = Regex("_tone\\d|skin_tone|-tone")
    }
}

/** Reads assets/shortcodes.txt: one "name<TAB>emoji" for each line. */
fun parseShortcodes(text: String): List<ShortcodeHit> =
    text.lineSequence().mapNotNull { line ->
        val i = line.indexOf('\t')
        if (i <= 0 || i == line.length - 1) null else ShortcodeHit(line.substring(0, i), line.substring(i + 1))
    }.toList()

/** "grinning face" becomes "grinning_face"; "flag: Fiji" becomes "flag_fiji". */
fun shortcodeOf(label: String): String =
    label.lowercase().replace(Regex("[^a-z0-9+]+"), "_").trim('_')

/** Put [emoji] and a space in place of the ":query". */
fun insertShortcode(text: String, caret: Int, match: TokenMatch, emoji: String): TextEdit {
    val insert = "$emoji "
    val c = caret.coerceIn(0, text.length)
    return TextEdit(text.substring(0, match.start) + insert + text.substring(c), match.start + insert.length)
}

/** Put [emoji] at the caret, or in place of the selection [from]..[to]. */
fun insertAtSelection(text: String, from: Int, to: Int, emoji: String): TextEdit {
    val a = minOf(from, to).coerceIn(0, text.length)
    val b = maxOf(from, to).coerceIn(0, text.length)
    return TextEdit(text.substring(0, a) + emoji + text.substring(b), a + emoji.length)
}

/** The reactions of the quick row when nothing was used yet (desktop DEFAULT_REACTIONS). */
val DefaultReactions = listOf("👍", "❤️", "😂", "👀")

/** The four reactions of the quick row: the ones used most lately, then the defaults. */
fun quickReactions(recent: List<String>, defaults: List<String> = DefaultReactions, n: Int = 4): List<String> =
    (recent + defaults).distinct().take(n)

private val URL = Regex("""https?://[^\s<>"]+""", RegexOption.IGNORE_CASE)

/** The http(s) links in a body, in order, with no repeat. The marks after a link are not part of it. */
fun linksIn(body: String, max: Int = 3): List<String> =
    URL.findAll(body).map { it.value.trimEnd('.', ',', ';', ':', '!', '?', ')', ']', '\'') }
        .filter { it.length > 8 }.distinct().take(max).toList()

/** The link that "Copy channel link" copies (menus.ts channelLink). */
fun channelLink(jid: String, direct: Boolean): String =
    if (direct) "xmpp:$jid?message" else "xmpp:$jid?join"

/** The text of a forwarded message: the body, and the file address when the body does not hold it. */
fun forwardText(body: String, attachment: String?): String =
    listOfNotNull(
        body.trim().ifEmpty { null },
        attachment?.takeIf { it.isNotBlank() && !body.contains(it) },
    ).joinToString("\n")

/** The line that the forward sheet shows for the message. */
fun forwardSummary(body: String, attachment: String?): String = when {
    body.isNotBlank() -> body.trim()
    attachment != null -> "File: ${attachment.substringAfterLast('/').substringBefore('?')}"
    else -> "Empty message"
}

/** A place that a message can be forwarded to. [jid] is the room, the person, or room/nick. */
data class ForwardTarget(val jid: String, val label: String, val hint: String, val direct: Boolean)

/** The chats of the list as targets. */
fun forwardTargets(channels: List<ChannelItem>): List<ForwardTarget> =
    channels.filter { it.joined || it.kind !is ChannelKind.Room }.map { c ->
        val name = c.name.ifBlank { c.jid.substringBefore('/').substringBefore('@') }
        when (c.kind) {
            is ChannelKind.Room -> ForwardTarget(c.jid, name, c.category.orEmpty().ifEmpty { "Channel" }, false)
            else -> ForwardTarget(c.jid, name, "Message", true)
        }
    }

/** The targets that match [query]: names that start with it first. All of them for an empty query. */
fun filterForwardTargets(targets: List<ForwardTarget>, query: String, max: Int = 30): List<ForwardTarget> {
    val q = query.trim().lowercase()
    if (q.isEmpty()) return targets.take(max)
    val starts = targets.filter { it.label.lowercase().startsWith(q) }
    val holds = targets.filter { !it.label.lowercase().startsWith(q) && it.label.lowercase().contains(q) }
    return (starts + holds).take(max)
}

/**
 * True when the account moderates the room: owner or admin, or the role moderator
 * (desktop canModerate). The member is found by the bare address.
 */
fun canModerate(members: List<MemberItem>, account: String?): Boolean {
    if (account == null) return false
    val me = members.firstOrNull { it.jid?.substringBefore('/').equals(account, ignoreCase = true) } ?: return false
    val aff = me.affiliation.lowercase()
    return aff == "owner" || aff == "admin" || me.role.equals("moderator", ignoreCase = true)
}

/** The rows of the message sheet below the links, in desktop order (menus.ts messageMenu). */
enum class MessageAction { Edit, Reply, Forward, CopyText, CopyChannelLink, Delete, Remove, CopyId }

/**
 * The rows for [m]. A deleted message keeps only the link and the ID. Pin and Mark unread are
 * not here: the core has no call for them.
 * [moderator]: the account moderates the room. [channelLink]: the chat has a link to copy.
 */
fun messageActions(m: MessageUi, moderator: Boolean, channelLink: Boolean): List<MessageAction> = buildList {
    val gone = m.retracted
    if (!gone) {
        if (m.outgoing) add(MessageAction.Edit)
        add(MessageAction.Reply)
        if (m.body.isNotBlank() || m.attachment != null) add(MessageAction.Forward)
        if (m.body.isNotEmpty()) add(MessageAction.CopyText)
    }
    if (channelLink) add(MessageAction.CopyChannelLink)
    if (!gone && m.outgoing) add(MessageAction.Delete) else if (!gone && moderator) add(MessageAction.Remove)
    add(MessageAction.CopyId)
}

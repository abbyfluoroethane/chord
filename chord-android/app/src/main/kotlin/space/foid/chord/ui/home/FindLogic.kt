package space.foid.chord.ui.home

import space.foid.chord.ui.contacts.contactName
import space.foid.chord.ui.contacts.peopleOf
import space.foid.chord.ui.join.looksLikeXmppUri
import space.foid.chord.ui.join.parseBareJid
import space.foid.chord.ui.screens.bareJid
import uniffi.chord_ffi.ChannelItem
import uniffi.chord_ffi.ChannelKind
import uniffi.chord_ffi.Contact
import uniffi.chord_ffi.SpaceItem

/**
 * The subsequence score of the quick switcher (fuzzy.ts). Zero means no match. Each letter of
 * [query] must appear in [text] in order. Letters that touch, and letters at the start of a
 * word, score more.
 */
fun fuzzyScore(query: String, text: String): Double {
    val q = query.lowercase()
    val t = text.lowercase()
    if (q.isEmpty()) return 1.0
    var ti = 0
    var s = 0.0
    var streak = 0
    for (ch in q) {
        val at = t.indexOf(ch, ti)
        if (at < 0) return 0.0
        streak = if (at == ti) streak + 1 else 0
        s += 1 + streak * 2 + (if (at == 0 || t[at - 1] in " -_/#@.") 4 else 0)
        ti = at + 1
    }
    if (t.startsWith(q)) s += 10
    return maxOf(s - t.length * 0.05, 0.01)
}

/** What a hit of "Find or start a chat" opens. */
sealed interface FindHit {
    val key: String
    val title: String

    /** A direct chat or a group chat that the list shows. */
    data class Chat(val item: ChannelItem, override val title: String, val direct: Boolean) : FindHit {
        override val key get() = "chat/${item.jid}"
    }

    /** A room of the open space. */
    data class Room(val item: ChannelItem, override val title: String) : FindHit {
        override val key get() = "room/${item.jid}"
    }

    /** A contact without a chat in the list: a tap starts the chat. */
    data class Person(val jid: String, override val title: String) : FindHit {
        override val key get() = "person/$jid"
    }

    data class Space(val item: SpaceItem) : FindHit {
        override val title get() = item.name.ifBlank { item.node }
        override val key get() = "space/${item.service}/${item.node}"
    }

    /** A typed address of a person, with no chat for it yet. */
    data class MessageAddress(val jid: String) : FindHit {
        override val title get() = jid
        override val key get() = "message/$jid"
    }

    /** A typed address that may be a room. The join sheet asks before it joins. */
    data class JoinAddress(val jid: String) : FindHit {
        override val title get() = jid
        override val key get() = "join/$jid"
    }

    /** A typed `xmpp:` link. */
    data class Link(val uri: String) : FindHit {
        override val title get() = uri
        override val key get() = "link/$uri"
    }
}

/** What the user typed, split into a filter and the text. `#` keeps rooms, `@` keeps people. */
data class FindQuery(val text: String, val only: Only? = null) {
    enum class Only { Rooms, People }

    companion object {
        fun parse(raw: String): FindQuery {
            val q = raw.trim()
            return when {
                q.startsWith("#") -> FindQuery(q.drop(1).trim(), Only.Rooms)
                q.startsWith("@") -> FindQuery(q.drop(1).trim(), Only.People)
                else -> FindQuery(q)
            }
        }
    }
}

/**
 * The hits for an address or link in what the user typed, as the desktop (address.ts, typedAddress).
 * A bare address needs a dot in the domain. [known] holds the addresses that have a chat already:
 * they give no hit. The list is empty if the text is neither.
 */
fun typedHits(raw: String, known: Set<String>): List<FindHit> {
    val s = raw.trim().trimStart('#', '@').trim()
    if (s.isEmpty()) return emptyList()
    if (looksLikeXmppUri(s)) return listOf(FindHit.Link(s))
    if (!Regex("^[^@\\s/]+@[^@\\s/]+\\.[^@\\s/]+$").matches(s)) return emptyList()
    val jid = parseBareJid(s) ?: return emptyList()
    if (jid.lowercase() in known) return emptyList()
    return listOf(FindHit.MessageAddress(jid), FindHit.JoinAddress(jid))
}

/** The maximum number of hits on a phone. */
const val FIND_LIMIT = 30

/**
 * The hits of "Find or start a chat".
 *
 * - Home chats first (a direct chat and a group chat), then the rooms of [spaceChannels], then the
 *   contacts that have no chat yet, then the spaces.
 * - With no text the list is the chats in their order, as the desktop shows "recent" first.
 * - With text, the hits are sorted by [fuzzyScore].
 * - A typed address or link that matches no chat comes first.
 *
 * @param homeChannels the list of Home
 * @param spaceChannels the rooms of the space that the user has open, or empty
 */
fun findHits(
    raw: String,
    homeChannels: List<ChannelItem>,
    spaceChannels: List<ChannelItem>,
    contacts: List<Contact>,
    spaces: List<SpaceItem>,
): List<FindHit> {
    val query = FindQuery.parse(raw)
    val chats = homeChannels.map { ch ->
        val direct = ch.kind !is ChannelKind.Room
        FindHit.Chat(ch, ch.name.ifBlank { bareJid(ch.jid) }, direct)
    }
    val rooms = spaceChannels.map { FindHit.Room(it, it.name.ifBlank { bareJid(it.jid) }) }
    val haveChat = homeChannels.filter { it.kind is ChannelKind.Direct }.map { bareJid(it.jid).lowercase() }.toSet()
    val people = peopleOf(contacts).filter { it.jid.lowercase() !in haveChat }
        .map { FindHit.Person(it.jid, contactName(it)) }
    val all: List<FindHit> = when (query.only) {
        FindQuery.Only.Rooms -> chats.filter { !it.direct } + rooms
        FindQuery.Only.People -> chats.filter { it.direct } + people
        null -> chats + rooms + people + spaces.map { FindHit.Space(it) }
    }
    val known = (homeChannels + spaceChannels).map { bareJid(it.jid).lowercase() }.toSet() +
        contacts.map { it.jid.lowercase() }
    val typed = typedHits(raw, known)
    val found = if (query.text.isEmpty()) all else all
        .map { it to fuzzyScore(query.text, it.title) * (if (it is FindHit.Space) 1.0 else 1.05) }
        .filter { it.second > 0 }
        .sortedByDescending { it.second }
        .map { it.first }
    return (typed + found).take(FIND_LIMIT)
}

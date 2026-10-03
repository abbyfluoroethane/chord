package space.foid.chord.ui.contacts

import space.foid.chord.ui.components.Presence
import space.foid.chord.viewmodel.RequestItem
import uniffi.chord_ffi.Contact
import uniffi.chord_ffi.SubscriptionState

/** The tabs of the contacts page, in the order of the desktop (ContactsHeader.svelte). */
enum class ContactsTab { Online, All, Pending, Blocked, Add }

/** What a row of the contacts page stands for. It decides the actions of the row. */
enum class EntryKind { Contact, Incoming, Outgoing, Blocked }

/** One row of the contacts page. [contact] is null for a request that came in or a blocked stranger. */
data class ContactEntry(val kind: EntryKind, val jid: String, val name: String, val contact: Contact? = null) {
    val key: String get() = "${kind.name}/$jid"
}

/** The name to show: the roster name, or the part of the address before the @. */
fun contactName(c: Contact): String = c.name?.trim()?.takeIf { it.isNotEmpty() } ?: localPart(c.jid)

fun localPart(jid: String): String = jid.substringBefore('/').substringBefore('@')

/**
 * We asked for the subscription and nobody answered. The desktop calls it an outgoing request
 * (adapt.ts, splitRoster).
 */
fun Contact.isOutgoing(): Boolean = ask && (subscription == SubscriptionState.NONE || subscription == SubscriptionState.FROM)

/** The roster without the blocked contacts and without the outgoing requests. */
fun peopleOf(contacts: List<Contact>): List<Contact> = contacts.filter { !it.blocked && !it.isOutgoing() }

/** The contacts that are online now. */
fun onlineOf(contacts: List<Contact>): List<Contact> = peopleOf(contacts).filter { it.online }

/** The presence shape of a roster entry. */
fun Contact.contactPresence(): Presence = space.foid.chord.ui.components.presenceOf(online, show)

/** The text under a contact: the status text of the contact, or none. The presence name is the fallback of the UI. */
fun Contact.statusText(): String? = status?.trim()?.takeIf { it.isNotEmpty() }

private fun byName(a: ContactEntry, b: ContactEntry) = a.name.compareTo(b.name, ignoreCase = true)

/** Does [entry] match what the user typed in the search field? Name and address count. */
fun ContactEntry.matches(query: String): Boolean {
    val q = query.trim()
    if (q.isEmpty()) return true
    return name.contains(q, ignoreCase = true) || jid.contains(q, ignoreCase = true)
}

/**
 * The rows of a tab, sorted by name, as ContactsList.svelte shows them. The Pending tab lists the
 * incoming requests first, then the outgoing ones. [query] filters by name or address. The Add
 * tab has no rows.
 *
 * @param blocked the addresses of the blocklist
 * @param requests the contact requests that wait for an answer
 */
fun contactEntries(
    tab: ContactsTab,
    contacts: List<Contact>,
    blocked: List<String>,
    requests: List<RequestItem>,
    query: String = "",
): List<ContactEntry> {
    fun person(c: Contact) = ContactEntry(EntryKind.Contact, c.jid, contactName(c), c)
    val rows = when (tab) {
        ContactsTab.Online -> onlineOf(contacts).map(::person).sortedWith(::byName)
        ContactsTab.All -> peopleOf(contacts).map(::person).sortedWith(::byName)
        ContactsTab.Pending -> {
            val incoming = requests.map { r ->
                val known = contacts.firstOrNull { it.jid.equals(r.jid, ignoreCase = true) }
                ContactEntry(EntryKind.Incoming, r.jid, known?.let(::contactName) ?: localPart(r.jid), known)
            }.sortedWith(::byName)
            val outgoing = contacts.filter { !it.blocked && it.isOutgoing() }
                .map { ContactEntry(EntryKind.Outgoing, it.jid, contactName(it), it) }.sortedWith(::byName)
            incoming + outgoing
        }
        ContactsTab.Blocked -> blocked.map { jid ->
            val known = contacts.firstOrNull { it.jid.equals(jid, ignoreCase = true) }
            ContactEntry(EntryKind.Blocked, jid, known?.let(::contactName) ?: localPart(jid), known)
        }.sortedWith(::byName)
        ContactsTab.Add -> emptyList()
    }
    return rows.filter { it.matches(query) }
}

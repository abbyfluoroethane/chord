package space.foid.chord.ui.contacts

import uniffi.chord_ffi.Contact
import uniffi.chord_ffi.SubscriptionState

/** Contacts for the tests of the contacts page and the Home list. */
internal object ContactsFixtures {
    fun contact(
        jid: String,
        name: String? = null,
        online: Boolean = false,
        show: String? = null,
        status: String? = null,
        subscription: SubscriptionState = SubscriptionState.BOTH,
        ask: Boolean = false,
        blocked: Boolean = false,
        activity: String? = null,
    ) = Contact(
        jid = jid, name = name, subscription = subscription, ask = ask, groups = emptyList(), approved = false,
        blocked = blocked, online = online, show = show, status = status, idleSince = null, activity = activity,
    )

    val contacts = listOf(
        contact("alice@chord.localhost", "Alice Martin", online = true, status = "Reading the spec"),
        contact("bob@chord.localhost", "Bob", online = true, show = "away"),
        contact("carol@chord.localhost", "Carol Nguyen", online = true, show = "dnd", status = "In a meeting", activity = "Boards of Canada - Roygbiv"),
        contact("dave@chord.localhost", "Dave"),
        contact("erin@other.example", null, online = true),
        contact("mika@chord.localhost", "Mika", subscription = SubscriptionState.NONE, ask = true),
        contact("evil@spam.example", "Spammer", blocked = true),
    )

    val blocked = listOf("evil@spam.example", "stranger@spam.example")
}

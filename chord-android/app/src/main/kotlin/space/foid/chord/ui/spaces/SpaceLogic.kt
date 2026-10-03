package space.foid.chord.ui.spaces

import uniffi.chord_ffi.Contact
import uniffi.chord_ffi.NotificationLevel
import uniffi.chord_ffi.NotificationSetting

// Pure rules of the space menus and dialogs. They hold no UI and call no FFI.

/** The space that a menu or a dialog is about. [key] is `service|node`. */
data class SpaceTarget(val service: String, val node: String, val name: String) {
    val key: String get() = "$service|$node"
}

/** A channel name as the desktop makes it: lowercase letters and digits, joined by single dashes. */
fun channelSlug(name: String): String =
    name.trim().lowercase().replace(Regex("[^a-z0-9]+"), "-").trim('-')

/**
 * The service on which the rooms of a space live. The core has no call for it. So take the
 * service of a room that we know in this space, or else guess it from the pubsub service:
 * `pubsub.example.org` becomes `conference.example.org`. Null when the address of the
 * service has no dot to guess from.
 */
fun roomServiceFor(spaceService: String, knownRooms: List<String>): String? {
    knownRooms.firstOrNull { it.contains('@') }?.let { return it.substringBefore('/').substringAfter('@') }
    if ('.' !in spaceService) return null
    val rest = spaceService.substringAfter('.')
    return "conference.$rest"
}

/** The address of a new room of a space. */
fun newRoomJid(node: String, slug: String, roomService: String): String = "$node-$slug@$roomService"

/** The nick that we use in the rooms of a space: the one that we chose, else the name of the account. */
fun spaceNick(chosen: String?, account: String): String =
    chosen?.trim()?.takeIf { it.isNotEmpty() } ?: account.substringBefore('@').substringBefore('/')

/** The link that opens a space in Chord (XEP-0147 style), as the desktop makes it. */
fun spaceInviteLink(service: String, node: String): String =
    "xmpp:$service?;node=${java.net.URLEncoder.encode(node, "UTF-8").replace("+", "%20")}"

/** The text of an invite that we send in a chat. */
fun inviteText(spaceName: String, link: String): String = "Join $spaceName on Chord: $link"

/** The durations of "Mute", as on the desktop. */
enum class MuteDuration(val millis: Long?) {
    Minutes15(15 * 60_000L),
    Hour1(60 * 60_000L),
    Hours8(8 * 60 * 60_000L),
    Hours24(24 * 60 * 60_000L),
    Forever(null),
}

/** The Unix time in ms at which a mute of [duration] ends, or null for no end. */
fun muteUntil(duration: MuteDuration, now: Long): Long? = duration.millis?.let { now + it }

/** What the mute of a chat is now. */
sealed interface MuteStatus {
    data object Off : MuteStatus

    /** Muted until the level is changed. */
    data object Forever : MuteStatus

    /** Muted until this Unix time in ms. */
    data class Until(val at: Long) : MuteStatus
}

fun muteStatus(setting: NotificationSetting?, now: Long): MuteStatus = when {
    setting == null -> MuteStatus.Off
    setting.level == NotificationLevel.NONE -> MuteStatus.Forever
    setting.muteUntil != null && setting.muteUntil!! > now -> MuteStatus.Until(setting.muteUntil!!)
    else -> MuteStatus.Off
}

/**
 * The level of a space: the level that all its channels share, else "all".
 * A timed mute does not count: it keeps the level.
 */
fun spaceLevel(levels: List<NotificationLevel>): NotificationLevel =
    levels.distinct().singleOrNull() ?: NotificationLevel.ALL

/** The contacts that match [query] in the name or the address. A blank query matches all. */
fun filterContacts(contacts: List<Contact>, query: String): List<Contact> {
    val q = query.trim().lowercase()
    if (q.isEmpty()) return contacts
    return contacts.filter { (it.name ?: "").lowercase().contains(q) || it.jid.lowercase().contains(q) }
}

/** The name to show for a contact. */
fun Contact.shownName(): String = name?.takeIf { it.isNotBlank() } ?: jid.substringBefore('@')

/** Plain label of the affiliation of a member of a space. */
fun affiliationLabel(affiliation: String): String = when (affiliation) {
    "owner" -> "Owner"
    "publisher", "publish-only" -> "Publisher"
    "member" -> "Member"
    "outcast" -> "Banned"
    else -> affiliation
}

/** What the settings page changes, or null for a value that stays. */
data class SettingsChange(val name: String?, val description: String?) {
    val isEmpty: Boolean get() = name == null && description == null
}

/** The change between the saved values and the fields. A blank name is no change. */
fun settingsChange(savedName: String, savedDescription: String, name: String, description: String): SettingsChange =
    SettingsChange(
        name = name.trim().takeIf { it.isNotEmpty() && it != savedName.trim() },
        description = description.trim().takeIf { it != savedDescription.trim() },
    )

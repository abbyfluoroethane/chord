package space.foid.chord.ui.timeline

import java.time.Instant
import java.time.ZoneId

/** Two messages of one sender join a group when they are less than this far apart (as in the core). */
const val GROUP_GAP_MS: Long = 5 * 60 * 1000

/**
 * Is [current] a continuation of [previous]: a row without avatar, name and time?
 *
 * Same rules as the desktop list: the core flag says the sender is the same and the messages
 * are close, and the row breaks off on a new day, at the "new messages" line ([dividerBefore]),
 * and on a reply. The sender and the gap are checked again here so a wrong flag cannot join
 * two people.
 */
fun continuesGroup(
    previous: MessageUi?,
    current: MessageUi,
    dividerBefore: Boolean = false,
    zone: ZoneId = ZoneId.systemDefault(),
): Boolean {
    if (previous == null || dividerBefore) return false
    if (!current.sameSenderAsPrevious) return false
    if (current.reply != null) return false
    if (!samePerson(previous, current)) return false
    val gap = current.timestamp - previous.timestamp
    if (gap < 0 || gap >= GROUP_GAP_MS) return false
    return sameDay(previous.timestamp, current.timestamp, zone)
}

/**
 * The same full address, or, in a 1:1 chat where the resource changes on each reconnect, the
 * same bare address with the same name. In a room the resource is the nick, so two people in
 * one room have different names.
 */
internal fun samePerson(a: MessageUi, b: MessageUi): Boolean =
    a.senderId == b.senderId ||
        (a.senderId.substringBefore('/') == b.senderId.substringBefore('/') && a.senderName == b.senderName)

internal fun sameDay(a: Long, b: Long, zone: ZoneId): Boolean =
    Instant.ofEpochMilli(a).atZone(zone).toLocalDate() == Instant.ofEpochMilli(b).atZone(zone).toLocalDate()

package space.foid.chord.ui.timeline

import space.foid.chord.ui.text.FormatPalette
import space.foid.chord.ui.text.formatMessage
import uniffi.chord_ffi.DeliveryStatus
import uniffi.chord_ffi.TimelineItem
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter

private val CLOCK: DateTimeFormatter = DateTimeFormatter.ofPattern("HH:mm")

private val CLOCK_12: DateTimeFormatter = DateTimeFormatter.ofPattern("h:mm a")

/** "14:05" in [zone], or "2:05 PM" when [is24] is false. */
fun clockLabel(timestampMs: Long, zone: ZoneId = ZoneId.systemDefault(), is24: Boolean = true): String =
    (if (is24) CLOCK else CLOCK_12).format(Instant.ofEpochMilli(timestampMs).atZone(zone))

/** The first line of a quoted body, cut so it stays short. */
fun replySnippet(body: String, max: Int = 140): String {
    val line = body.lineSequence().map { it.trim() }.firstOrNull { it.isNotEmpty() } ?: ""
    return if (line.length > max) line.take(max).trimEnd() + "…" else line
}

/**
 * The names that an `@mention` can use for the own account: the room nick, the local part of the
 * address, the address. The room nick is the sender name of an own message in [items], if any.
 */
fun ownMentionNames(account: String?, items: List<TimelineItem>): List<String> {
    val nick = items.lastOrNull { it.outgoing }?.senderName
    val local = account?.substringBefore('@')
    return listOfNotNull(nick, local, account).filter { it.isNotBlank() }.distinct()
}

/**
 * Pure mapping from the core's [TimelineItem]. It calls no FFI function, so it runs on the JVM
 * without the native library. [pending] is true for a local message the core did not confirm yet.
 */
fun TimelineItem.toMessageUi(
    pending: Boolean = false,
    zone: ZoneId = ZoneId.systemDefault(),
    palette: FormatPalette? = null,
    ownNames: List<String> = emptyList(),
    now: Long = System.currentTimeMillis(),
    account: String? = null,
    /** True in a room: the sender address is room@service/nick there, so it has no foreign domain. */
    inRoom: Boolean = true,
): MessageUi = MessageUi(
    id = id,
    senderId = sender,
    senderName = senderName,
    avatarUrl = avatar,
    body = body,
    timestamp = timestamp,
    timeLabel = clockLabel(timestamp, zone),
    stamp = stampLabel(timestamp, now, zone),
    foreignDomain = if (inRoom) null else foreignDomain(sender, account),
    outgoing = outgoing,
    sameSenderAsPrevious = sameSenderAsPrevious,
    edited = edited,
    retracted = retracted,
    reactions = reactions.map { ReactionUi(it.emoji, it.count.toInt(), it.mine) },
    reply = replyTo?.let { ReplyUi(it.id, it.senderName, replySnippet(it.body)) },
    attachment = attachment,
    state = when {
        status == DeliveryStatus.FAILED -> SendState.FAILED
        pending -> SendState.PENDING
        else -> SendState.SENT
    },
    formatted = if (palette != null && !retracted) formatMessage(body, palette, ownNames, senderName) else null,
)

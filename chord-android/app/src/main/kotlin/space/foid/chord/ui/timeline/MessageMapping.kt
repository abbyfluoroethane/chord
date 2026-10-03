package space.foid.chord.ui.timeline

import uniffi.chord_ffi.DeliveryStatus
import uniffi.chord_ffi.TimelineItem
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter

private val CLOCK: DateTimeFormatter = DateTimeFormatter.ofPattern("HH:mm")

/** "14:05" in [zone]. */
fun clockLabel(timestampMs: Long, zone: ZoneId = ZoneId.systemDefault()): String =
    CLOCK.format(Instant.ofEpochMilli(timestampMs).atZone(zone))

/** The first line of a quoted body, cut so it stays short. */
fun replySnippet(body: String, max: Int = 140): String {
    val line = body.lineSequence().map { it.trim() }.firstOrNull { it.isNotEmpty() } ?: ""
    return if (line.length > max) line.take(max).trimEnd() + "…" else line
}

/**
 * Pure mapping from the core's [TimelineItem]. It calls no FFI function, so it runs on the JVM
 * without the native library. [pending] is true for a local message the core did not confirm yet.
 */
fun TimelineItem.toMessageUi(
    pending: Boolean = false,
    zone: ZoneId = ZoneId.systemDefault(),
): MessageUi = MessageUi(
    id = id,
    senderId = sender,
    senderName = senderName,
    avatarUrl = avatar,
    body = body,
    timestamp = timestamp,
    timeLabel = clockLabel(timestamp, zone),
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
)

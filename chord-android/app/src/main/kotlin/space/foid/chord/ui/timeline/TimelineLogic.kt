package space.foid.chord.ui.timeline

import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.time.temporal.ChronoUnit
import java.util.Locale

/*
 * Pure logic of the timeline chrome: dates, the unread split and the typing line. No Android and
 * no FFI types, so every function runs in a JVM test. The wording follows the desktop (format.ts
 * and MessageList.svelte).
 */

private fun dateOf(ms: Long, zone: ZoneId): LocalDate = Instant.ofEpochMilli(ms).atZone(zone).toLocalDate()

/** Whole days from [ms] to [now]: 0 is today, 1 is yesterday. */
fun dayDiff(ms: Long, now: Long, zone: ZoneId = ZoneId.systemDefault()): Int =
    ChronoUnit.DAYS.between(dateOf(ms, zone), dateOf(now, zone)).toInt()

/** The text of a date separator: "Saturday, October 3, 2026". */
fun dayLabel(ms: Long, zone: ZoneId = ZoneId.systemDefault(), locale: Locale = Locale.getDefault()): String =
    DateTimeFormatter.ofPattern("EEEE, MMMM d, yyyy", locale).format(Instant.ofEpochMilli(ms).atZone(zone))

/** The time next to a sender name: "today 13:24", "yesterday 21:10", "28 Sep 15:04". */
fun stampLabel(
    ms: Long,
    now: Long,
    zone: ZoneId = ZoneId.systemDefault(),
    locale: Locale = Locale.getDefault(),
): String {
    val clock = clockLabel(ms, zone)
    return when (dayDiff(ms, now, zone)) {
        0 -> "today $clock"
        1 -> "yesterday $clock"
        else -> DateTimeFormatter.ofPattern("d MMM", locale).format(Instant.ofEpochMilli(ms).atZone(zone)) + " " + clock
    }
}

/**
 * The domain of a sender that is not on our server, for the "@other.example" suffix. Null for our
 * own server and when there is no domain. Both addresses may carry a resource.
 */
fun foreignDomain(sender: String, account: String?): String? {
    val domain = domainOf(sender) ?: return null
    val own = account?.let(::domainOf) ?: return null
    return if (domain.equals(own, ignoreCase = true)) null else domain
}

private fun domainOf(address: String): String? =
    address.substringBefore('/').substringAfter('@', "").takeIf { it.isNotEmpty() }

/**
 * The id of the first unread message: the [unreadOnOpen]-th incoming message from the end. This is
 * how the desktop places its "new" line (markDivider). [incomingIds] are the ids of the messages
 * of other people, oldest first. Null when nothing is unread.
 */
fun firstUnreadId(incomingIds: List<String>, unreadOnOpen: Int): String? {
    if (unreadOnOpen <= 0 || incomingIds.isEmpty()) return null
    return incomingIds[maxOf(0, incomingIds.size - unreadOnOpen)]
}

/** How many messages from [firstUnreadId] on are unread: the incoming ones that are not deleted. */
fun unreadCount(oldestFirst: List<MessageUi>, firstUnreadId: String?): Int {
    if (firstUnreadId == null) return 0
    val at = oldestFirst.indexOfFirst { it.id == firstUnreadId }
    if (at < 0) return 0
    return oldestFirst.subList(at, oldestFirst.size).count { !it.outgoing && !it.retracted }
}

/** The time of the first unread message, or null if it is not in the list. */
fun unreadSince(oldestFirst: List<MessageUi>, firstUnreadId: String?): Long? =
    oldestFirst.firstOrNull { it.id == firstUnreadId }?.timestamp

/** "13 new messages since 12:14". */
fun unreadBarText(count: Int, sinceClock: String): String =
    "$count new ${if (count == 1) "message" else "messages"} since $sinceClock"

/** "A is typing…", "A and B are typing…", "Several people are typing…". Empty for nobody. */
fun typingText(names: List<String>): String = when (names.size) {
    0 -> ""
    1 -> "${names[0]} is typing…"
    2 -> "${names[0]} and ${names[1]} are typing…"
    else -> "Several people are typing…"
}

/**
 * The names to show for the typers of the core. In a 1:1 chat the core sends bare addresses and
 * the name is the title of the chat ([directName]). In a room the core sends nicks. Blank
 * entries drop out and a name shows once.
 */
fun typerNames(typers: List<String>, directName: String?): List<String> =
    typers
        .map { if (directName != null && '@' in it) directName else it }
        .filter { it.isNotBlank() }
        .distinct()

/** The chrome that one message row carries above it. */
data class RowChrome(
    /** The date separator above the row, or null. */
    val dayLabel: String? = null,
    /** The red "NEW" line above the row. */
    val newDivider: Boolean = false,
)

/** The chrome of each message, oldest first. A date separator shows at the first message and at each new day. */
fun rowChrome(
    oldestFirst: List<MessageUi>,
    firstUnreadId: String?,
    zone: ZoneId = ZoneId.systemDefault(),
    locale: Locale = Locale.getDefault(),
): List<RowChrome> = oldestFirst.mapIndexed { i, m ->
    val newDay = i == 0 || !sameDay(oldestFirst[i - 1].timestamp, m.timestamp, zone)
    RowChrome(
        dayLabel = if (newDay) dayLabel(m.timestamp, zone, locale) else null,
        newDivider = m.id == firstUnreadId,
    )
}

/** What the start-of-history header says. */
data class StartText(val title: String, val subtitle: String)

fun startText(name: String, isRoom: Boolean): StartText =
    if (isRoom) StartText("Welcome to #$name", "This is the start of the channel.")
    else StartText(name, "This is the start of your messages with $name.")

package space.foid.chord.ui.timeline

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import space.foid.chord.ui.screens.buildTimelineRows
import java.time.ZoneId
import java.time.ZonedDateTime
import java.util.Locale

class TimelineLogicTest {
    private val utc = ZoneId.of("UTC")
    private val us = Locale.US
    private fun at(day: Int, h: Int, m: Int, month: Int = 10) =
        ZonedDateTime.of(2026, month, day, h, m, 0, 0, utc).toInstant().toEpochMilli()

    private fun ui(id: String, ts: Long, out: Boolean = false, retracted: Boolean = false, sender: String = "a@x") = MessageUi(
        id = id, senderId = sender, senderName = sender, avatarUrl = null, body = "hi", timestamp = ts,
        timeLabel = "", outgoing = out, sameSenderAsPrevious = true, edited = false, retracted = retracted,
    )

    // Dates

    @Test fun day_label_is_long_and_has_the_weekday() =
        assertEquals("Saturday, October 3, 2026", dayLabel(at(3, 13, 24), utc, us))

    @Test fun stamp_says_today_and_yesterday() {
        val now = at(3, 15, 0)
        assertEquals("today 13:24", stampLabel(at(3, 13, 24), now, utc, us))
        assertEquals("yesterday 21:10", stampLabel(at(2, 21, 10), now, utc, us))
    }

    @Test fun stamp_of_an_older_day_has_the_date() =
        assertEquals("28 Sep 15:04", stampLabel(at(28, 15, 4, month = 9), at(3, 15, 0), utc, us))

    @Test fun day_diff_counts_calendar_days_not_hours() {
        // 23:59 to 00:01 is one day apart.
        assertEquals(1, dayDiff(at(2, 23, 59), at(3, 0, 1), utc))
        assertEquals(0, dayDiff(at(3, 0, 0), at(3, 23, 59), utc))
    }

    @Test fun separator_at_the_first_message_and_at_each_new_day() {
        val list = listOf(ui("1", at(2, 10, 0)), ui("2", at(2, 11, 0)), ui("3", at(3, 9, 0)))
        val chrome = rowChrome(list, null, utc, us)
        assertEquals("Friday, October 2, 2026", chrome[0].dayLabel)
        assertNull(chrome[1].dayLabel)
        assertEquals("Saturday, October 3, 2026", chrome[2].dayLabel)
    }

    // Foreign senders

    @Test fun foreign_domain_shows_for_another_server() =
        assertEquals("other.example", foreignDomain("sam@other.example/phone", "abby@foid.space/dev"))

    @Test fun own_server_shows_no_suffix() {
        assertNull(foreignDomain("rin@foid.space", "abby@foid.space/dev"))
        assertNull(foreignDomain("rin@FOID.space", "abby@foid.space"))
    }

    @Test fun no_account_or_no_domain_shows_no_suffix() {
        assertNull(foreignDomain("sam@other.example", null))
        assertNull(foreignDomain("sam", "abby@foid.space"))
    }

    // Unread split

    @Test fun first_unread_is_the_nth_incoming_from_the_end() {
        val ids = listOf("a", "b", "c", "d")
        assertEquals("c", firstUnreadId(ids, 2))
        assertEquals("a", firstUnreadId(ids, 99))
        assertNull(firstUnreadId(ids, 0))
        assertNull(firstUnreadId(emptyList(), 3))
    }

    @Test fun unread_count_skips_own_and_deleted_messages() {
        val list = listOf(
            ui("1", 1), ui("2", 2), ui("3", 3, out = true), ui("4", 4, retracted = true), ui("5", 5),
        )
        assertEquals(2, unreadCount(list, "2"))
        assertEquals(0, unreadCount(list, null))
        assertEquals(0, unreadCount(list, "gone"))
        assertEquals(2L, unreadSince(list, "2"))
        assertNull(unreadSince(list, "gone"))
    }

    @Test fun unread_bar_text_is_singular_and_plural() {
        assertEquals("1 new message since 12:14", unreadBarText(1, "12:14"))
        assertEquals("13 new messages since 12:14", unreadBarText(13, "12:14"))
    }

    @Test fun the_new_line_is_on_the_first_unread_and_it_breaks_the_group() {
        val list = listOf(ui("1", at(3, 10, 0)), ui("2", at(3, 10, 1)), ui("3", at(3, 10, 2)))
        val rows = buildTimelineRows(list, firstUnreadId = "2", zone = utc, locale = us)
        // Newest first.
        assertEquals(listOf("3", "2", "1"), rows.map { it.message.id })
        assertEquals(listOf(false, true, false), rows.map { it.chrome.newDivider })
        // "2" starts a group below the line. "3" continues it.
        assertEquals(listOf(true, false, false), rows.map { it.grouped })
        // The date separator belongs to the oldest row only.
        assertEquals(listOf(false, false, true), rows.map { it.chrome.dayLabel != null })
    }

    // Typing

    @Test fun typing_text_for_one_two_and_many() {
        assertEquals("", typingText(emptyList()))
        assertEquals("Bay is typing…", typingText(listOf("Bay")))
        assertEquals("Bay and Jo are typing…", typingText(listOf("Bay", "Jo")))
        assertEquals("Several people are typing…", typingText(listOf("Bay", "Jo", "Kit")))
    }

    @Test fun in_a_direct_chat_an_address_becomes_the_chat_name() =
        assertEquals(listOf("Rin"), typerNames(listOf("rin@foid.space"), "Rin"))

    @Test fun in_a_room_nicks_stay_and_blanks_and_repeats_drop() =
        assertEquals(listOf("Bay", "Jo"), typerNames(listOf("Bay", " ", "Jo", "Bay"), null))

    // Start of history

    @Test fun start_text_follows_the_desktop() {
        assertEquals(StartText("Welcome to #general", "This is the start of the channel."), startText("general", true))
        assertEquals(StartText("Jo", "This is the start of your messages with Jo."), startText("Jo", false))
    }

    @Test fun chrome_flags_are_off_without_an_unread_id() {
        val rows = buildTimelineRows(listOf(ui("1", at(3, 10, 0))), zone = utc, locale = us)
        assertFalse(rows.single().chrome.newDivider)
        assertTrue(rows.single().chrome.dayLabel != null)
    }
}

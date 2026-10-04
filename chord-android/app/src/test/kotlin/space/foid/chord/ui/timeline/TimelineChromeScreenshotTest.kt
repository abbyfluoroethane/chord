package space.foid.chord.ui.timeline

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Column
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performClick
import com.github.takahirom.roborazzi.captureRoboImage
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import space.foid.chord.ui.components.Presence
import space.foid.chord.ui.screens.TimelineContent
import space.foid.chord.ui.screens.buildTimelineRows
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordTheme
import java.time.ZoneId
import java.time.ZonedDateTime
import java.util.Locale

@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [36], qualifiers = "w360dp-h740dp-xxhdpi")
class TimelineChromeScreenshotTest {
    @get:Rule val compose = createComposeRule()

    private val utc = ZoneId.of("UTC")
    private fun at(day: Int, h: Int, m: Int) = ZonedDateTime.of(2026, 10, day, h, m, 0, 0, utc).toInstant().toEpochMilli()
    private val topic = "Static fire is Thursday. Keep it calm and keep it short. Ask Jo for the run sheet before you start."

    private fun shot(dark: Boolean, name: String, content: @Composable () -> Unit) {
        compose.setContent { ChordTheme(dark = dark) { Column(Modifier.background(Chord.colors.surface100)) { content() } } }
        compose.onRoot().captureRoboImage("src/test/screenshots/$name.png")
    }

    private val pieces: @Composable () -> Unit = {
        TimelineHeaderContent("general", isRoom = true, onOpenChannels = {}, onOpenMembers = {}, topic = topic)
        TimelineHeaderContent("Rin", isRoom = false, onOpenChannels = {}, onOpenMembers = {}, presence = Presence.Online)
        TimelineHeaderContent("Jo", isRoom = false, onOpenChannels = {}, onOpenMembers = {}, presence = Presence.Dnd)
        UnreadBar("13 new messages since 12:14", onMarkRead = {})
        DateSeparator("Saturday, October 3, 2026")
        NewDivider()
        TypingLine("Bay is typing…")
        JumpToPresent(onClick = {})
        StartOfHistory("general", isRoom = true)
        StartOfHistory("Jo", isRoom = false)
    }

    @Test fun pieces_dark() = shot(true, "timeline_chrome_dark", pieces)
    @Test fun pieces_light() = shot(false, "timeline_chrome_light", pieces)

    private fun m(n: Int, who: String, body: String, ts: Long, sender: String = "$who@chord.test", out: Boolean = false) = MessageUi(
        id = "m:$n", senderId = sender, senderName = who, avatarUrl = null, body = body, timestamp = ts,
        timeLabel = "%02d:%02d".format((ts / 3_600_000 % 24).toInt(), (ts / 60_000 % 60).toInt()),
        stamp = if (ts >= at(3, 0, 0)) "today %02d:%02d".format((ts / 3_600_000 % 24).toInt(), (ts / 60_000 % 60).toInt()) else "yesterday 21:10",
        foreignDomain = if (sender.endsWith("other.example")) "other.example" else null,
        outgoing = out, sameSenderAsPrevious = false, edited = n == 4, retracted = false,
    )

    private val conversation = listOf(
        m(1, "Rin", "Range is booked until noon.", at(2, 21, 10)),
        m(2, "Jo", "Thanks, I will bring the run sheet.", at(3, 9, 30)),
        m(3, "Sam", "Weather looks clear for Thursday.", at(3, 12, 20), sender = "sam@other.example"),
        m(4, "Kit", "Docs say 12 bar max.", at(3, 12, 25)),
        m(5, "Me", "Got it, checking the valve order.", at(3, 12, 30), out = true),
    )

    private fun full(dark: Boolean, name: String, scrolled: Boolean = false) = shot(dark, name) {
        TimelineContent(
            rows = buildTimelineRows(conversation, firstUnreadId = "m:3", zone = utc, locale = Locale.US),
            title = "general",
            topic = "Static fire is Thursday. Keep it calm.",
            unreadBar = "2 new messages since 12:20",
            typing = "Bay and Jo are typing…",
            reachedStart = true,
        )
    }

    @Test fun full_dark() = full(true, "timeline_full_dark")
    @Test fun full_light() = full(false, "timeline_full_light")

    @Test fun empty_dm_dark() = shot(true, "timeline_empty_dm_dark") {
        TimelineContent(rows = emptyList(), title = "Jo", isRoom = false, presence = Presence.Away)
    }
    @Test fun empty_dm_light() = shot(false, "timeline_empty_dm_light") {
        TimelineContent(rows = emptyList(), title = "Jo", isRoom = false, presence = Presence.Away)
    }

    @Test
    fun topicOpensOnTapAndFoldsAgain() {
        compose.setContent {
            ChordTheme(dark = true) {
                TimelineHeaderContent("general", isRoom = true, onOpenChannels = {}, onOpenMembers = {}, topic = topic)
            }
        }
        val before = compose.onNodeWithTag("header_topic").fetchSemanticsNode().size.height
        compose.onNodeWithTag("header_topic").performClick()
        compose.waitForIdle()
        val open = compose.onNodeWithTag("header_topic").fetchSemanticsNode().size.height
        assert(open > before) { "the topic did not grow: $before -> $open" }
        compose.onNodeWithTag("header_topic").performClick()
        compose.waitForIdle()
        assertEquals(before, compose.onNodeWithTag("header_topic").fetchSemanticsNode().size.height)
    }

    @Test
    fun directChatHasAtGlyphAndRoomHasNoDot() {
        compose.setContent {
            ChordTheme(dark = true) {
                TimelineHeaderContent("Rin", isRoom = false, onOpenChannels = {}, onOpenMembers = {}, presence = Presence.Online)
            }
        }
        compose.onNodeWithTag("header_glyph").assertTextEquals("@")
        compose.onAllNodesWithTag("header_presence").assertCountEquals(1)
        compose.onNodeWithContentDescription("Profile").assertExists()
    }

    @Test
    fun roomHasHashAndNoPresenceDot() {
        compose.setContent {
            ChordTheme(dark = true) {
                TimelineHeaderContent("general", isRoom = true, onOpenChannels = {}, onOpenMembers = {}, presence = Presence.Online)
            }
        }
        compose.onNodeWithTag("header_glyph").assertTextEquals("#")
        compose.onAllNodesWithTag("header_presence").assertCountEquals(0)
    }

    @Test
    fun markAsReadCallsBack() {
        var taps = 0
        compose.setContent { ChordTheme(dark = true) { UnreadBar("1 new message since 12:14", onMarkRead = { taps++ }) } }
        compose.onNodeWithTag("unread_bar_mark_read").performClick()
        assertEquals(1, taps)
    }
}

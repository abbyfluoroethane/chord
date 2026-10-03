package space.foid.chord.ui.screens

import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performScrollToIndex
import com.github.takahirom.roborazzi.captureRoboImage
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import space.foid.chord.ui.theme.ChordTheme
import space.foid.chord.ui.timeline.MessageUi
import space.foid.chord.ui.timeline.ReactionUi
import space.foid.chord.ui.timeline.ReplyUi

@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [36], qualifiers = "w360dp-h740dp-xxhdpi")
class TimelineScreenTest {
    @get:Rule val compose = createComposeRule()

    private val t0 = 1_760_000_000_000L
    private val min = 60_000L

    private fun m(
        n: Int, who: String, body: String, at: Long, same: Boolean = false, outgoing: Boolean = false,
        reply: ReplyUi? = null, reactions: List<ReactionUi> = emptyList(),
    ) = MessageUi(
        id = "m:$n", senderId = "$who@chord.test", senderName = who, avatarUrl = null, body = body,
        timestamp = at, timeLabel = "%02d:%02d".format(9 + (at - t0).toInt() / 3_600_000, ((at - t0) / min % 60).toInt()),
        outgoing = outgoing, sameSenderAsPrevious = same, edited = false, retracted = false,
        reactions = reactions, reply = reply,
    )

    private fun conversation(): List<MessageUi> = listOf(
        m(1, "Alice", "Morning. Did the build finish?", t0),
        m(2, "Alice", "I need it before lunch.", t0 + 1 * min, same = true),
        m(3, "Bob", "It did, about a minute ago.", t0 + 3 * min, reactions = listOf(ReactionUi("+1", 3, true), ReactionUi("ok", 1, false))),
        m(4, "Bob", "The new release is on the staging server.", t0 + 3 * min + 20_000, same = true),
        m(5, "Me", "Great, thanks. Checking it now.", t0 + 9 * min, outgoing = true),
        m(6, "Alice", "Found one small bug in the login form.", t0 + 20 * min, reply = ReplyUi("m:5", "Me", "Great, thanks. Checking it now.")),
        m(7, "Alice", "A long message that must wrap over several lines on a narrow phone screen, so we can see how the text flows.", t0 + 21 * min, same = true),
        m(8, "Me", "On it.", t0 + 25 * min, outgoing = true),
    )

    private fun rows(list: List<MessageUi>) = buildTimelineRows(list)

    private fun shot(dark: Boolean, name: String, content: @Composable () -> Unit) {
        compose.setContent { ChordTheme(dark = dark) { content() } }
        compose.onRoot().captureRoboImage("src/test/screenshots/$name.png")
    }


    @Test fun conversation_dark() = shot(true, "timeline_dark") {
        TimelineContent(rows(conversation()), "general", reachedStart = true, composerText = "")
    }

    @Test fun conversation_light() = shot(false, "timeline_light") {
        TimelineContent(rows(conversation()), "general", reachedStart = true)
    }

    @Test fun loading_older_dark() = shot(true, "timeline_loading_older_dark") {
        TimelineContent(rows(conversation().take(4)), "general", loadingOlder = true)
    }

    @Test fun loading_older_light() = shot(false, "timeline_loading_older_light") {
        TimelineContent(rows(conversation().take(4)), "general", loadingOlder = true)
    }

    @Test fun beginning_light() = shot(false, "timeline_beginning_light") {
        TimelineContent(rows(conversation().take(3)), "general", reachedStart = true)
    }

    @Test fun empty_dark() = shot(true, "timeline_empty_dark") {
        TimelineContent(emptyList(), "general")
    }

    @Test fun empty_light() = shot(false, "timeline_empty_light") {
        TimelineContent(emptyList(), "general")
    }

    @Test fun reply_mode_dark() = shot(true, "timeline_reply_dark") {
        TimelineContent(rows(conversation()), "general", composerText = "Thanks, fixing it", replyingTo = "Alice")
    }

    @Test fun reply_mode_light() = shot(false, "timeline_reply_light") {
        TimelineContent(rows(conversation()), "general", composerText = "Thanks, fixing it", replyingTo = "Alice")
    }

    @Test fun private_chat_error_dark() = shot(true, "timeline_private_error_dark") {
        TimelineContent(rows(conversation().take(4)), "Alice", isRoom = false, error = "The message could not be sent.")
    }

    @Test fun rows_group_and_run_newest_first() {
        val r = rows(conversation())
        assertEquals("m:8", r.first().message.id)
        assertEquals("m:1", r.last().message.id)
        assertTrue(r.first { it.message.id == "m:2" }.grouped)
        assertTrue(!r.first { it.message.id == "m:6" }.grouped) // a reply starts a group
    }

    // Performance: 5,000 messages compose and scroll; a new message does not recompose
    // the rows that were already visible.
    @Test fun five_thousand_messages_scroll_and_append() {
        val big = (1..5000).map { i ->
            m(i, if (i % 7 < 3) "Alice" else "Bob", "Message number $i with a little text in it.", t0 + i * 20_000L, same = i % 7 != 0 && i % 7 != 3)
        }
        var list by mutableStateOf(rows(big))
        val composed = HashMap<String, Int>()
        compose.setContent {
            ChordTheme(dark = true) {
                TimelineContent(
                    list, "general", listState = rememberLazyListState(),
                    onRowComposed = { composed.merge(it, 1, Int::plus) },
                )
            }
        }
        compose.waitForIdle()
        assertTrue("rows are on screen", composed.isNotEmpty())

        val visible = composed.keys.toSet()
        composed.clear()
        val newId = "m:5001"
        val start = System.nanoTime()
        compose.runOnIdle {
            list = rows(big + m(5001, "Alice", "A new message arrives.", t0 + 5001 * 20_000L))
        }
        compose.waitForIdle()
        val ms = (System.nanoTime() - start) / 1_000_000
        println("PERF append to 5000: idle after $ms ms; recompositions: $composed")
        assertEquals("the new row composed once", 1, composed[newId])
        val again = composed.filterKeys { it != newId && it in visible }
        assertTrue("visible rows recomposed: $again", again.isEmpty())

        val t1 = System.nanoTime()
        compose.onNodeWithTag("timeline").performScrollToIndex(2500)
        compose.waitForIdle()
        compose.onNodeWithTag("timeline").performScrollToIndex(4900)
        compose.waitForIdle()
        println("PERF scroll 5000: ${(System.nanoTime() - t1) / 1_000_000} ms")
    }
}

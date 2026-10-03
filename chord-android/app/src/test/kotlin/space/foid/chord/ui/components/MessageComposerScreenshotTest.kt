package space.foid.chord.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onRoot
import com.github.takahirom.roborazzi.captureRoboImage
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordSize
import space.foid.chord.ui.theme.ChordTheme
import space.foid.chord.ui.timeline.MessageUi
import space.foid.chord.ui.timeline.ReactionUi
import space.foid.chord.ui.timeline.ReplyUi
import space.foid.chord.ui.timeline.SendState

@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [36], qualifiers = "w360dp-h900dp-xxhdpi")
class MessageComposerScreenshotTest {
    @get:Rule val compose = createComposeRule()

    private fun msg(
        body: String = "Morning. Did the build finish?",
        name: String = "Alice",
        outgoing: Boolean = false,
        edited: Boolean = false,
        retracted: Boolean = false,
        reply: ReplyUi? = null,
        reactions: List<ReactionUi> = emptyList(),
        state: SendState = SendState.SENT,
    ) = MessageUi(
        id = "m:1", senderId = name, senderName = name, avatarUrl = null, body = body, timestamp = 0,
        timeLabel = "14:05", outgoing = outgoing, sameSenderAsPrevious = false, edited = edited,
        retracted = retracted, reactions = reactions, reply = reply, state = state,
    )

    @Composable private fun Placeholder() {
        Box(Modifier.size(ChordSize.avatar).background(Chord.colors.accent, CircleShape))
    }

    @Composable private fun Rows() {
        Column(Modifier.background(Chord.colors.surface100)) {
            MessageRow(msg(), grouped = false, avatar = { Placeholder() })
            MessageRow(msg(body = "It did, about a minute ago."), grouped = true, avatar = { Placeholder() })
            MessageRow(
                msg(name = "Bob", reply = ReplyUi("m:0", "Alice", "Did the build finish? I need to know before lunch")),
                grouped = false, avatar = { Placeholder() },
            )
            MessageRow(
                msg(
                    name = "Me", outgoing = true, edited = true, body = "Fixed a typo.",
                    reactions = listOf(ReactionUi("+1", 3, true), ReactionUi("ok", 1, false)),
                ),
                grouped = false, avatar = { Placeholder() },
            )
            MessageRow(msg(name = "Carol", retracted = true), grouped = false, avatar = { Placeholder() })
            MessageRow(msg(name = "Me", outgoing = true, body = "Sending this one.", state = SendState.PENDING), grouped = false, avatar = { Placeholder() })
            MessageRow(msg(name = "Me", outgoing = true, body = "This one failed.", state = SendState.FAILED), grouped = false, avatar = { Placeholder() })
            MessageRow(
                msg(name = "Dave", body = "A long message that must wrap over several lines on a narrow phone screen, so we can see how the body text flows next to the avatar column and keeps its margins. ".repeat(2)),
                grouped = false, avatar = { Placeholder() },
            )
        }
    }

    @Composable private fun Composers() {
        Column(Modifier.background(Chord.colors.surface100)) {
            ComposerBar(text = "", onTextChange = {}, onSend = {}, placeholder = "Message #general")
            ComposerBar(text = "Looks good to me", onTextChange = {}, onSend = {})
            ComposerBar(text = "Agreed", onTextChange = {}, onSend = {}, replyingTo = "Alice")
            ComposerBar(text = "Fixed a typo.", onTextChange = {}, onSend = {}, editing = true)
        }
    }

    private fun shot(dark: Boolean, name: String, content: @Composable () -> Unit) {
        compose.setContent { ChordTheme(dark = dark) { content() } }
        compose.onRoot().captureRoboImage("src/test/screenshots/$name.png")
    }

    @Test fun message_rows_dark() = shot(true, "message_rows_dark") { Rows() }
    @Test fun message_rows_light() = shot(false, "message_rows_light") { Rows() }
    @Test fun composer_dark() = shot(true, "composer_dark") { Composers() }
    @Test fun composer_light() = shot(false, "composer_light") { Composers() }
}

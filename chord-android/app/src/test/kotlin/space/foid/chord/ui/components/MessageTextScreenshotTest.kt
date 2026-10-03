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
import space.foid.chord.ui.text.formatMessage
import space.foid.chord.ui.text.formatPalette
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordSize
import space.foid.chord.ui.theme.ChordTheme
import space.foid.chord.ui.timeline.MessageUi
import space.foid.chord.ui.timeline.ReplyUi

@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [36], qualifiers = "w360dp-h1100dp-xxhdpi")
class MessageTextScreenshotTest {
    @get:Rule val compose = createComposeRule()

    @Composable private fun Placeholder() {
        Box(Modifier.size(ChordSize.avatar).background(Chord.colors.accent, CircleShape))
    }

    @Composable private fun Row(
        body: String,
        name: String = "Alice",
        reply: ReplyUi? = null,
        edited: Boolean = false,
        grouped: Boolean = false,
    ) {
        val palette = Chord.colors.formatPalette()
        val m = MessageUi(
            id = "m:$body", senderId = name, senderName = name, avatarUrl = null, body = body, timestamp = 0,
            timeLabel = "14:05", outgoing = false, sameSenderAsPrevious = false, edited = edited,
            retracted = false, reply = reply,
            formatted = formatMessage(body, palette, listOf("abby"), name),
        )
        MessageRow(m, grouped = grouped, avatar = { Placeholder() })
    }

    @Composable private fun Rows() {
        Column(Modifier.background(Chord.colors.surface100)) {
            Row("Docs are at https://example.com/docs/chord?page=2, and the room is xmpp:chord@conference.example.org?join.")
            Row("This is **bold**, this is *italic*, this is __underlined__, this is ~~struck~~ and ***all of it***.", name = "Bob")
            Row("Hey @abby, can you look at this? Not @abbyx though.", name = "Carol")
            Row("> The build is red again\nIt passes on my machine.", name = "Dave")
            Row("Try this:\n```kotlin\nval x = **1**\nprintln(x)\n```\nand `inline code` too.", name = "Erin")
            Row("😀👍🎉", name = "Frank")
            Row("/me waves at https://example.com", name = "Grace")
            Row("# Heading\n- one\n- two\n1. first\n2. second", name = "Heidi", edited = true)
            Row("Fixed it.", name = "Ivan", reply = ReplyUi(null, "Judy", ""))
            Row("Agreed.", name = "Ken", reply = ReplyUi("m:1", "Judy", "The build is red again"))
        }
    }

    private fun shot(dark: Boolean, name: String) {
        compose.setContent { ChordTheme(dark = dark) { Rows() } }
        compose.onRoot().captureRoboImage("src/test/screenshots/$name.png")
    }

    @Test fun message_text_dark() = shot(true, "message_text_dark")
    @Test fun message_text_light() = shot(false, "message_text_light")
}

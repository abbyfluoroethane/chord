package space.foid.chord.ui.components

import androidx.compose.foundation.layout.Box
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.platform.UriHandler
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.longClick
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.click
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import space.foid.chord.ui.theme.ChordTheme
import space.foid.chord.ui.timeline.MessageUi

/** Link taps and the row long press must not fight each other. */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [36], qualifiers = "w360dp-h640dp-xxhdpi")
class MessageLinkTapTest {
    @get:Rule val compose = createComposeRule()

    private val opened = ArrayList<String>()
    private val xmpp = ArrayList<String>()
    private var longPresses = 0

    private fun show(body: String) {
        val handler = object : UriHandler {
            override fun openUri(uri: String) { opened.add(uri) }
        }
        val m = MessageUi(
            id = "m:1", senderId = "a", senderName = "Alice", avatarUrl = null, body = body, timestamp = 0,
            timeLabel = "14:05", outgoing = false, sameSenderAsPrevious = false, edited = false, retracted = false,
        )
        compose.setContent {
            ChordTheme(dark = true) {
                CompositionLocalProvider(LocalUriHandler provides handler) {
                    Box {
                        MessageRow(
                            m, grouped = true, avatar = {},
                            modifier = Modifier.testTag("row"),
                            onLongPress = { longPresses++ },
                            onXmppLink = { xmpp.add(it) },
                        )
                    }
                }
            }
        }
    }

    // A grouped row: the text starts after the 40dp gutter, 12dp gap and 16dp padding.
    private val onText = Offset(300f, 30f)

    @Test fun tapOnHttpLinkOpensIt() {
        show("https://example.com/x")
        compose.onNodeWithTag("row").performTouchInput { click(onText) }
        compose.waitForIdle()
        assertEquals(listOf("https://example.com/x"), opened)
        assertEquals(0, longPresses)
    }

    @Test fun tapOnXmppLinkCallsTheCallback() {
        show("xmpp:room@example.org?join")
        compose.onNodeWithTag("row").performTouchInput { click(onText) }
        compose.waitForIdle()
        assertEquals(listOf("xmpp:room@example.org?join"), xmpp)
        assertEquals(emptyList<String>(), opened)
    }

    @Test fun longPressOnALinkOpensTheActions() {
        show("https://example.com/x")
        compose.onNodeWithTag("row").performTouchInput { longClick(onText) }
        compose.waitForIdle()
        assertEquals(1, longPresses)
        assertEquals(emptyList<String>(), opened)
    }

    @Test fun longPressOnPlainTextOpensTheActions() {
        show("just some text")
        compose.onNodeWithTag("row").performTouchInput { longClick(onText) }
        compose.waitForIdle()
        assertEquals(1, longPresses)
    }
}

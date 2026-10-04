package space.foid.chord.ui.sheets

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.unit.dp
import com.github.takahirom.roborazzi.captureRoboImage
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import space.foid.chord.ui.composer.ForwardContent
import space.foid.chord.ui.composer.ForwardTarget
import space.foid.chord.ui.composer.linksIn
import space.foid.chord.ui.composer.messageActions
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordTheme
import space.foid.chord.ui.timeline.MessageUi

// The sheet window is not drawn here: the tests draw the content in a frame styled like the sheet.
// Robolectric draws emoji as empty boxes (no colour emoji font), so the emoji cells look like tofu.
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [36], qualifiers = "w360dp-h640dp-xxhdpi")
class SheetsScreenshotTest {
    @get:Rule val compose = createComposeRule()

    private val message = MessageUi(
        id = "m:1", senderId = "alice", senderName = "Alice", avatarUrl = null,
        body = "Morning. Did the build finish? I need to know before lunch, because the release depends on it and Bob is waiting.",
        timestamp = 0, timeLabel = "14:05", outgoing = false, sameSenderAsPrevious = false, edited = false, retracted = false,
    )

    private val groups = listOf(
        EmojiGroup("smileys", "Smileys", (0x1F600..0x1F64F).map { EmojiEntry(String(Character.toChars(it)), "e$it", "e$it") }),
        EmojiGroup("nature", "Nature", (0x1F400..0x1F43F).map { EmojiEntry(String(Character.toChars(it)), "n$it", "n$it") }),
        EmojiGroup("food", "Food", (0x1F345..0x1F37F).map { EmojiEntry(String(Character.toChars(it)), "f$it", "f$it") }),
    )

    private fun shot(dark: Boolean, name: String, content: @Composable () -> Unit) {
        compose.setContent {
            ChordTheme(dark = dark) {
                // The scrim over a page, and the sheet at the bottom.
                Box(Modifier.fillMaxSize().background(Chord.colors.surface100)) {
                    Box(Modifier.fillMaxSize().background(Chord.colors.scrim))
                    Box(Modifier.align(Alignment.BottomCenter)) { content() }
                }
            }
        }
        compose.onRoot().captureRoboImage("src/test/screenshots/sheets/$name.png")
    }

    private val quick = listOf("\uD83D\uDC4D", "\u2764\uFE0F", "\uD83D\uDE02", "\uD83D\uDC40")

    @Composable private fun Actions(
        m: MessageUi = message,
        moderator: Boolean = false,
        links: List<String> = emptyList(),
        channelLink: Boolean = true,
    ) = SheetFrame {
        MessageActionsContent(
            message = m,
            actions = messageActions(m, moderator, channelLink),
            quick = quick,
            onAction = {}, onReact = {}, onMoreReactions = {},
            links = links,
        )
    }

    private val own = message.copy(outgoing = true, body = "Fixed. See https://example.org/build/42 for the log.")

    @Composable private fun Picker(query: String = "") = SheetFrame {
        ReactionPickerContent(groups, recents = listOf("😀", "😂", "👍"), onPick = {}, modifier = Modifier.height(440.dp), initialQuery = query)
    }

    @Composable private fun Attach() = SheetFrame { AttachmentContent({}, {}) }

    @Composable private fun Confirm(remove: Boolean = false) = Box(Modifier.fillMaxWidth().padding(24.dp)) { DeleteConfirmCard(message, remove, {}, {}) }

    @Composable private fun Forward(query: String = "", selected: String? = "bob@example.org") = SheetFrame {
        ForwardContent(
            senderName = "Alice", summary = message.body,
            targets = listOf(
                ForwardTarget("bob@example.org", "Bob", "Message", true),
                ForwardTarget("general@conf.example.org", "general", "Channel", false),
                ForwardTarget("ops@conf.example.org", "ops", "Launch Ops", false),
                ForwardTarget("carol@example.org", "Carol", "Message", true),
            ),
            onCancel = {}, onForward = {},
            modifier = Modifier.height(520.dp),
            initialQuery = query, initialSelected = selected,
        )
    }

    @Test fun actions_dark() = shot(true, "actions_dark") { Actions(own, links = linksIn(own.body)) }
    @Test fun actions_light() = shot(false, "actions_light") { Actions(own, links = linksIn(own.body)) }
    @Test fun actions_other_dark() = shot(true, "actions_other_dark") { Actions() }
    @Test fun actions_other_light() = shot(false, "actions_other_light") { Actions() }
    @Test fun actions_moderator_dark() = shot(true, "actions_moderator_dark") { Actions(moderator = true) }
    @Test fun actions_deleted_dark() = shot(true, "actions_deleted_dark") { Actions(message.copy(retracted = true)) }
    @Test fun forward_dark() = shot(true, "forward_dark") { Forward() }
    @Test fun forward_light() = shot(false, "forward_light") { Forward() }
    @Test fun forward_search_dark() = shot(true, "forward_search_dark") { Forward(query = "zz", selected = null) }
    @Test fun remove_dark() = shot(true, "remove_dark") { Confirm(remove = true) }
    @Test fun picker_dark() = shot(true, "picker_dark") { Picker() }
    @Test fun picker_light() = shot(false, "picker_light") { Picker() }
    @Test fun picker_search_dark() = shot(true, "picker_search_dark") { Picker(query = "e1f60") }
    @Test fun attach_dark() = shot(true, "attach_dark") { Attach() }
    @Test fun attach_light() = shot(false, "attach_light") { Attach() }
    @Test fun confirm_dark() = shot(true, "confirm_dark") { Confirm() }
    @Test fun confirm_light() = shot(false, "confirm_light") { Confirm() }
}

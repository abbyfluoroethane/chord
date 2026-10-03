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

    @Composable private fun Actions(canEdit: Boolean = true, canRetract: Boolean = true) = SheetFrame {
        MessageActionsContent(message, canEdit, canRetract, {}, {}, {}, {}, {}, {})
    }

    @Composable private fun Picker(query: String = "") = SheetFrame {
        ReactionPickerContent(groups, recents = listOf("😀", "😂", "👍"), onPick = {}, modifier = Modifier.height(440.dp), initialQuery = query)
    }

    @Composable private fun Attach() = SheetFrame { AttachmentContent({}, {}) }

    @Composable private fun Confirm() = Box(Modifier.fillMaxWidth().padding(24.dp)) { DeleteConfirmCard({}, {}) }

    @Test fun actions_dark() = shot(true, "actions_dark") { Actions() }
    @Test fun actions_light() = shot(false, "actions_light") { Actions() }
    @Test fun actions_other_dark() = shot(true, "actions_other_dark") { Actions(canEdit = false, canRetract = false) }
    @Test fun picker_dark() = shot(true, "picker_dark") { Picker() }
    @Test fun picker_light() = shot(false, "picker_light") { Picker() }
    @Test fun picker_search_dark() = shot(true, "picker_search_dark") { Picker(query = "e1f60") }
    @Test fun attach_dark() = shot(true, "attach_dark") { Attach() }
    @Test fun attach_light() = shot(false, "attach_light") { Attach() }
    @Test fun confirm_dark() = shot(true, "confirm_dark") { Confirm() }
    @Test fun confirm_light() = shot(false, "confirm_light") { Confirm() }
}

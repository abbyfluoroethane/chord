package space.foid.chord.ui.composer

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Column
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
import space.foid.chord.ui.attachments.PendingUploads
import space.foid.chord.ui.components.ComposerBar
import space.foid.chord.ui.sheets.EmojiEntry
import space.foid.chord.ui.sheets.EmojiGroup
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordTheme
import space.foid.chord.viewmodel.UploadStage
import space.foid.chord.viewmodel.UploadUi

@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [36], qualifiers = "w360dp-h640dp-xxhdpi")
class ComposerScreenshotTest {
    @get:Rule val compose = createComposeRule()

    private val nicks = listOf("Rin", "Ricky", "Karin", "Sam")
    private val index = ShortcodeIndex(
        listOf(
            EmojiGroup(
                "smileys", "Smileys",
                listOf(
                    EmojiEntry("🔥", "fire", "fire flame"),
                    EmojiEntry("🐟", "fish", "fish"),
                    EmojiEntry("✊", "raised fist", "fist"),
                    EmojiEntry("🎆", "fireworks", "fireworks"),
                ),
            ),
        ),
    )

    private fun shot(dark: Boolean, name: String, content: @Composable () -> Unit) {
        compose.setContent { ChordTheme(dark = dark) { Column(Modifier.background(Chord.colors.surface100)) { content() } } }
        compose.onRoot().captureRoboImage("src/test/screenshots/composer/$name.png")
    }

    @Composable private fun Suggestions() {
        ComposerBar(text = "hey @ri", onTextChange = {}, onSend = {}, placeholder = "Message #general", mentionNicks = nicks, shortcodeIndex = index)
    }

    @Composable private fun Emoji() {
        ComposerBar(text = "nice :fi", onTextChange = {}, onSend = {}, placeholder = "Message #general", shortcodeIndex = index)
    }

    @Composable private fun States() {
        ComposerBar(text = "", onTextChange = {}, onSend = {}, placeholder = "Message #general", shortcodeIndex = index)
        ComposerBar(text = "", onTextChange = {}, onSend = {}, placeholder = "Message Rin", shortcodeIndex = index)
        ComposerBar(text = "On it", onTextChange = {}, onSend = {}, replyingTo = "Rin", shortcodeIndex = index)
        ComposerBar(text = "Fixed a typo.", onTextChange = {}, onSend = {}, editing = true, shortcodeIndex = index)
    }

    @Composable private fun Tray() {
        PendingUploads(
            listOf(
                UploadUi(1, "checklist.pdf", UploadStage.UPLOADING),
                UploadUi(2, "a-very-long-photo-name-2026.jpg", UploadStage.FAILED, "Too large (max 10 MB)"),
                UploadUi(3, "notes.txt", UploadStage.PREPARING),
            ),
            onRetry = {}, onDismiss = {},
        )
        ComposerBar(text = "", onTextChange = {}, onSend = {}, placeholder = "Message #general", shortcodeIndex = index)
    }

    @Test fun mention_dark() = shot(true, "mention_dark") { Suggestions() }
    @Test fun mention_light() = shot(false, "mention_light") { Suggestions() }
    @Test fun shortcode_dark() = shot(true, "shortcode_dark") { Emoji() }
    @Test fun shortcode_light() = shot(false, "shortcode_light") { Emoji() }
    @Test fun states_dark() = shot(true, "states_dark") { States() }
    @Test fun states_light() = shot(false, "states_light") { States() }
    @Test fun tray_dark() = shot(true, "tray_dark") { Tray() }
    @Test fun tray_light() = shot(false, "tray_light") { Tray() }
}

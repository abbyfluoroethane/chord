package space.foid.chord.ui.theme

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.unit.dp
import com.github.takahirom.roborazzi.captureRoboImage
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import space.foid.chord.ui.components.MessageRow
import space.foid.chord.ui.emoji.EmojiImages
import space.foid.chord.ui.emoji.EmojiPack
import space.foid.chord.ui.emoji.LocalEmojiImages
import space.foid.chord.ui.emoji.PackIndex
import space.foid.chord.ui.emoji.installPack
import space.foid.chord.ui.text.formatMessage
import space.foid.chord.ui.text.formatPalette
import space.foid.chord.ui.timeline.MessageUi
import space.foid.chord.ui.timeline.ReactionUi
import java.io.File

/** A short timeline under each bundled theme, and with an image emoji pack. */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [36], qualifiers = "w360dp-h560dp-xxhdpi")
class ThemedTimelineScreenshotTest {
    @get:Rule val compose = createComposeRule()
    @get:Rule val tmp = TemporaryFolder()

    @Composable private fun Rows() {
        val c = Chord.colors
        val palette = c.formatPalette()
        fun message(name: String, body: String, reactions: List<ReactionUi> = emptyList(), outgoing: Boolean = false) = MessageUi(
            id = "m:$body", senderId = name, senderName = name, avatarUrl = null, body = body, timestamp = 0,
            timeLabel = "14:05", stamp = "today 14:05", outgoing = outgoing, sameSenderAsPrevious = false, edited = false,
            retracted = false, reactions = reactions,
            formatted = formatMessage(body, palette, listOf("abby"), name),
        )
        Column(Modifier.background(c.surface100)) {
            listOf(
                message("Rin", "Static fire moved to 14:00 tomorrow. Bring the checklist 🚀 and see https://chord.example/notes"),
                message(
                    "Mika", "Hey @abby, it works 🎉",
                    reactions = listOf(ReactionUi("👍", 3, true), ReactionUi("❤️", 1, false), ReactionUi("😂", 2, false)),
                ),
                message("Mika", "😀🚀🎉"),
                message("Abby", "Great, thanks! `code` and **bold**", outgoing = true),
            ).forEach { m -> MessageRow(m, grouped = false, avatar = { Box(Modifier.size(40.dp).background(c.accent, CircleShape)) }) }
        }
    }

    private fun shot(name: String, id: String, accent: String? = null, images: EmojiImages? = null) {
        val entry = TestThemes.entry(id)
        compose.setContent {
            ChordTheme(dark = entry.info.dark, colors = entry.info.colors(accent ?: entry.info.defaultAccent)) {
                CompositionLocalProvider(LocalEmojiImages provides images) { Rows() }
            }
        }
        compose.onRoot().captureRoboImage("src/test/screenshots/$name.png")
    }

    @Test fun chord_dark() = shot("timeline_theme_chord_dark", "chord-dark")
    @Test fun chord_light() = shot("timeline_theme_chord_light", "chord-light")
    @Test fun mocha() = shot("timeline_theme_catppuccin_mocha", "catppuccin-mocha")
    @Test fun mocha_peach() = shot("timeline_theme_catppuccin_mocha_peach", "catppuccin-mocha", accent = "peach")
    @Test fun latte() = shot("timeline_theme_catppuccin_latte", "catppuccin-latte")

    @Test fun twemoji_dark() {
        val target = File(tmp.newFolder(), "twemoji")
        installPack(File("../../chord-desktop/src-tauri/resources/twemoji-1.2.5.tgz").inputStream(), target, null)
        val images = EmojiImages(EmojiPack.Twemoji, PackIndex(target), px = 96)
        // The bitmaps are ready before the first frame, so the screenshot shows them.
        listOf("🚀", "🎉", "👍", "❤️", "😂", "😀").forEach { images.loadNow(it) }
        shot("timeline_emoji_twemoji_dark", "chord-dark", images = images)
    }
}

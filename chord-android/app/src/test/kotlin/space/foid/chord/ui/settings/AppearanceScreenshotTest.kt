package space.foid.chord.ui.settings

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
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
import space.foid.chord.ui.emoji.EmojiImages
import space.foid.chord.ui.emoji.EmojiPack
import space.foid.chord.ui.emoji.EmojiPackStatus
import space.foid.chord.ui.emoji.PackIndex
import space.foid.chord.ui.emoji.installPack
import space.foid.chord.ui.theme.Appearance
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordTheme
import space.foid.chord.ui.theme.CustomTheme
import space.foid.chord.ui.theme.ImportError
import space.foid.chord.ui.theme.MotionMode
import space.foid.chord.ui.theme.TestThemes
import space.foid.chord.ui.theme.ThemeEntry
import space.foid.chord.ui.theme.TimeFormat
import space.foid.chord.ui.text.emojiRuns
import java.io.File

@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [36], qualifiers = "w360dp-h2900dp-xxhdpi")
class AppearanceScreenshotTest {
    @get:Rule val compose = createComposeRule()
    @get:Rule val tmp = TemporaryFolder()

    private val library: List<ThemeEntry> = TestThemes.bundled() + ThemeEntry(
        "custom-1",
        "/**\n * @name Nord Night\n * @author Someone\n * @mode dark\n */\n:root { --surface-100: #2e3440; --surface-200: #3b4252; --ink: #eceff4; --brand: #88c0d0; }",
        builtIn = false,
        url = "https://raw.githubusercontent.com/someone/nord/main/theme.css",
    )

    private fun twemoji(): EmojiImages {
        val target = File(tmp.newFolder(), "twemoji")
        installPack(File("../../chord-desktop/src-tauri/resources/twemoji-1.2.5.tgz").inputStream(), target, null)
        // Draw the card's sample now, so the shot never catches the row while it loads.
        return EmojiImages(EmojiPack.Twemoji, PackIndex(target), px = 64).also { images ->
            emojiRuns(SAMPLE).forEach { images.loadNow(SAMPLE.substring(it.first, it.last + 1)) }
        }
    }

    @Composable private fun Page(model: AppearanceModel) {
        Column(Modifier.fillMaxWidth().background(Chord.colors.surface100).padding(horizontal = 12.dp)) {
            AppearanceContent(model, AppearanceActions())
        }
    }

    private fun shot(dark: Boolean, name: String, model: AppearanceModel, appearance: Appearance = Appearance()) {
        val entry = TestThemes.entry(if (dark) model.prefs.darkTheme else model.prefs.lightTheme)
        compose.setContent {
            ChordTheme(
                dark = dark,
                colors = entry.info.colors(entry.info.accentOr(model.prefs.accents[entry.id])),
                appearance = appearance,
            ) { Page(model) }
        }
        compose.onRoot().captureRoboImage("src/test/screenshots/$name.png")
    }

    @Test fun appearance_page_dark() {
        val tw = twemoji()
        shot(
            true, "appearance_page_dark",
            AppearanceModel(
                AppPrefs(theme = ThemeMode.Dark, emojiPack = EmojiPack.Twemoji),
                library,
                EmojiPackStatus(installed = setOf(EmojiPack.Twemoji, EmojiPack.System)),
                samples = mapOf(EmojiPack.Twemoji to tw),
            ),
        )
    }

    @Test fun appearance_page_light() = shot(
        false, "appearance_page_light",
        AppearanceModel(
            AppPrefs(
                theme = ThemeMode.Light, lightTheme = "catppuccin-latte", darkTheme = "custom-1",
                accents = mapOf("catppuccin-latte" to "peach"), timeFormat = TimeFormat.H12, jumboEmoji = false,
                underlineLinks = false, motion = MotionMode.Reduce, fontSize = 17,
            ),
            library,
            EmojiPackStatus(installing = EmojiPack.Fluent, progress = 0.42f),
        ),
        Appearance(fontSize = 17, underlineLinks = false, jumboEmoji = false, is24h = false),
    )

    @Composable private fun Import(tab: Int, text: String, error: ImportError?) {
        Column(Modifier.fillMaxWidth().background(Chord.colors.surface200).padding(16.dp)) {
            ImportThemeForm(
                onPaste = { null }, onLink = { null }, onDone = {},
                initialTab = tab, initialText = text, initialError = error,
            )
        }
    }

    private fun importShot(dark: Boolean, name: String, tab: Int, text: String, error: ImportError?) {
        compose.setContent { ChordTheme(dark = dark) { Import(tab, text, error) } }
        compose.onRoot().captureRoboImage("src/test/screenshots/$name.png")
    }

    @Test fun import_link_dark() = importShot(true, "appearance_import_link_dark", 0, "https://github.com/user/repo/blob/main/theme.css", null)
    @Test fun import_paste_light() = importShot(
        false, "appearance_import_paste_light", 1, "/**\n * @name Plain\n * @mode light\n */\n:root { --surface-100: #fff; }", ImportError.NoColours,
    )
}

package space.foid.chord.ui.settings

import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onRoot
import com.github.takahirom.roborazzi.captureRoboImage
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import space.foid.chord.ui.theme.ChordTheme
import space.foid.chord.viewmodel.SettingsState
import uniffi.chord_ffi.Availability

@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [36], qualifiers = "w360dp-h1900dp-xxhdpi")
class SettingsScreenshotTest {
    @get:Rule val compose = createComposeRule()

    private val state = SettingsState(
        jid = "alice@chat.example.org",
        nickname = "Alice", nicknameDraft = "Alice",
        availability = Availability.DND, status = "In a meeting", statusDraft = "In a meeting",
        canHide = true,
        blocked = listOf("spam@example.org", "bot@example.net"),
        shareInfo = true, theme = ThemeMode.Dark, loaded = true,
    )

    @Test fun settings_dark() = shot(true, state, "settings_dark")
    @Test fun settings_light() = shot(false, state.copy(theme = ThemeMode.Light, availability = Availability.AVAILABLE), "settings_light")

    @Test
    fun settings_edited_light() = shot(
        false,
        state.copy(
            nicknameDraft = "Alice B", blocked = emptyList(), canHide = false, availability = Availability.AWAY,
            theme = ThemeMode.System, error = "You are not connected to the server.",
        ),
        "settings_edited_light",
        notifications = false,
    )

    private fun shot(dark: Boolean, s: SettingsState, name: String, notifications: Boolean = true) {
        compose.setContent {
            ChordTheme(dark = dark) {
                SettingsContent(
                    state = s,
                    notificationsOn = notifications,
                    version = "0.1.0",
                    licenses = listOf("MIT-Emojibase.txt", "OFL-BricolageGrotesque.txt", "OFL-IBMPlex.txt"),
                    actions = SettingsActions(),
                )
            }
        }
        compose.onRoot().captureRoboImage("src/test/screenshots/$name.png")
    }
}

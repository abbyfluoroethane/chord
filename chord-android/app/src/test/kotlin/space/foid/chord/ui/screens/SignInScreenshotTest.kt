package space.foid.chord.ui.screens

import androidx.compose.runtime.Composable
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
import space.foid.chord.viewmodel.SignInState

// Record: ./gradlew recordRoborazziDebug. Check: verifyRoborazziDebug.
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [36], qualifiers = "w360dp-h720dp-xxhdpi")
class SignInScreenshotTest {
    @get:Rule val compose = createComposeRule()

    private val filled = SignInState(jid = "alice@chord.example", password = "correct horse")

    @Test fun empty_dark() = shot(true, "empty", SignInState())
    @Test fun empty_light() = shot(false, "empty", SignInState())
    @Test fun filled_dark() = shot(true, "filled", filled)
    @Test fun filled_light() = shot(false, "filled", filled)
    @Test fun error_dark() = shot(true, "error", filled.copy(error = "The server refused this password. Check it and try again."))
    @Test fun error_light() = shot(false, "error", filled.copy(error = "The server refused this password. Check it and try again."))
    @Test fun submitting_dark() = shot(true, "submitting", filled.copy(submitting = true))
    @Test fun submitting_light() = shot(false, "submitting", filled.copy(submitting = true))
    @Test fun advanced_dark() = shot(true, "advanced", filled.copy(server = "starttls://chat.example:5222"), advanced = true)
    @Test fun advanced_light() = shot(false, "advanced", filled.copy(server = "starttls://chat.example:5222"), advanced = true)

    private fun shot(dark: Boolean, name: String, state: SignInState, advanced: Boolean = false) {
        compose.setContent { Content(dark, state, advanced) }
        compose.onRoot().captureRoboImage("src/test/screenshots/screens/signin_${name}_${if (dark) "dark" else "light"}.png")
    }

    @Composable
    private fun Content(dark: Boolean, state: SignInState, advanced: Boolean) {
        ChordTheme(dark = dark) {
            SignInContent(
                state = state,
                onJidChange = {},
                onPasswordChange = {},
                onServerChange = {},
                onSubmit = {},
                advancedInitiallyOpen = advanced,
            )
        }
    }
}

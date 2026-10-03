package space.foid.chord.ui.register

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
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
import space.foid.chord.ui.forms.DataFormView
import space.foid.chord.ui.forms.allKindsForm
import space.foid.chord.ui.forms.registrationFormWithCaptcha
import space.foid.chord.ui.forms.withValues
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordTheme
import space.foid.chord.viewmodel.RegisterState
import space.foid.chord.viewmodel.RegisterStep
import uniffi.chord_ffi.RegistrationInfo

// Record: recordRoborazziDebug. Check: verifyRoborazziDebug.
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [36], qualifiers = "w360dp-h720dp-xxhdpi")
class RegisterScreenshotTest {
    @get:Rule val compose = createComposeRule()

    private val server = RegisterState(domain = "foid.space")
    private val legacyInfo = RegistrationInfo(
        "Choose a username and password to register with this server",
        null, listOf("username", "password"), null, false,
    )
    private val legacy = RegisterState(step = RegisterStep.FORM, domain = "foid.space", info = legacyInfo)
    private val captchaForm = registrationFormWithCaptcha()
    private val captcha = RegisterState(
        step = RegisterStep.FORM, domain = "foid.space",
        info = RegistrationInfo(null, captchaForm, emptyList(), null, false), form = captchaForm,
    )

    @Test fun server_dark() = shot(true, "server", server)
    @Test fun server_light() = shot(false, "server", server)
    @Test fun server_busy_dark() = shot(true, "server_busy", server.copy(busy = true))
    @Test fun server_error_dark() = shot(true, "server_error", server.copy(error = "Can't reach the server. Check the address and your connection."))
    @Test fun legacy_dark() = shot(true, "legacy", legacy)
    @Test fun legacy_light() = shot(false, "legacy", legacy)
    @Test fun legacy_error_dark() = shot(true, "legacy_error", legacy.copy(error = "Fill in: Username, Password."))
    @Test fun captcha_dark() = shot(true, "captcha", captcha)
    @Test fun captcha_light() = shot(false, "captcha", captcha)
    @Test fun captcha_errors_dark() = shot(true, "captcha_errors", captcha.copy(showProblems = true, error = "User is required"))
    @Test fun captcha_errors_light() = shot(false, "captcha_errors", captcha.copy(showProblems = true, error = "User is required"))
    @Test fun creating_dark() = shot(true, "creating", legacy.copy(busy = true))
    @Test fun link_only_dark() = shot(
        true, "link_only",
        legacy.copy(info = RegistrationInfo(null, null, emptyList(), uniffi.chord_ffi.OobLink("https://foid.space/join", "Register on the web page."), false)),
    )
    @Test fun all_kinds_dark() = formShot(true)
    @Test fun all_kinds_light() = formShot(false)

    private fun shot(dark: Boolean, name: String, state: RegisterState) {
        compose.setContent {
            ChordTheme(dark = dark) {
                RegisterContent(
                    state = state, onBack = {}, onDomainChange = {}, onContinue = {},
                    onFormChange = {}, onLegacyChange = { _, _ -> }, onSubmit = {},
                )
            }
        }
        compose.onRoot().captureRoboImage("src/test/screenshots/register/register_${name}_${if (dark) "dark" else "light"}.png")
    }

    private fun formShot(dark: Boolean) {
        compose.setContent { FormHost(dark) }
        compose.onRoot().captureRoboImage("src/test/screenshots/register/dataform_all_kinds_${if (dark) "dark" else "light"}.png")
    }

    @Composable
    private fun FormHost(dark: Boolean) {
        ChordTheme(dark = dark) {
            Box(Modifier.fillMaxSize().background(Chord.colors.surface100).padding(16.dp)) {
                DataFormView(form = allKindsForm().withValues(1, emptyList()), onChange = {}, showProblems = true)
            }
        }
    }
}

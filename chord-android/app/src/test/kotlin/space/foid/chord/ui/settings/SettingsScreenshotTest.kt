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
import space.foid.chord.viewmodel.PasswordError
import space.foid.chord.viewmodel.PasswordState
import space.foid.chord.viewmodel.SettingsState
import uniffi.chord_ffi.Availability

@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [36], qualifiers = "w360dp-h800dp-xxhdpi")
class SettingsScreenshotTest {
    @get:Rule val compose = createComposeRule()

    private val state = SettingsState(
        jid = "alice@chat.example.org",
        nickname = "Alice", nicknameDraft = "Alice",
        availability = Availability.DND, status = "In a meeting",
        blocked = listOf("spam@example.org", "bot@example.net"),
        prefs = AppPrefs(
            theme = ThemeMode.Dark, signInShow = SignInShow.Away, signInStatus = "Back soon",
            quietHours = true,
        ),
        signInStatusDraft = "Back soon",
        loaded = true,
    )
    private val licenses = listOf("MIT-Emojibase.txt", "OFL-BricolageGrotesque.txt", "OFL-IBMPlex.txt")

    @Test fun home_dark() = shot(true, SettingsPage.Home, state)
    @Test fun home_light() = shot(false, SettingsPage.Home, state.copy(availability = Availability.AVAILABLE))

    @Test fun profile_dark() = shot(true, SettingsPage.Account, state)
    @Test fun profile_light() =
        shot(false, SettingsPage.Account, state.copy(nicknameDraft = "Alice B", error = "You are not connected to the server."))

    @Test fun account_dark() = shot(true, SettingsPage.Account, state, tab = AccountTab.Account)
    @Test fun account_light() = shot(false, SettingsPage.Account, state, tab = AccountTab.Account)

    @Test fun password_dark() = shot(true, SettingsPage.Password, state)
    @Test fun password_light() =
        shot(false, SettingsPage.Password, state.copy(password = PasswordState(error = PasswordError.Mismatch)))

    @Test fun privacy_dark() = shot(true, SettingsPage.Privacy, state)
    @Test fun privacy_light() =
        shot(false, SettingsPage.Privacy, state.copy(blocked = emptyList(), prefs = state.prefs.copy(shareIdle = false)))

    @Test fun notifications_dark() = shot(true, SettingsPage.Notifications, state)
    @Test fun notifications_light() = shot(
        false, SettingsPage.Notifications, state.copy(prefs = state.prefs.copy(quietHours = false)),
        notifications = false,
    )

    @Test fun appearance_dark() = shot(true, SettingsPage.Appearance, state)
    @Test fun appearance_light() = shot(
        false, SettingsPage.Appearance,
        state.copy(prefs = state.prefs.copy(theme = ThemeMode.Light, showPresence = false)),
    )

    @Test fun advanced_dark() = shot(true, SettingsPage.Advanced, state)
    @Test fun advanced_light() = shot(false, SettingsPage.Advanced, state, connection = SettingsConnection.Offline)

    @Test fun about_dark() = shot(true, SettingsPage.About, state)
    @Test fun about_light() = shot(false, SettingsPage.About, state)

    private fun shot(
        dark: Boolean,
        page: SettingsPage,
        s: SettingsState,
        notifications: Boolean = true,
        tab: AccountTab = AccountTab.Profile,
        connection: SettingsConnection = SettingsConnection.Connected,
    ) {
        compose.setContent {
            ChordTheme(dark = dark) {
                SettingsContent(
                    page = page, onPage = {},
                    state = s,
                    notificationsOn = notifications,
                    version = "0.1.0",
                    licenses = licenses,
                    actions = SettingsActions(),
                    connection = connection,
                    initialTab = tab,
                )
            }
        }
        val tabName = if (page == SettingsPage.Account && tab == AccountTab.Account) "_account" else ""
        val name = "settings_" + page.name.lowercase() + tabName + "_" + (if (dark) "dark" else "light")
        compose.onRoot().captureRoboImage("src/test/screenshots/$name.png")
    }
}

package space.foid.chord.ui.status

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
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
import space.foid.chord.ui.components.ConnectionNotice
import space.foid.chord.ui.screens.AccountUi
import space.foid.chord.ui.screens.UserPanel
import space.foid.chord.ui.sheets.SheetFrame
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordTheme
import uniffi.chord_ffi.Availability

@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [36], qualifiers = "w360dp-h640dp-xxhdpi")
class StatusScreenshotTest {
    @get:Rule val compose = createComposeRule()

    private val me = AccountUi("abby@chat.example.org", "Abby")

    @Test fun panel_dark() = panels(true, "status_panel_dark")
    @Test fun panel_light() = panels(false, "status_panel_light")
    @Test fun sheet_dark() = sheet(true, "status_sheet_dark", StatusState(Availability.AVAILABLE, "🌴 On holiday until Monday", canHide = true))
    @Test fun sheet_light() = sheet(false, "status_sheet_light", StatusState(Availability.DND, null, canHide = true))
    @Test fun sheet_no_invisible_light() = sheet(false, "status_sheet_no_invisible_light", StatusState(Availability.AWAY, null, canHide = false))

    private fun panels(dark: Boolean, name: String) {
        compose.setContent {
            ChordTheme(dark = dark) {
                Column(Modifier.fillMaxWidth().background(Chord.colors.surfaceSide)) {
                    UserPanel(me, StatusState(Availability.AVAILABLE, "🌴 On holiday"), null, {}, {})
                    UserPanel(me, StatusState(Availability.AWAY), null, {}, {})
                    UserPanel(me, StatusState(Availability.DND, "In a meeting"), null, {}, {})
                    UserPanel(me, StatusState(Availability.INVISIBLE), null, {}, {})
                    UserPanel(me, StatusState(Availability.AVAILABLE), ConnectionNotice.Offline, {}, {})
                }
            }
        }
        compose.onRoot().captureRoboImage("src/test/screenshots/$name.png")
    }

    private fun sheet(dark: Boolean, name: String, state: StatusState) {
        compose.setContent {
            ChordTheme(dark = dark) {
                SheetFrame { StatusSheetContent(state, {}, {}, {}) }
            }
        }
        compose.onRoot().captureRoboImage("src/test/screenshots/$name.png")
    }
}

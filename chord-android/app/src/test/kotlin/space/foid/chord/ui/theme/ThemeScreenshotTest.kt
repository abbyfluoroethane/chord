package space.foid.chord.ui.theme

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
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

// The screenshot test pattern. Record: ./gradlew recordRoborazziDebug. Check: verifyRoborazziDebug.
// The images go to app/src/test/screenshots and are the reference for the iOS port.
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [36], qualifiers = "w360dp-h640dp-xxhdpi")
class ThemeScreenshotTest {
    @get:Rule val compose = createComposeRule()

    @Test
    fun tokens_dark() = snapshot(dark = true, "tokens_dark")

    @Test
    fun tokens_light() = snapshot(dark = false, "tokens_light")

    private fun snapshot(dark: Boolean, name: String) {
        compose.setContent {
            ChordTheme(dark = dark) {
                Column(Modifier.background(Chord.colors.surface100).padding(ChordSpace.s4)) {
                    Text("Chord", style = ChordType.title, color = Chord.colors.ink)
                    Text("Muted text", style = ChordType.body, color = Chord.colors.inkMuted)
                    Text("Brand ink", style = ChordType.label, color = Chord.colors.brandInk)
                }
            }
        }
        compose.onRoot().captureRoboImage("src/test/screenshots/$name.png")
    }
}

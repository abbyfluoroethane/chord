package space.foid.chord.ui.avatar

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Canvas
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.Paint
import androidx.compose.ui.graphics.drawscope.CanvasDrawScope
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.unit.dp
import com.github.takahirom.roborazzi.captureRoboImage
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import space.foid.chord.ui.components.Presence
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordSpace
import space.foid.chord.ui.theme.ChordTheme

@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [36], qualifiers = "w360dp-h160dp-xxhdpi")
class JidAvatarScreenshotTest {
    @get:Rule val compose = createComposeRule()

    @Test fun jid_avatar_dark() = shot(true, "jid_avatar_dark")
    @Test fun jid_avatar_light() = shot(false, "jid_avatar_light")

    /** A bitmap with a diagonal gradient, so that it shows through the circle clip. */
    private fun fakeBitmap(): ImageBitmap {
        val bmp = ImageBitmap(120, 120)
        CanvasDrawScope().draw(Density(1f), LayoutDirection.Ltr, Canvas(bmp), Size(120f, 120f)) {
            drawRect(Brush.linearGradient(listOf(Color(0xFFE8A33D), Color(0xFF2F6FDE)), Offset.Zero, Offset(120f, 120f)))
            drawCircle(Color.White, 20f, Offset(60f, 50f))
        }
        return bmp
    }

    private fun shot(dark: Boolean, name: String) {
        val bmp = fakeBitmap()
        compose.setContent {
            ChordTheme(dark = dark) { Sheet(bmp) }
        }
        compose.onRoot().captureRoboImage("src/test/screenshots/components/$name.png")
    }

    @Composable
    private fun Sheet(bmp: ImageBitmap) {
        Row(
            Modifier.background(Chord.colors.surface100).padding(ChordSpace.s4),
            horizontalArrangement = Arrangement.spacedBy(ChordSpace.s4),
        ) {
            JidAvatarContent("alice@example.org", "Alice Smith", image = null)
            JidAvatarContent("alice@example.org", "Alice Smith", image = bmp)
            JidAvatarContent("bob@example.org", "Bob", image = bmp, presence = Presence.Online)
            JidAvatarContent("carol@example.org", "Carol", image = bmp, size = 28.dp)
            JidAvatarContent("dave@example.org", "Dave", image = null, presence = Presence.Away)
        }
    }
}

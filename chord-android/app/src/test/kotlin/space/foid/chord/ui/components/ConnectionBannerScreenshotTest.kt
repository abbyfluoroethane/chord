package space.foid.chord.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performClick
import com.github.takahirom.roborazzi.captureRoboImage
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordSpace
import space.foid.chord.ui.theme.ChordTheme
import space.foid.chord.ui.theme.ChordType

@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [36], qualifiers = "w360dp-h200dp-xxhdpi")
class ConnectionBannerScreenshotTest {
    @get:Rule val compose = createComposeRule()

    @Test fun connecting_dark() = shot(ConnectionNotice.Connecting, true, "banner_connecting_dark")
    @Test fun connecting_light() = shot(ConnectionNotice.Connecting, false, "banner_connecting_light")
    @Test fun offline_dark() = shot(ConnectionNotice.Offline, true, "banner_offline_dark")
    @Test fun offline_light() = shot(ConnectionNotice.Offline, false, "banner_offline_light")
    @Test fun signedOut_dark() = shot(ConnectionNotice.SignedOut, true, "banner_signed_out_dark")
    @Test fun signedOut_light() = shot(ConnectionNotice.SignedOut, false, "banner_signed_out_light")

    @Test
    fun connectedShowsNothing() {
        compose.setContent { ChordTheme(dark = true) { ConnectionBannerContent(null, onSignIn = {}) } }
        compose.onAllNodesWithTag("connection_banner").assertCountEquals(0)
    }

    @Test
    fun onlySignedOutIsTappable() {
        var taps = 0
        var notice by mutableStateOf(ConnectionNotice.Offline)
        compose.setContent { ChordTheme(dark = true) { ConnectionBannerContent(notice, onSignIn = { taps++ }) } }
        compose.onNodeWithTag("connection_banner").performClick()
        assertEquals(0, taps)
        notice = ConnectionNotice.SignedOut
        compose.waitForIdle()
        compose.onNodeWithTag("connection_banner").performClick()
        assertEquals(1, taps)
    }

    @Test fun dots_dark() = dots(true, "banner_dots_dark")
    @Test fun dots_light() = dots(false, "banner_dots_light")

    private fun dots(dark: Boolean, name: String) {
        compose.setContent {
            ChordTheme(dark = dark) {
                Row(
                    Modifier.background(Chord.colors.surfaceRail).padding(ChordSpace.s4),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    listOf(null, ConnectionNotice.Connecting, ConnectionNotice.Offline, ConnectionNotice.SignedOut).forEach {
                        ConnectionDot(it, cut = Chord.colors.surfaceRail, modifier = Modifier.padding(ChordSpace.s2))
                    }
                }
            }
        }
        compose.onRoot().captureRoboImage("src/test/screenshots/$name.png")
    }

    private fun shot(notice: ConnectionNotice, dark: Boolean, name: String) {
        compose.setContent {
            ChordTheme(dark = dark) {
                Column(Modifier.background(Chord.colors.surface100)) {
                    Text("# general", style = ChordType.title, color = Chord.colors.ink, modifier = Modifier.padding(ChordSpace.s4))
                    ConnectionBannerContent(notice, onSignIn = {})
                    Text("Messages show here.", style = ChordType.body, color = Chord.colors.inkMuted, modifier = Modifier.padding(ChordSpace.s4))
                }
            }
        }
        compose.onRoot().captureRoboImage("src/test/screenshots/$name.png")
    }
}

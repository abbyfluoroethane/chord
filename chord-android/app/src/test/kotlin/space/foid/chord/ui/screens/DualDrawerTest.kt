package space.foid.chord.ui.screens

import androidx.activity.ComponentActivity
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.Text
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.swipeLeft
import androidx.compose.ui.test.swipeRight
import androidx.compose.ui.test.click
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import space.foid.chord.ui.layout.DrawerPane
import space.foid.chord.ui.layout.DualDrawer
import space.foid.chord.ui.layout.DualDrawerState
import space.foid.chord.ui.theme.ChordTheme
import uniffi.chord_ffi.ChannelScope

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], qualifiers = "w360dp-h780dp-xxhdpi")
class DualDrawerTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()
    private lateinit var state: DualDrawerState

    private fun setUp() {
        compose.setContent {
            ChordTheme(dark = true) {
                state = remember { DualDrawerState() }
                DualDrawer(
                    state,
                    left = {
                        ChannelDrawerContent(
                            DrawerFixtures.spaces, ChannelScope.Home, {}, DrawerFixtures.homeChannels, true,
                            null, {}, null, {},
                        )
                    },
                    right = { MemberDrawerContent("general", DrawerFixtures.members, true) },
                ) { Box(Modifier.fillMaxSize()) { Text("Timeline") } }
            }
        }
    }

    @Test fun swipe_right_opens_left_drawer() {
        setUp()
        compose.onNodeWithTag("drawer_left").assertDoesNotExist()
        compose.onNodeWithTag("drawer_center").performTouchInput { swipeRight() }
        compose.waitForIdle()
        assertEquals(DrawerPane.Left, state.current)
        compose.onNodeWithTag("channel_list").assertIsDisplayed()
        compose.onNodeWithTag("channel_item_bob@chord.localhost").assertIsDisplayed()
    }

    @Test fun swipe_left_opens_right_drawer() {
        setUp()
        compose.onNodeWithTag("drawer_center").performTouchInput { swipeLeft() }
        compose.waitForIdle()
        assertEquals(DrawerPane.Right, state.current)
        compose.onNodeWithTag("member_list").assertIsDisplayed()
    }

    @Test fun back_closes_open_drawer() {
        setUp()
        compose.onNodeWithTag("drawer_center").performTouchInput { swipeRight() }
        compose.waitForIdle()
        assertEquals(DrawerPane.Left, state.current)
        compose.runOnUiThread { compose.activity.onBackPressedDispatcher.onBackPressed() }
        compose.waitForIdle()
        assertEquals(DrawerPane.Center, state.current)
        compose.onNodeWithText("Timeline").assertIsDisplayed()
    }

    @Test fun tap_on_shifted_center_closes_drawer() {
        setUp()
        compose.onNodeWithTag("drawer_center").performTouchInput { swipeLeft() }
        compose.waitForIdle()
        compose.onNodeWithTag("drawer_center").performTouchInput { click(centerRight - androidx.compose.ui.geometry.Offset(20f, 0f)) }
        compose.waitForIdle()
        assertEquals(DrawerPane.Center, state.current)
    }
}

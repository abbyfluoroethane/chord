package space.foid.chord.ui.inbox

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
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
import space.foid.chord.ui.sheets.SheetFrame
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordTheme
import space.foid.chord.viewmodel.InboxState
import space.foid.chord.viewmodel.InviteItem
import space.foid.chord.viewmodel.RequestItem
import uniffi.chord_ffi.PendingSpaceJoin

@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [36], qualifiers = "w360dp-h760dp-xxhdpi")
class InboxScreenshotTest {
    @get:Rule val compose = createComposeRule()

    private fun shot(dark: Boolean, name: String, content: @Composable () -> Unit) {
        compose.setContent {
            ChordTheme(dark = dark) {
                Box(Modifier.fillMaxSize().background(Chord.colors.surface100)) {
                    Box(Modifier.fillMaxSize().background(Chord.colors.scrim))
                    Box(Modifier.align(Alignment.BottomCenter)) { content() }
                }
            }
        }
        compose.onRoot().captureRoboImage("src/test/screenshots/inbox/$name.png")
    }

    @Composable private fun Inbox(state: InboxState) = SheetFrame { InboxContent(state, InboxCallbacks()) }

    private val request = RequestItem("chord-smoke2@chat.foid.space")
    private val invite = InviteItem("lounge@conference.chat.foid.space", "alice@chat.foid.space", "We are talking about the release.", null)
    private val full = InboxState(
        requests = listOf(request, RequestItem("bob@chat.foid.space", addBack = false)),
        invites = listOf(invite),
        pendingSpaces = listOf(PendingSpaceJoin("pubsub.chat.foid.space", "club", "Closed Club")),
    )

    @Test fun inbox_empty_dark() = shot(true, "inbox_empty_dark") { Inbox(InboxState()) }
    @Test fun inbox_empty_light() = shot(false, "inbox_empty_light") { Inbox(InboxState()) }
    @Test fun inbox_full_dark() = shot(true, "inbox_full_dark") { Inbox(full) }
    @Test fun inbox_full_light() = shot(false, "inbox_full_light") { Inbox(full) }
    @Test fun inbox_request_dark() = shot(true, "inbox_request_dark") { Inbox(InboxState(requests = listOf(request))) }
    @Test fun inbox_invite_light() = shot(false, "inbox_invite_light") { Inbox(InboxState(invites = listOf(invite))) }
    @Test fun inbox_busy_dark() = shot(true, "inbox_busy_dark") {
        Inbox(InboxState(requests = listOf(request), invites = listOf(invite), busy = setOf(request.id)))
    }
    @Test fun inbox_error_dark() = shot(true, "inbox_error_dark") {
        Inbox(InboxState(invites = listOf(invite), error = "Only members can join this room. Ask an admin to add you."))
    }

    @Test fun snackbar_dark() = shot(true, "snackbar_dark") { NoticeSnackbar("Could not join lounge@conference.chat.foid.space: the room is full.") }
    @Test fun snackbar_light() = shot(false, "snackbar_light") { NoticeSnackbar("Request sent. The owner has to approve it.") }
}

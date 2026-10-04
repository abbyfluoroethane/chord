package space.foid.chord.ui.contacts

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.unit.dp
import androidx.compose.foundation.layout.padding
import com.github.takahirom.roborazzi.captureRoboImage
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import space.foid.chord.ui.contacts.ContactsFixtures.blocked
import space.foid.chord.ui.contacts.ContactsFixtures.contacts
import space.foid.chord.ui.join.ConfirmCard
import space.foid.chord.ui.sheets.SheetFrame
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordTheme
import space.foid.chord.viewmodel.AddResult
import space.foid.chord.viewmodel.ContactsState
import space.foid.chord.viewmodel.RequestItem

@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [36], qualifiers = "w360dp-h780dp-xxhdpi")
class ContactsScreenshotTest {
    @get:Rule val compose = createComposeRule()

    private val requests = listOf(RequestItem("rin@foid.space"), RequestItem("sam@other.example"))

    private fun shot(dark: Boolean, name: String, content: @Composable () -> Unit) {
        compose.setContent { ChordTheme(dark = dark) { content() } }
        compose.onRoot().captureRoboImage("src/test/screenshots/contacts/$name.png")
    }

    @Composable private fun Page(
        tab: ContactsTab,
        query: String = "",
        state: ContactsState = ContactsState(contacts, blocked, loaded = true),
        reqs: List<RequestItem> = requests,
    ) = ContactsContent(
        state = state, tab = tab, query = query,
        entries = contactEntries(tab, state.contacts, state.blocked, reqs, query),
        pending = reqs.size, callbacks = ContactsCallbacks(),
    )

    @Test fun online_dark() = shot(true, "online_dark") { Page(ContactsTab.Online) }
    @Test fun online_light() = shot(false, "online_light") { Page(ContactsTab.Online) }
    @Test fun all_dark() = shot(true, "all_dark") { Page(ContactsTab.All) }
    @Test fun all_light() = shot(false, "all_light") { Page(ContactsTab.All) }
    @Test fun pending_dark() = shot(true, "pending_dark") { Page(ContactsTab.Pending) }
    @Test fun pending_light() = shot(false, "pending_light") { Page(ContactsTab.Pending) }
    @Test fun blocked_dark() = shot(true, "blocked_dark") { Page(ContactsTab.Blocked) }
    @Test fun blocked_light() = shot(false, "blocked_light") { Page(ContactsTab.Blocked) }
    @Test fun add_dark() = shot(true, "add_dark") { Page(ContactsTab.Add) }
    @Test fun add_light() = shot(false, "add_light") { Page(ContactsTab.Add) }
    @Test fun add_sent_dark() = shot(true, "add_sent_dark") {
        Page(ContactsTab.Add, state = ContactsState(contacts, blocked, loaded = true, addResult = AddResult.Sent("mika@chord.example")))
    }
    @Test fun add_failed_light() = shot(false, "add_failed_light") {
        Page(ContactsTab.Add, state = ContactsState(contacts, blocked, loaded = true, addResult = AddResult.Failed("That is not an address. Use name@server.example.")))
    }
    @Test fun search_no_match_dark() = shot(true, "search_no_match_dark") { Page(ContactsTab.All, query = "zzz") }
    @Test fun empty_pending_light() = shot(false, "empty_pending_light") { Page(ContactsTab.Pending, reqs = emptyList(), state = ContactsState(emptyList(), emptyList(), loaded = true)) }

    // ---- The row menu and the questions ----
    private fun sheet(dark: Boolean, name: String, content: @Composable () -> Unit) = shot(dark, name) {
        Box(Modifier.fillMaxSize().background(Chord.colors.surface100)) {
            Box(Modifier.fillMaxSize().background(Chord.colors.scrim))
            Box(Modifier.align(Alignment.BottomCenter)) { content() }
        }
    }

    @Composable private fun Menu() = SheetFrame {
        ContactMenuContent(
            ContactEntry(EntryKind.Contact, "alice@chord.localhost", "Alice Martin", contacts.first()),
            onMessage = {}, onRename = {}, onBlock = {}, onRemove = {}, onCopy = {},
        )
    }

    @Test fun menu_dark() = sheet(true, "menu_dark") { Menu() }
    @Test fun menu_light() = sheet(false, "menu_light") { Menu() }

    @Test fun rename_dark() = shot(true, "rename_dark") {
        Box(Modifier.fillMaxSize().background(Chord.colors.surface100).padding(24.dp)) {
            RenameCard("Alice Martin", "alice@chord.localhost", onSave = {}, onCancel = {})
        }
    }

    @Test fun remove_confirm_light() = shot(false, "remove_confirm_light") {
        Box(Modifier.fillMaxWidth().background(Chord.colors.surface100).padding(24.dp)) {
            ConfirmCard(
                "Remove Alice Martin?", "This person leaves your contacts. You will not see when they are online.",
                "Remove contact", "Cancel", {}, {},
            )
        }
    }
}

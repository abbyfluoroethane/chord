package space.foid.chord.viewmodel

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.chord_ffi.ChordException
import uniffi.chord_ffi.JoinOutcome
import uniffi.chord_ffi.NotificationLevel
import uniffi.chord_ffi.SpaceInfo

@OptIn(ExperimentalCoroutinesApi::class)
class JoinViewModelTest {
    private val room = "chord-smoke@conference.chat.foid.space"

    private fun TestScope.vm(api: FakeConversationApi?) = JoinViewModel({ api }, backgroundScope)

    private fun events(vm: JoinViewModel, scope: CoroutineScope): MutableList<JoinEvent> {
        val got = ArrayList<JoinEvent>()
        scope.launch(UnconfinedTestDispatcher()) { vm.events.collect { got += it } }
        return got
    }

    // ---- the error mapping ----

    @Test
    fun serverConditionReadsTheFirstWord() {
        assertEquals("not-authorized", serverCondition(ChordException.Server("not-authorized: password needed")))
        assertEquals("registration-required", serverCondition(ChordException.Server("registration-required")))
        assertEquals("forbidden", serverCondition(ChordException.Server("forbidden (auth)")))
        assertNull(serverCondition(ChordException.Server("Something odd happened")))
        assertNull(serverCondition(ChordException.Timeout()))
    }

    @Test
    fun joinErrorsHavePlainText() {
        assertEquals(JoinFailure.Password(false), describeJoinError(ChordException.Server("not-authorized")))
        assertEquals(JoinFailure.Password(true), describeJoinError(ChordException.Server("not-authorized"), sentPassword = true))
        assertTrue((describeJoinError(ChordException.Server("registration-required")) as JoinFailure.Other).text.contains("members"))
        assertTrue((describeJoinError(ChordException.Server("forbidden")) as JoinFailure.Other).text.contains("banned"))
        assertTrue((describeJoinError(ChordException.Server("item-not-found")) as JoinFailure.Other).text.contains("not found"))
        assertTrue((describeJoinError(ChordException.Server("conflict")) as JoinFailure.Other).text.contains("nickname"))
        assertEquals(
            JoinFailure.Other("You are not connected to the server."),
            describeJoinError(ChordException.NotConnected()),
        )
    }

    // ---- join a room ----

    @Test
    fun joinSelectsTheRoomAndClosesTheSheet() = runTest {
        val api = FakeConversationApi()
        val vm = vm(api)
        val got = events(vm, backgroundScope)
        vm.show()
        vm.onAddress("xmpp:$room?join")
        vm.joinRoom()
        runCurrent()
        assertEquals(listOf("join $room nick=null password=null"), api.calls)
        assertEquals(listOf<JoinEvent>(JoinEvent.OpenChannel(room, "chord-smoke")), got)
        assertFalse(vm.state.value.visible)
        assertEquals("", vm.state.value.address)
    }

    @Test
    fun nicknameAndLinkPasswordAreSent() = runTest {
        val api = FakeConversationApi()
        val vm = vm(api)
        vm.openXmppUri("xmpp:$room?join;password=secret")
        assertTrue(vm.state.value.visible)
        assertEquals(room, vm.state.value.address)
        vm.onNick("  Abby ")
        vm.joinRoom()
        runCurrent()
        assertEquals(listOf("join $room nick=Abby password=secret"), api.calls)
    }

    @Test
    fun badAddressCannotBeSubmitted() = runTest {
        val api = FakeConversationApi()
        val vm = vm(api)
        vm.onAddress("not an address")
        assertFalse(vm.state.value.canJoin)
        vm.joinRoom()
        runCurrent()
        assertNotNull(vm.state.value.roomError)
        assertTrue(api.calls.isEmpty())
    }

    @Test
    fun membersOnlyRoomShowsAClearError() = runTest {
        val api = FakeConversationApi().apply { joinFailures += ChordException.Server("registration-required") }
        val vm = vm(api)
        vm.onAddress(room)
        vm.joinRoom()
        runCurrent()
        val s = vm.state.value
        assertTrue(s.roomError!!.contains("Only members"))
        assertFalse(s.joining)
        assertNull(s.passwordPrompt)
        assertTrue(s.canJoin)
    }

    @Test
    fun passwordRoomAsksThenJoins() = runTest {
        val api = FakeConversationApi().apply { joinFailures += ChordException.Server("not-authorized") }
        val vm = vm(api)
        val got = events(vm, backgroundScope)
        vm.onAddress(room)
        vm.joinRoom()
        runCurrent()
        assertEquals(PasswordPrompt(wrong = false), vm.state.value.passwordPrompt)
        assertNull(vm.state.value.roomError)
        assertFalse(vm.state.value.canJoin) // needs the password first
        vm.onPassword("hunter2")
        assertTrue(vm.state.value.canJoin)
        vm.joinRoom()
        runCurrent()
        assertEquals("join $room nick=null password=hunter2", api.calls.last())
        assertEquals(listOf<JoinEvent>(JoinEvent.OpenChannel(room, "chord-smoke")), got)
    }

    @Test
    fun wrongPasswordAsksAgainWithAMessage() = runTest {
        val api = FakeConversationApi().apply {
            joinFailures += ChordException.Server("not-authorized")
            joinFailures += ChordException.Server("not-authorized")
        }
        val vm = vm(api)
        vm.onAddress(room)
        vm.joinRoom()
        runCurrent()
        vm.onPassword("nope")
        vm.joinRoom()
        runCurrent()
        assertEquals(PasswordPrompt(wrong = true), vm.state.value.passwordPrompt)
        assertEquals("That password was not accepted.", vm.state.value.roomError)
        assertEquals("", vm.state.value.password)
    }

    @Test
    fun editingTheAddressDropsThePasswordQuestion() = runTest {
        val api = FakeConversationApi().apply { joinFailures += ChordException.Server("not-authorized") }
        val vm = vm(api)
        vm.onAddress(room)
        vm.joinRoom()
        runCurrent()
        vm.onAddress("other@conference.chat.foid.space")
        assertNull(vm.state.value.passwordPrompt)
    }

    @Test
    fun signedOutJoinFails() = runTest {
        val vm = vm(null)
        vm.onAddress(room)
        vm.joinRoom()
        runCurrent()
        assertEquals("You are not connected to the server.", vm.state.value.roomError)
    }

    // ---- message someone ----

    @Test
    fun messageAddsAnUnknownContactThenOpensTheChat() = runTest {
        val api = FakeConversationApi()
        val vm = vm(api)
        val got = events(vm, backgroundScope)
        vm.show(JoinTab.Person)
        vm.onPersonJid("Chord-Smoke2@chat.foid.space")
        vm.onPersonName(" Two ")
        vm.messagePerson()
        runCurrent()
        assertEquals(listOf("contacts", "addContact chord-smoke2@chat.foid.space name=Two"), api.calls)
        assertEquals(listOf<JoinEvent>(JoinEvent.OpenChannel("chord-smoke2@chat.foid.space", "Two")), got)
        assertFalse(vm.state.value.visible)
    }

    @Test
    fun messageToAKnownContactDoesNotAddItAgain() = runTest {
        val api = FakeConversationApi().apply { contactList = listOf(contact("bob@example.org", "Bobby")) }
        val vm = vm(api)
        val got = events(vm, backgroundScope)
        vm.onPersonJid("bob@example.org")
        vm.messagePerson()
        runCurrent()
        assertEquals(listOf("contacts"), api.calls)
        assertEquals(listOf<JoinEvent>(JoinEvent.OpenChannel("bob@example.org", "Bobby")), got)
    }

    @Test
    fun messageToYourselfIsRefused() = runTest {
        val api = FakeConversationApi()
        val vm = vm(api)
        vm.onPersonJid("ME@example.org")
        vm.messagePerson()
        runCurrent()
        assertEquals("That is your own address.", vm.state.value.personError)
        assertFalse(api.calls.any { it.startsWith("add") })
    }

    @Test
    fun badPersonAddressHasAMessage() = runTest {
        val vm = vm(FakeConversationApi())
        vm.onPersonJid("bob")
        assertFalse(vm.state.value.canMessage)
        vm.messagePerson()
        assertNotNull(vm.state.value.personError)
    }

    @Test
    fun plainLinkOpensMessageSomeone() = runTest {
        val vm = vm(FakeConversationApi())
        vm.openXmppUri("xmpp:bob@example.org?roster;name=Bob")
        val s = vm.state.value
        assertEquals(JoinTab.Person, s.tab)
        assertEquals("bob@example.org", s.personJid)
        assertEquals("Bob", s.personName)
        assertTrue(s.visible)
    }

    @Test
    fun badLinkGivesAMessageAndNoSheet() = runTest {
        val vm = vm(FakeConversationApi())
        val got = events(vm, backgroundScope)
        vm.openXmppUri("xmpp:bob@example.org?sendfile")
        assertEquals(listOf<JoinEvent>(JoinEvent.Message("This link is not valid.")), got)
        assertFalse(vm.state.value.visible)
    }

    // ---- spaces ----

    private val space = SpaceInfo("pubsub.example.org", "design", "Design", "About design", "open")

    @Test
    fun spacesLoadOnceWhenTheTabOpens() = runTest {
        val api = FakeConversationApi().apply { spaceList = listOf(space) }
        val vm = vm(api)
        vm.show(JoinTab.Spaces)
        runCurrent()
        vm.selectTab(JoinTab.Room)
        vm.selectTab(JoinTab.Spaces)
        runCurrent()
        assertEquals(1, api.calls.count { it == "browse" })
        val list = vm.state.value.spaces as SpacesState.Loaded
        assertEquals("Design", list.rows.single().info.name)
    }

    @Test
    fun spaceListFailureCanBeRetried() = runTest {
        val api = FakeConversationApi().apply { failWith = ChordException.NotConnected() }
        val vm = vm(api)
        vm.show(JoinTab.Spaces)
        runCurrent()
        assertTrue(vm.state.value.spaces is SpacesState.Failed)
        api.failWith = null
        api.spaceList = listOf(space)
        vm.loadSpaces(force = true)
        runCurrent()
        assertTrue(vm.state.value.spaces is SpacesState.Loaded)
    }

    @Test
    fun joiningASpaceSelectsIt() = runTest {
        val api = FakeConversationApi().apply { spaceList = listOf(space) }
        val vm = vm(api)
        val got = events(vm, backgroundScope)
        vm.show(JoinTab.Spaces)
        runCurrent()
        vm.joinSpace((vm.state.value.spaces as SpacesState.Loaded).rows.single())
        runCurrent()
        assertEquals(listOf<JoinEvent>(JoinEvent.OpenSpace("pubsub.example.org", "design")), got)
        assertFalse(vm.state.value.visible)
    }

    @Test
    fun joiningAClosedSpaceShowsRequested() = runTest {
        val api = FakeConversationApi().apply { spaceList = listOf(space); spaceOutcome = JoinOutcome.PENDING }
        val vm = vm(api)
        val got = events(vm, backgroundScope)
        vm.show(JoinTab.Spaces)
        runCurrent()
        vm.joinSpace((vm.state.value.spaces as SpacesState.Loaded).rows.single())
        runCurrent()
        val row = (vm.state.value.spaces as SpacesState.Loaded).rows.single()
        assertEquals(SpaceStatus.Requested, row.status)
        assertEquals(listOf<JoinEvent>(JoinEvent.SpaceRequested), got)
        assertTrue(vm.state.value.visible)
    }

    @Test
    fun failedSpaceJoinKeepsTheRowWithAnError() = runTest {
        val api = FakeConversationApi().apply { spaceList = listOf(space) }
        val vm = vm(api)
        vm.show(JoinTab.Spaces)
        runCurrent()
        api.failWith = ChordException.Server("forbidden")
        vm.joinSpace((vm.state.value.spaces as SpacesState.Loaded).rows.single())
        runCurrent()
        val row = (vm.state.value.spaces as SpacesState.Loaded).rows.single()
        assertEquals(SpaceStatus.Idle, row.status)
        assertNotNull(row.error)
    }

    // ---- channel actions ----

    private val target = ChannelActionTarget(room, "chord-smoke", ActionKind.Room)

    @Test
    fun actionsLoadAndChangeTheLevel() = runTest {
        val api = FakeConversationApi().apply { level = NotificationLevel.MENTIONS }
        val vm = vm(api)
        vm.openActions(target)
        runCurrent()
        assertEquals(NotificationLevel.MENTIONS, vm.actions.value!!.level)
        vm.setLevel(NotificationLevel.NONE)
        runCurrent()
        assertEquals(NotificationLevel.NONE, vm.actions.value!!.level)
        assertEquals("setLevel $room NONE", api.calls.last())
    }

    @Test
    fun failedLevelChangeGoesBack() = runTest {
        val api = FakeConversationApi().apply { level = NotificationLevel.ALL }
        val vm = vm(api)
        vm.openActions(target)
        runCurrent()
        api.failWith = ChordException.NotConnected()
        vm.setLevel(NotificationLevel.NONE)
        runCurrent()
        assertEquals(NotificationLevel.ALL, vm.actions.value!!.level)
        assertNotNull(vm.actions.value!!.error)
    }

    @Test
    fun leavingARoomClosesTheSheetAndTellsTheScreen() = runTest {
        val api = FakeConversationApi()
        val vm = vm(api)
        val got = events(vm, backgroundScope)
        vm.openActions(target)
        runCurrent()
        vm.leave()
        runCurrent()
        assertEquals("leave $room", api.calls.last())
        assertNull(vm.actions.value)
        assertEquals(listOf<JoinEvent>(JoinEvent.Left(room)), got)
    }

    @Test
    fun removingAContactUsesRemoveContact() = runTest {
        val api = FakeConversationApi()
        val vm = vm(api)
        vm.openActions(ChannelActionTarget("bob@example.org", "Bob", ActionKind.Contact))
        runCurrent()
        vm.leave()
        runCurrent()
        assertEquals("removeContact bob@example.org", api.calls.last())
    }

    @Test
    fun anOccupantCannotBeLeft() = runTest {
        val api = FakeConversationApi()
        val vm = vm(api)
        vm.openActions(ChannelActionTarget("$room/nick", "nick", ActionKind.Occupant))
        runCurrent()
        vm.leave()
        runCurrent()
        assertFalse(api.calls.any { it.startsWith("leave") || it.startsWith("removeContact") })
        assertEquals(room, vm.actions.value!!.target.address)
    }

    @Test
    fun markReadCallsTheCore() = runTest {
        val api = FakeConversationApi()
        val vm = vm(api)
        vm.openActions(target)
        runCurrent()
        vm.markRead()
        runCurrent()
        assertEquals("markRead $room", api.calls.last())
    }
}

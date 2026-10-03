package space.foid.chord.viewmodel

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.chord_ffi.ChordException
import uniffi.chord_ffi.ClientEvent
import uniffi.chord_ffi.PendingSpaceJoin

@OptIn(ExperimentalCoroutinesApi::class)
class InboxViewModelTest {
    private class Rig(val api: FakeConversationApi, val bus: MutableSharedFlow<ClientEvent>, val vm: InboxViewModel)

    private fun TestScope.rig(api: FakeConversationApi = FakeConversationApi()): Rig {
        val bus = MutableSharedFlow<ClientEvent>(extraBufferCapacity = 16)
        val vm = InboxViewModel({ api }, bus, backgroundScope)
        runCurrent()
        return Rig(api, bus, vm)
    }

    private fun events(vm: InboxViewModel, scope: CoroutineScope): MutableList<JoinEvent> {
        val got = ArrayList<JoinEvent>()
        scope.launch(UnconfinedTestDispatcher()) { vm.events.collect { got += it } }
        return got
    }

    private val invite = ClientEvent.RoomInvite("lounge@conference.example.org", "alice@example.org", "Come in", null)

    @Test
    fun startsEmptyAndReloadsPendingSpaceJoins() = runTest {
        val api = FakeConversationApi().apply { pending = listOf(PendingSpaceJoin("pubsub.example.org", "closed", "Closed club")) }
        val r = rig(api)
        assertEquals(0, r.vm.state.value.count)
        assertEquals("Closed club", r.vm.state.value.pendingSpaces.single().name)
        assertTrue("pending" in api.calls)
        assertEquals(0, r.vm.state.value.count) // a pending join does not count in the badge
    }

    @Test
    fun requestsAndInvitesCountInTheBadge() = runTest {
        val r = rig()
        r.bus.emit(ClientEvent.SubscriptionRequest("bob@example.org"))
        r.bus.emit(invite)
        runCurrent()
        assertEquals(2, r.vm.state.value.count)
        assertEquals("bob@example.org", r.vm.state.value.requests.single().jid)
        assertEquals("Come in", r.vm.state.value.invites.single().reason)
    }

    @Test
    fun theSameRequestOrInviteCountsOnce() = runTest {
        val r = rig()
        r.bus.emit(ClientEvent.SubscriptionRequest("bob@example.org"))
        r.bus.emit(ClientEvent.SubscriptionRequest("BOB@example.org"))
        r.bus.emit(invite)
        r.bus.emit(ClientEvent.RoomInvite(invite.room, "carol@example.org", "New reason", "pw"))
        runCurrent()
        val s = r.vm.state.value
        assertEquals(1, s.requests.size)
        assertEquals(1, s.invites.size)
        assertEquals("carol@example.org", s.invites.single().from)
        assertEquals("pw", s.invites.single().password)
    }

    @Test
    fun otherEventsAreIgnored() = runTest {
        val r = rig()
        r.bus.emit(ClientEvent.ContactChanged("bob@example.org"))
        runCurrent()
        assertEquals(0, r.vm.state.value.count)
    }

    @Test
    fun noticesGoToTheSnackbar() = runTest {
        val r = rig()
        val got = events(r.vm, backgroundScope)
        r.bus.emit(ClientEvent.Notice("Could not join the room"))
        runCurrent()
        assertEquals(listOf<JoinEvent>(JoinEvent.Message("Could not join the room")), got)
        assertEquals(0, r.vm.state.value.count)
    }

    @Test
    fun acceptWithAddBackIsTheDefault() = runTest {
        val r = rig()
        r.bus.emit(ClientEvent.SubscriptionRequest("bob@example.org"))
        runCurrent()
        r.vm.accept(r.vm.state.value.requests.single())
        runCurrent()
        assertEquals("approve bob@example.org addBack=true", r.api.calls.last())
        assertEquals(0, r.vm.state.value.count)
        assertTrue(r.vm.state.value.busy.isEmpty())
    }

    @Test
    fun acceptWithoutAddBack() = runTest {
        val r = rig()
        r.bus.emit(ClientEvent.SubscriptionRequest("bob@example.org"))
        runCurrent()
        r.vm.setAddBack("bob@example.org", false)
        r.vm.accept(r.vm.state.value.requests.single())
        runCurrent()
        assertEquals("approve bob@example.org addBack=false", r.api.calls.last())
    }

    @Test
    fun denyRemovesTheRequest() = runTest {
        val r = rig()
        r.bus.emit(ClientEvent.SubscriptionRequest("bob@example.org"))
        runCurrent()
        r.vm.deny(r.vm.state.value.requests.single())
        runCurrent()
        assertEquals("deny bob@example.org", r.api.calls.last())
        assertEquals(0, r.vm.state.value.count)
    }

    @Test
    fun failedAnswerKeepsTheItemAndShowsAnError() = runTest {
        val r = rig()
        r.bus.emit(ClientEvent.SubscriptionRequest("bob@example.org"))
        runCurrent()
        r.api.failWith = ChordException.NotConnected()
        r.vm.accept(r.vm.state.value.requests.single())
        runCurrent()
        assertEquals(1, r.vm.state.value.count)
        assertNotNull(r.vm.state.value.error)
        assertTrue(r.vm.state.value.busy.isEmpty())
        r.vm.clearError()
        assertNull(r.vm.state.value.error)
    }

    @Test
    fun acceptingAnInviteJoinsAndSelectsTheRoom() = runTest {
        val r = rig()
        val got = events(r.vm, backgroundScope)
        r.bus.emit(ClientEvent.RoomInvite(invite.room, invite.from, null, "pw"))
        runCurrent()
        r.vm.accept(r.vm.state.value.invites.single())
        runCurrent()
        assertEquals("join ${invite.room} nick=null password=pw", r.api.calls.last())
        assertEquals(listOf<JoinEvent>(JoinEvent.OpenChannel(invite.room, "lounge")), got)
        assertEquals(0, r.vm.state.value.count)
    }

    @Test
    fun failedInviteJoinShowsTheReason() = runTest {
        val api = FakeConversationApi().apply { joinFailures += ChordException.Server("registration-required") }
        val r = rig(api)
        r.bus.emit(invite)
        runCurrent()
        r.vm.accept(r.vm.state.value.invites.single())
        runCurrent()
        assertTrue(r.vm.state.value.error!!.contains("Only members"))
        assertEquals(1, r.vm.state.value.count)
    }

    @Test
    fun decliningAnInviteTellsTheInviter() = runTest {
        val r = rig()
        r.bus.emit(invite)
        runCurrent()
        r.vm.decline(r.vm.state.value.invites.single())
        runCurrent()
        assertEquals("decline ${invite.room} ${invite.from}", r.api.calls.last())
        assertEquals(0, r.vm.state.value.count)
    }
}

package space.foid.chord.viewmodel

import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import space.foid.chord.ui.contacts.ContactsFixtures.contact
import uniffi.chord_ffi.ChordException
import uniffi.chord_ffi.ClientEvent
import uniffi.chord_ffi.Contact

private class FakeContactsApi : ContactsApi {
    var roster = listOf(contact("alice@chord.localhost", "Alice", online = true))
    var blockedList = emptyList<String>()
    val calls = ArrayList<String>()
    var failWith: Throwable? = null
    var reads = 0

    private fun record(call: String) {
        failWith?.let { throw it }
        calls += call
    }

    override suspend fun contacts(): List<Contact> { reads++; return roster }
    override suspend fun blocked(): List<String> = blockedList
    override suspend fun add(jid: String, name: String?) = record("add $jid $name")
    override suspend fun remove(jid: String) = record("remove $jid")
    override suspend fun rename(jid: String, name: String?) = record("rename $jid $name")
    override suspend fun block(jid: String) { record("block $jid"); blockedList = blockedList + jid }
    override suspend fun unblock(jid: String) { record("unblock $jid"); blockedList = blockedList - jid }
    override fun account(): String = "me@chord.localhost/phone"
}

@OptIn(ExperimentalCoroutinesApi::class)
class ContactsViewModelTest {
    private class Rig(val api: FakeContactsApi, val bus: MutableSharedFlow<ClientEvent>, val vm: ContactsViewModel)

    private fun TestScope.rig(api: FakeContactsApi = FakeContactsApi()): Rig {
        val bus = MutableSharedFlow<ClientEvent>(extraBufferCapacity = 16)
        val vm = ContactsViewModel({ api }, bus, backgroundScope, settle = 100)
        runCurrent()
        return Rig(api, bus, vm)
    }

    @Test
    fun loadsTheRosterAndTheBlocklistAtStart() = runTest {
        val api = FakeContactsApi().apply { blockedList = listOf("evil@spam.example") }
        val r = rig(api)
        assertTrue(r.vm.state.value.loaded)
        assertEquals("Alice", r.vm.state.value.contacts.single().name)
        assertEquals(listOf("evil@spam.example"), r.vm.state.value.blocked)
    }

    @Test
    fun aContactChangedEventReadsTheRosterAgain() = runTest {
        val r = rig()
        r.api.roster = listOf(contact("alice@chord.localhost", "Alice", online = false))
        r.bus.emit(ClientEvent.ContactChanged("alice@chord.localhost"))
        advanceTimeBy(150)
        runCurrent()
        assertEquals(false, r.vm.state.value.contacts.single().online)
    }

    @Test
    fun theBlockListEventReadsTheBlocklistAgain() = runTest {
        val r = rig()
        r.api.blockedList = listOf("x@spam.example")
        r.bus.emit(ClientEvent.BlockListChanged)
        advanceTimeBy(150)
        runCurrent()
        assertEquals(listOf("x@spam.example"), r.vm.state.value.blocked)
    }

    @Test
    fun aBurstOfPresenceChangesSharesOneRead() = runTest {
        val r = rig()
        val before = r.api.reads
        repeat(10) { r.bus.emit(ClientEvent.ContactChanged("alice@chord.localhost")) }
        advanceTimeBy(500)
        runCurrent()
        // The first change reads at once. The rest wait and share one more read.
        assertTrue("reads: ${r.api.reads - before}", r.api.reads - before <= 2)
    }

    @Test
    fun otherEventsAreIgnored() = runTest {
        val r = rig()
        val before = r.api.reads
        r.bus.emit(ClientEvent.Notice("hello"))
        advanceTimeBy(500)
        runCurrent()
        assertEquals(before, r.api.reads)
    }

    @Test
    fun addAsksForTheSubscriptionAndReportsIt() = runTest {
        val r = rig()
        r.vm.add("  Mika@Chord.Example ")
        runCurrent()
        assertEquals("add mika@chord.example null", r.api.calls.single())
        assertEquals(AddResult.Sent("mika@chord.example"), r.vm.state.value.addResult)
    }

    @Test
    fun addRefusesABadAddressOwnAddressAndAContact() = runTest {
        val r = rig()
        r.vm.add("not an address")
        runCurrent()
        assertTrue(r.vm.state.value.addResult is AddResult.Failed)
        r.vm.add("me@chord.localhost")
        runCurrent()
        assertEquals(AddResult.Failed("That is your own address."), r.vm.state.value.addResult)
        r.vm.add("alice@chord.localhost")
        runCurrent()
        assertTrue((r.vm.state.value.addResult as AddResult.Failed).text.contains("a contact already"))
        assertTrue(r.api.calls.isEmpty())
    }

    @Test
    fun addFailureShowsAPlainText() = runTest {
        val r = rig()
        r.api.failWith = ChordException.NotConnected()
        r.vm.add("mika@chord.example")
        runCurrent()
        assertEquals(AddResult.Failed("You are not connected to the server."), r.vm.state.value.addResult)
        r.vm.clearAddResult()
        assertNull(r.vm.state.value.addResult)
    }

    @Test
    fun renameTrimsAndSendsNullForAnEmptyName() = runTest {
        val r = rig()
        r.vm.rename("alice@chord.localhost", "  Ally ")
        runCurrent()
        r.vm.rename("alice@chord.localhost", "   ")
        runCurrent()
        assertEquals(listOf("rename alice@chord.localhost Ally", "rename alice@chord.localhost null"), r.api.calls)
    }

    @Test
    fun blockAndUnblockUpdateTheBlocklist() = runTest {
        val r = rig()
        r.vm.block("alice@chord.localhost")
        runCurrent()
        assertEquals(listOf("alice@chord.localhost"), r.vm.state.value.blocked)
        r.vm.unblock("alice@chord.localhost")
        runCurrent()
        assertTrue(r.vm.state.value.blocked.isEmpty())
    }

    @Test
    fun removeCallsBackAfterTheCoreAnswered() = runTest {
        val r = rig()
        var done = false
        r.vm.remove("alice@chord.localhost") { done = true }
        runCurrent()
        assertEquals("remove alice@chord.localhost", r.api.calls.single())
        assertTrue(done)
        assertTrue(r.vm.state.value.busy.isEmpty())
    }

    @Test
    fun aFailedActionKeepsTheListAndShowsAnError() = runTest {
        val r = rig()
        r.api.failWith = ChordException.NotConnected()
        r.vm.remove("alice@chord.localhost")
        runCurrent()
        assertNotNull(r.vm.state.value.error)
        assertEquals(1, r.vm.state.value.contacts.size)
        assertTrue(r.vm.state.value.busy.isEmpty())
        r.vm.clearError()
        assertNull(r.vm.state.value.error)
    }

    @Test
    fun findIgnoresCaseAndResource() = runTest {
        val r = rig()
        assertNotNull(r.vm.state.value.find("ALICE@chord.localhost/phone"))
        assertNull(r.vm.state.value.find("bob@chord.localhost"))
    }
}

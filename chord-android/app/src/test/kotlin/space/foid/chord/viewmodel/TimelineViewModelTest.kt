package space.foid.chord.viewmodel

import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import space.foid.chord.data.TimelineTarget
import uniffi.chord_ffi.ChordException
import uniffi.chord_ffi.TimelineDiff

@OptIn(ExperimentalCoroutinesApi::class)
class TimelineViewModelTest {
    private val room = TimelineTarget.Room("room@muc.example.org")

    private fun TestScope.vm(api: FakeChatApi?, flow: MutableStateFlow<space.foid.chord.data.ChatApi?> = MutableStateFlow(api)) =
        TimelineViewModel(room, flow, backgroundScope) to flow

    private suspend fun TestScope.settle() {
        advanceTimeBy(20)
        runCurrent()
    }

    @Test
    fun opensTheTimelineAndShowsTheFirstReset() = runTest {
        val api = FakeChatApi()
        val (vm, _) = vm(api)
        runCurrent()
        assertEquals(1, api.timelines.size)
        assertFalse(vm.loaded.value)
        api.timelines[0].sink(TimelineDiff.Reset(listOf(msg("1"), msg("2"))))
        settle()
        assertEquals(listOf("1", "2"), vm.items.value.map { it.id })
        assertTrue(vm.loaded.value)
    }

    @Test
    fun diffsInsertAtTheEnd() = runTest {
        val api = FakeChatApi()
        val (vm, _) = vm(api)
        runCurrent()
        val sink = api.timelines[0].sink
        sink(TimelineDiff.Reset(listOf(msg("1"))))
        sink(TimelineDiff.Insert(1u, msg("2")))
        sink(TimelineDiff.Update(0u, msg("1", "edited")))
        settle()
        assertEquals(listOf("edited", "text 2"), vm.items.value.map { it.body })
    }

    @Test
    fun noClientMeansNoSubscriptionAndAnEmptyList() = runTest {
        val (vm, flow) = vm(null)
        runCurrent()
        assertTrue(vm.items.value.isEmpty())
        val api = FakeChatApi()
        flow.value = api
        runCurrent()
        api.timelines[0].sink(TimelineDiff.Reset(listOf(msg("1"))))
        settle()
        assertEquals(1, vm.items.value.size)
        // Sign out: the subscription closes and the list is empty.
        flow.value = null
        settle()
        assertTrue(api.timelines[0].closed)
        assertTrue(vm.items.value.isEmpty())
        assertFalse(vm.loaded.value)
    }

    @Test
    fun aNewClientOpensANewSubscription() = runTest {
        val first = FakeChatApi()
        val (vm, flow) = vm(first)
        runCurrent()
        val second = FakeChatApi()
        flow.value = second
        runCurrent()
        assertTrue(first.timelines[0].closed)
        assertEquals(1, second.timelines.size)
        assertEquals(0, vm.items.value.size)
    }

    @Test
    fun anImpossibleDiffOpensAFreshSubscription() = runTest {
        val api = FakeChatApi()
        val (vm, _) = vm(api)
        runCurrent()
        api.timelines[0].sink(TimelineDiff.Reset(listOf(msg("1"))))
        settle()
        api.timelines[0].sink(TimelineDiff.Remove(9u))
        settle()
        assertTrue(api.timelines[0].closed)
        assertEquals(2, api.timelines.size)
        api.timelines[1].sink(TimelineDiff.Reset(listOf(msg("1"), msg("2"))))
        settle()
        assertEquals(2, vm.items.value.size)
    }

    @Test
    fun loadOlderCallsTheCoreOnceAtATime() = runTest {
        val api = FakeChatApi()
        val (vm, _) = vm(api)
        runCurrent()
        val t = api.timelines[0]
        t.sink(TimelineDiff.Reset(listOf(msg("5"))))
        settle()
        val gate = kotlinx.coroutines.CompletableDeferred<Unit>()
        t.onPaginate = { gate.await() }
        vm.loadOlder()
        vm.loadOlder()
        vm.loadOlder()
        runCurrent()
        assertEquals(1, t.paginations)
        assertTrue(vm.loadingOlder.value)
        t.sink(TimelineDiff.Insert(0u, msg("4")))
        gate.complete(Unit)
        settle()
        assertFalse(vm.loadingOlder.value)
        assertFalse(vm.reachedStart.value)
        vm.loadOlder()
        runCurrent()
        assertEquals(2, t.paginations)
    }

    @Test
    fun loadOlderStopsAtTheStartOfTheHistory() = runTest {
        val api = FakeChatApi()
        val (vm, _) = vm(api)
        runCurrent()
        val t = api.timelines[0]
        t.sink(TimelineDiff.Reset(listOf(msg("1"))))
        settle()
        vm.loadOlder() // The core adds nothing.
        advanceTimeBy(TimelineViewModel.GROW_WAIT_MILLIS + 100)
        runCurrent()
        assertTrue(vm.reachedStart.value)
        assertFalse(vm.loadingOlder.value)
        vm.loadOlder()
        runCurrent()
        assertEquals(1, t.paginations)
    }

    @Test
    fun olderMessagesThatArriveAfterTheCallStillCount() = runTest {
        val api = FakeChatApi()
        val (vm, _) = vm(api)
        runCurrent()
        val t = api.timelines[0]
        t.sink(TimelineDiff.Reset(listOf(msg("3"))))
        settle()
        vm.loadOlder()
        runCurrent()
        t.sink(TimelineDiff.Insert(0u, msg("2")))
        advanceTimeBy(100)
        runCurrent()
        advanceUntilIdle()
        assertFalse(vm.reachedStart.value)
        assertEquals(listOf("2", "3"), vm.items.value.map { it.id })
    }

    @Test
    fun loadOlderFailureBecomesAMessageAndAllowsARetry() = runTest {
        val api = FakeChatApi()
        val (vm, _) = vm(api)
        runCurrent()
        val t = api.timelines[0]
        t.sink(TimelineDiff.Reset(listOf(msg("1"))))
        settle()
        t.onPaginate = { throw ChordException.NotConnected() }
        val errors = ArrayList<String>()
        backgroundScope.launch { vm.errors.collect { errors += it } }
        runCurrent()
        vm.loadOlder()
        runCurrent()
        assertEquals(listOf("You are not connected to the server."), errors)
        assertFalse(vm.loadingOlder.value)
        assertFalse(vm.reachedStart.value)
        vm.loadOlder()
        runCurrent()
        assertEquals(2, t.paginations)
    }

    @Test
    fun loadOlderDoesNothingWithoutASubscription() = runTest {
        val (vm, _) = vm(null)
        runCurrent()
        vm.loadOlder()
        runCurrent()
        assertFalse(vm.loadingOlder.value)
    }

    @Test
    fun actionsCallTheCore() = runTest {
        val api = FakeChatApi()
        val (vm, _) = vm(api)
        runCurrent()
        vm.send("hi")
        vm.reply("1", "re")
        vm.edit("1", "ed")
        vm.retract("1")
        vm.toggleReaction("1", "+1")
        vm.markRead()
        runCurrent()
        assertEquals(
            listOf("send hi", "reply 1 re", "edit 1 ed", "retract 1", "react 1 +1", "markRead"),
            api.calls,
        )
    }

    @Test
    fun aFailedActionIsReportedInPlainEnglish() = runTest {
        val api = FakeChatApi()
        val (vm, _) = vm(api)
        runCurrent()
        val errors = ArrayList<String>()
        backgroundScope.launch { vm.errors.collect { errors += it } }
        runCurrent()
        api.failWith = ChordException.Server("forbidden")
        vm.send("hi")
        runCurrent()
        assertEquals(listOf("The server refused the request."), errors)
    }

    @Test
    fun anActionWithoutAClientTellsTheUser() = runTest {
        val (vm, _) = vm(null)
        val errors = ArrayList<String>()
        backgroundScope.launch { vm.errors.collect { errors += it } }
        runCurrent()
        vm.send("hi")
        runCurrent()
        assertEquals(listOf("You are not signed in."), errors)
    }

    @Test
    fun markReadFailureIsQuiet() = runTest {
        val api = FakeChatApi()
        val (vm, _) = vm(api)
        runCurrent()
        val errors = ArrayList<String>()
        backgroundScope.launch { vm.errors.collect { errors += it } }
        runCurrent()
        api.failWith = ChordException.NotConnected()
        vm.markRead()
        runCurrent()
        assertTrue(errors.isEmpty())
    }

    @Test
    fun typingStartsOnceAndStopsAfterTheIdleTime() = runTest {
        val api = FakeChatApi()
        val (vm, _) = vm(api)
        runCurrent()
        vm.onComposerChanged("h")
        vm.onComposerChanged("hi")
        runCurrent()
        assertEquals(listOf("typing true"), api.calls)
        advanceTimeBy(TimelineViewModel.TYPING_IDLE_MILLIS - 100)
        vm.onComposerChanged("hi!") // Input restarts the timer.
        advanceTimeBy(TimelineViewModel.TYPING_IDLE_MILLIS - 100)
        runCurrent()
        assertEquals(listOf("typing true"), api.calls)
        advanceTimeBy(200)
        runCurrent()
        assertEquals(listOf("typing true", "typing false"), api.calls)
    }

    @Test
    fun emptyComposerAndSendStopTyping() = runTest {
        val api = FakeChatApi()
        val (vm, _) = vm(api)
        runCurrent()
        vm.onComposerChanged("a")
        vm.onComposerChanged("")
        runCurrent()
        assertEquals(listOf("typing true", "typing false"), api.calls)
        api.calls.clear()
        vm.onComposerChanged("b")
        vm.send("b")
        runCurrent()
        assertEquals(listOf("typing true", "send b", "typing false"), api.calls)
    }

    @Test
    fun aFailedSubscriptionSetsTheError() = runTest {
        val api = FakeChatApi()
        api.failWith = ChordException.InvalidJid("x")
        val (vm, _) = vm(api)
        runCurrent()
        assertEquals("That is not a valid address. Use the form name@server.example.", vm.error.value)
    }
}

package space.foid.chord.viewmodel

import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import space.foid.chord.data.ChatApi
import space.foid.chord.data.TimelineTarget
import uniffi.chord_ffi.ClientEvent
import uniffi.chord_ffi.TimelineDiff

/** The typing line and the NEW line of [TimelineViewModel]. */
@OptIn(ExperimentalCoroutinesApi::class)
class TimelineChromeViewModelTest {
    private val room = TimelineTarget.Room("room@muc.example.org")
    private val events = MutableSharedFlow<ClientEvent>(extraBufferCapacity = 16)

    private fun TestScope.vm(target: TimelineTarget = room, api: FakeChatApi = FakeChatApi()) =
        TimelineViewModel(
            target, MutableStateFlow<ChatApi?>(api), backgroundScope,
            events = events, typersExpireMillis = 1_000,
        ) to api

    @Test
    fun typersFollowTheEventsOfThisChatOnly() = runTest {
        val (vm, _) = vm()
        runCurrent()
        events.emit(ClientEvent.Typing("room@muc.example.org", listOf("Bay")))
        events.emit(ClientEvent.Typing("other@muc.example.org", listOf("Jo")))
        runCurrent()
        assertEquals(listOf("Bay"), vm.typers.value)
        events.emit(ClientEvent.Typing("room@muc.example.org", emptyList()))
        runCurrent()
        assertEquals(emptyList<String>(), vm.typers.value)
    }

    @Test
    fun aPrivateChatMatchesRoomSlashNick() = runTest {
        val (vm, _) = vm(TimelineTarget.Private("room@muc.example.org", "bay"))
        runCurrent()
        events.emit(ClientEvent.Typing("room@muc.example.org/bay", listOf("bay")))
        runCurrent()
        assertEquals(listOf("bay"), vm.typers.value)
    }

    @Test
    fun typersClearByThemselvesWhenTheStopEventIsLost() = runTest {
        val (vm, _) = vm()
        runCurrent()
        events.emit(ClientEvent.Typing("room@muc.example.org", listOf("Bay")))
        runCurrent()
        advanceTimeBy(999)
        assertEquals(listOf("Bay"), vm.typers.value)
        advanceTimeBy(2)
        assertEquals(emptyList<String>(), vm.typers.value)
    }

    @Test
    fun theNewLineGoesAtTheNthIncomingMessageFromTheEnd() = runTest {
        val (vm, api) = vm()
        runCurrent()
        vm.openWithUnread(2)
        runCurrent()
        // Not loaded yet: no line.
        assertNull(vm.newFrom.value)
        api.timelines[0].sink(TimelineDiff.Reset(listOf(msg("1"), msg("2"), msg("3"))))
        advanceTimeBy(20)
        runCurrent()
        assertEquals("2", vm.newFrom.value)
    }

    @Test
    fun zeroUnreadClearsTheLine() = runTest {
        val (vm, api) = vm()
        runCurrent()
        api.timelines[0].sink(TimelineDiff.Reset(listOf(msg("1"), msg("2"))))
        advanceTimeBy(20)
        runCurrent()
        vm.openWithUnread(1)
        runCurrent()
        assertEquals("2", vm.newFrom.value)
        vm.openWithUnread(0)
        runCurrent()
        assertNull(vm.newFrom.value)
    }
}

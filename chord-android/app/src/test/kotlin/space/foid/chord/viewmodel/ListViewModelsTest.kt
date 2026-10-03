package space.foid.chord.viewmodel

import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.chord_ffi.ChannelDiff
import uniffi.chord_ffi.ChannelScope
import uniffi.chord_ffi.MemberDiff
import uniffi.chord_ffi.SpaceDiff
import uniffi.chord_ffi.SpaceItem

@OptIn(ExperimentalCoroutinesApi::class)
class ListViewModelsTest {
    @Test
    fun channelListFollowsTheScope() = runTest {
        val api = FakeChatApi()
        val vm = ChannelListViewModel(MutableStateFlow(api), scope = backgroundScope)
        runCurrent()
        assertEquals(ChannelScope.Home, api.channelSubs[0].param)
        api.channelSubs[0].sink(ChannelDiff.Reset(listOf(chan("a@x"), chan("b@x"))))
        advanceTimeBy(20)
        runCurrent()
        assertEquals(listOf("a@x", "b@x"), vm.channels.value.map { it.jid })

        val space = ChannelScope.Space("svc", "node")
        vm.setScope(space)
        // The old rows are gone at once.
        assertTrue(vm.channels.value.isEmpty())
        runCurrent()
        assertTrue(api.channelSubs[0].closed)
        assertEquals(space, api.channelSubs[1].param)
        api.channelSubs[1].sink(ChannelDiff.Reset(listOf(chan("c@x"))))
        advanceTimeBy(20)
        runCurrent()
        assertEquals(listOf("c@x"), vm.channels.value.map { it.jid })
        assertEquals(space, vm.currentScope.value)
    }

    @Test
    fun settingTheSameScopeKeepsTheSubscription() = runTest {
        val api = FakeChatApi()
        val vm = ChannelListViewModel(MutableStateFlow(api), scope = backgroundScope)
        runCurrent()
        vm.setScope(ChannelScope.Home)
        runCurrent()
        assertEquals(1, api.channelSubs.size)
        vm.setScope(ChannelScope.Space("a", "b"))
        vm.setScope(ChannelScope.Space("a", "b"))
        runCurrent()
        assertEquals(2, api.channelSubs.size)
    }

    @Test
    fun channelDiffsApplyInOrder() = runTest {
        val api = FakeChatApi()
        val vm = ChannelListViewModel(MutableStateFlow(api), scope = backgroundScope)
        runCurrent()
        val sink = api.channelSubs[0].sink
        sink(ChannelDiff.Reset(listOf(chan("a"), chan("c"))))
        sink(ChannelDiff.Insert(1u, chan("b")))
        sink(ChannelDiff.Remove(0u))
        advanceTimeBy(20)
        runCurrent()
        assertEquals(listOf("b", "c"), vm.channels.value.map { it.jid })
    }

    @Test
    fun spaceList() = runTest {
        val api = FakeChatApi()
        val vm = SpaceListViewModel(MutableStateFlow(api), backgroundScope)
        runCurrent()
        val item = SpaceItem("svc", "n", "Name", null)
        api.spaceSubs[0].sink(SpaceDiff.Reset(listOf(item)))
        advanceTimeBy(20)
        runCurrent()
        assertEquals(listOf(item), vm.spaces.value)
    }

    @Test
    fun memberListOpensTheRoom() = runTest {
        val api = FakeChatApi()
        val vm = MemberListViewModel("room@muc.x", MutableStateFlow(api), backgroundScope)
        runCurrent()
        assertEquals("room@muc.x", api.memberSubs[0].param)
        api.memberSubs[0].sink(MemberDiff.Reset(emptyList()))
        advanceTimeBy(20)
        runCurrent()
        assertTrue(vm.loaded.value)
        assertTrue(vm.members.value.isEmpty())
    }
}

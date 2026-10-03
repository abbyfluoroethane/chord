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
import space.foid.chord.ui.spaces.SpaceTarget
import uniffi.chord_ffi.ChordException
import uniffi.chord_ffi.NotificationLevel
import uniffi.chord_ffi.SpaceAccess
import uniffi.chord_ffi.SpaceItem

@OptIn(ExperimentalCoroutinesApi::class)
class SpaceViewModelTest {
    private val target = SpaceTarget("pubsub.example.org", "club", "Club")
    private val general = "club-general@conference.example.org"
    private val random = "club-random@conference.example.org"

    private fun api() = FakeSpaceApi().apply { channelList = listOf(room(general, unread = 2), room(random)) }

    private fun TestScope.vm(api: FakeSpaceApi?) = SpaceViewModel({ api }, backgroundScope)

    private fun events(vm: SpaceViewModel, scope: CoroutineScope): MutableList<SpaceEvent> {
        val got = ArrayList<SpaceEvent>()
        scope.launch(UnconfinedTestDispatcher()) { vm.events.collect { got += it } }
        return got
    }

    private fun TestScope.opened(api: FakeSpaceApi = api()): Pair<SpaceViewModel, FakeSpaceApi> {
        val v = vm(api)
        v.open(target)
        runCurrent()
        return v to api
    }

    @Test
    fun openReadsChannelsLevelAndOwner() = runTest {
        val api = api().apply { levels[general] = NotificationLevel.MENTIONS; levels[random] = NotificationLevel.MENTIONS }
        val (vm, _) = opened(api)
        val s = vm.state.value
        assertEquals(2, s.rooms.size)
        assertEquals(NotificationLevel.MENTIONS, s.level)
        assertEquals(true, s.owner)
    }

    @Test
    fun refusedMembersMeanNotOwner() = runTest {
        val (vm, _) = opened(api().apply { owner = false })
        assertEquals(false, vm.state.value.owner)
    }

    @Test
    fun mixedLevelsShowAll() = runTest {
        val api = api().apply { levels[general] = NotificationLevel.NONE }
        val (vm, _) = opened(api)
        assertEquals(NotificationLevel.ALL, vm.state.value.level)
    }

    @Test
    fun closeClearsTheState() = runTest {
        val (vm, _) = opened()
        vm.close()
        assertNull(vm.state.value.target)
    }

    @Test
    fun invitesAddTheMemberThenMessageTheLink() = runTest {
        val (vm, api) = opened()
        val got = events(vm, backgroundScope)
        var sent = -1
        vm.invite(listOf("rin@example.org", "sam@example.org")) { sent = it }
        runCurrent()
        assertEquals(2, sent)
        assertTrue(api.calls.contains("addMember club rin@example.org"))
        assertTrue(api.calls.contains("send rin@example.org Join Club on Chord: xmpp:pubsub.example.org?;node=club"))
        assertEquals(listOf<SpaceEvent>(SpaceEvent.Message("Sent 2 invites.")), got)
    }

    @Test
    fun inviteFailureStaysOpenWithAnError() = runTest {
        val (vm, api) = opened()
        api.failWith = ChordException.NotConnected()
        var sent = -1
        vm.invite(listOf("rin@example.org")) { sent = it }
        runCurrent()
        assertEquals(-1, sent)
        assertNotNull(vm.state.value.error)
        assertFalse(vm.state.value.busy)
    }

    @Test
    fun createChannelJoinsNamesAddsAndOpens() = runTest {
        val (vm, api) = opened()
        val got = events(vm, backgroundScope)
        var done = false
        vm.createChannel("  Game Night ") { done = true }
        runCurrent()
        val room = "club-game-night@conference.example.org"
        assertEquals(
            listOf("joinRoom $room nick=me", "configureRoom $room game-night", "addRoom club $room game-night"),
            api.calls.takeLast(3),
        )
        assertTrue(done)
        assertEquals(listOf<SpaceEvent>(SpaceEvent.OpenChannel(room, "game-night")), got)
    }

    @Test
    fun createChannelWithoutOwnerRightsStillWorksWithANote() = runTest {
        val (vm, api) = opened(api().apply { owner = false })
        val got = events(vm, backgroundScope)
        vm.createChannel("lobby") {}
        runCurrent()
        assertTrue(api.calls.last().startsWith("addRoom club club-lobby@"))
        assertTrue(got.first() is SpaceEvent.Message)
        assertTrue(got.last() is SpaceEvent.OpenChannel)
    }

    @Test
    fun createChannelNeedsLettersOrDigits() = runTest {
        val (vm, api) = opened()
        val before = api.calls.size
        vm.createChannel(" !!! ") {}
        runCurrent()
        assertEquals(before, api.calls.size)
        assertNotNull(vm.state.value.error)
    }

    @Test
    fun nicknameGoesToEveryJoinedRoom() = runTest {
        val api = api().apply { channelList = listOf(room(general), room(random), room("club-old@conference.example.org", joined = false)) }
        val (vm, _) = opened(api)
        var done = false
        vm.changeNick(" Abby ") { done = true }
        runCurrent()
        assertTrue(done)
        assertEquals(listOf("nick $general Abby", "nick $random Abby"), api.calls.filter { it.startsWith("nick") })
    }

    @Test
    fun nicknameFailsWhenEveryRoomRefuses() = runTest {
        val api = api().apply { failRooms += listOf(general, random) }
        val (vm, _) = opened(api)
        var done = false
        vm.changeNick("Abby") { done = true }
        runCurrent()
        assertFalse(done)
        assertNotNull(vm.state.value.error)
    }

    @Test
    fun nicknamePartlyDoneSaysSo() = runTest {
        val api = api().apply { failRooms += random }
        val (vm, _) = opened(api)
        val got = events(vm, backgroundScope)
        var done = false
        vm.changeNick("Abby") { done = true }
        runCurrent()
        assertTrue(done)
        assertEquals(listOf<SpaceEvent>(SpaceEvent.Message("The nickname changed in 1 of 2 channels.")), got)
    }

    @Test
    fun spaceLevelGoesToEveryRoom() = runTest {
        val (vm, api) = opened()
        var done = false
        vm.setLevel(NotificationLevel.NONE) { done = true }
        runCurrent()
        assertTrue(done)
        assertEquals(NotificationLevel.NONE, vm.state.value.level)
        assertEquals(listOf("notify $general NONE until=null", "notify $random NONE until=null"), api.calls.filter { it.startsWith("notify") })
    }

    @Test
    fun markSpaceReadMarksOnlyUnreadRooms() = runTest {
        val (vm, api) = opened()
        vm.markSpaceRead(target)
        runCurrent()
        assertEquals(listOf("markRead $general"), api.calls.filter { it.startsWith("markRead") })
    }

    @Test
    fun markEverySpaceReadWalksAllScopes() = runTest {
        val api = api().apply { homeList = listOf(room("rin@example.org", unread = 3)) }
        val vm = vm(api)
        vm.markAllRead(listOf(SpaceItem("pubsub.example.org", "club", "Club", null)), includeHome = true)
        runCurrent()
        assertEquals(listOf("markRead rin@example.org", "markRead $general"), api.calls.filter { it.startsWith("markRead") })
    }

    @Test
    fun leaveLeavesRoomsThenTheSpaceAndTellsTheScreen() = runTest {
        val (vm, api) = opened()
        val got = events(vm, backgroundScope)
        var done = false
        vm.leaveSpace { done = true }
        runCurrent()
        assertTrue(done)
        assertEquals(listOf("leaveRoom $general", "leaveRoom $random", "leaveSpace club"), api.calls.takeLast(3))
        assertEquals(listOf<SpaceEvent>(SpaceEvent.SpaceGone("pubsub.example.org", "club", listOf(general, random))), got)
    }

    @Test
    fun leaveStillLeavesTheSpaceWhenARoomFails() = runTest {
        val api = api().apply { failRooms += general }
        val (vm, _) = opened(api)
        val got = events(vm, backgroundScope)
        vm.leaveSpace {}
        runCurrent()
        assertEquals("leaveSpace club", api.calls.last())
        assertTrue(got.any { it is SpaceEvent.Message })
    }

    @Test
    fun settingsLoadAndSaveOnlyTheChange() = runTest {
        val (vm, api) = opened()
        vm.loadSettings()
        runCurrent()
        assertEquals("About it", vm.state.value.description)
        assertEquals(1, vm.state.value.requests.size)
        var done = false
        vm.saveSettings("Club", "About it") { done = true }
        runCurrent()
        assertTrue(done)
        assertTrue(api.calls.none { it.startsWith("configureSpace") })
        vm.saveSettings("Club 2", "About it") {}
        runCurrent()
        assertEquals("configureSpace club name=Club 2 description=null", api.calls.last())
    }

    @Test
    fun settingsOfANonOwnerStayReadOnly() = runTest {
        val (vm, _) = opened(api().apply { owner = false })
        vm.loadSettings()
        runCurrent()
        assertEquals(false, vm.state.value.owner)
        assertTrue(vm.state.value.settingsLoaded)
        assertTrue(vm.state.value.requests.isEmpty())
    }

    @Test
    fun removeAndBanMembers() = runTest {
        val (vm, api) = opened()
        vm.removeMember("pat@example.org", ban = false)
        runCurrent()
        vm.removeMember("pat@example.org", ban = true)
        runCurrent()
        assertTrue(api.calls.contains("removeMember club pat@example.org"))
        assertTrue(api.calls.contains("banMember club pat@example.org"))
    }

    @Test
    fun answeringARequestDropsItFromTheList() = runTest {
        val (vm, api) = opened()
        vm.loadSettings()
        runCurrent()
        vm.answerRequest("pat@example.org", approve = true)
        runCurrent()
        assertTrue(vm.state.value.requests.isEmpty())
        assertEquals("answer club pat@example.org approve=true", api.calls.last())
    }

    @Test
    fun removeChannelKeepsTheRoomButDropsTheRow() = runTest {
        val (vm, api) = opened()
        vm.removeChannel(random)
        runCurrent()
        assertEquals("removeRoom club $random", api.calls.last())
        assertEquals(listOf(general), vm.state.value.rooms.map { it.jid })
    }

    @Test
    fun deleteTellsTheScreen() = runTest {
        val (vm, api) = opened()
        val got = events(vm, backgroundScope)
        var done = false
        vm.deleteSpace { done = true }
        runCurrent()
        assertTrue(done)
        assertEquals("deleteSpace club", api.calls.last())
        assertTrue(got.single() is SpaceEvent.SpaceGone)
    }

    @Test
    fun imageGoesToTheRightCall() = runTest {
        val (vm, api) = opened()
        vm.setImage(banner = true, mime = "image/png", data = ByteArray(10), width = 800, height = 200)
        runCurrent()
        assertEquals("image club banner=true image/png 10 800x200", api.calls.last())
    }

    @Test
    fun createSpaceOpensIt() = runTest {
        val api = api()
        val vm = vm(api)
        val got = events(vm, backgroundScope)
        var done = false
        vm.createSpace("  Rustaceans ", " ", SpaceAccess.AUTHORIZE) { done = true }
        runCurrent()
        assertTrue(done)
        assertEquals("createSpace Rustaceans description=null access=AUTHORIZE", api.calls.last())
        assertEquals(listOf<SpaceEvent>(SpaceEvent.OpenSpace("pubsub.example.org", "new-space")), got)
    }

    @Test
    fun createSpaceNeedsAName() = runTest {
        val api = api()
        val vm = vm(api)
        vm.createSpace("   ", "", SpaceAccess.OPEN) {}
        runCurrent()
        assertTrue(api.calls.isEmpty())
    }

    @Test
    fun signedOutGivesAnError() = runTest {
        val vm = vm(null)
        vm.open(target)
        runCurrent()
        assertEquals("You are not connected to the server.", vm.state.value.error)
    }

    @Test
    fun serverRefusalHasPlainText() {
        assertEquals("Only the owner of this space can do this.", describeSpaceError(ChordException.Server("forbidden")))
        assertEquals("This server does not let you do this.", describeSpaceError(ChordException.Server("not-allowed")))
        assertEquals("Someone already uses this name.", describeSpaceError(ChordException.Server("conflict")))
    }
}

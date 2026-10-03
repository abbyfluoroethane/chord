package space.foid.chord.ui.profile

import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import space.foid.chord.ui.components.Presence
import space.foid.chord.ui.screens.DrawerFixtures
import space.foid.chord.ui.screens.groupMembers
import space.foid.chord.ui.screens.isModerator
import space.foid.chord.viewmodel.ProfileApi
import space.foid.chord.viewmodel.ProfileNotice
import space.foid.chord.viewmodel.ProfileState
import space.foid.chord.viewmodel.ProfileSubject
import space.foid.chord.viewmodel.ProfileViewModel
import space.foid.chord.viewmodel.presenceFor
import uniffi.chord_ffi.Contact
import uniffi.chord_ffi.Profile
import uniffi.chord_ffi.SpaceItem
import uniffi.chord_ffi.SpaceMember
import uniffi.chord_ffi.SubscriptionState

class FakeProfileApi(
    var me: String = "me@chord.localhost",
    var contacts: List<Contact> = emptyList(),
    var blocked: List<String> = emptyList(),
    var members: Map<String, List<String>> = emptyMap(),
    var fail: Boolean = false,
) : ProfileApi {
    val calls = ArrayList<String>()
    override fun account() = me
    override suspend fun profile(jid: String) = Profile("nick", "Full Name")
    override suspend fun contacts() = contacts
    override suspend fun blocked() = blocked
    private fun rec(s: String) {
        calls += s
        if (fail) error("boom")
    }
    override suspend fun addContact(jid: String, name: String?) { rec("add $jid"); contacts = contacts + contact(jid) }
    override suspend fun removeContact(jid: String) { rec("remove $jid"); contacts = contacts.filter { it.jid != jid } }
    override suspend fun renameContact(jid: String, name: String?) = rec("rename $jid $name")
    override suspend fun block(jid: String) { rec("block $jid"); blocked = blocked + jid }
    override suspend fun unblock(jid: String) { rec("unblock $jid"); blocked = blocked - jid }
    override suspend fun spaceMembers(space: SpaceItem) = (members[space.node] ?: error("no access")).map { SpaceMember(it, "member") }
    override suspend fun inviteToSpace(space: SpaceItem, jid: String) = rec("invite ${space.node} $jid")

    companion object {
        fun contact(jid: String, name: String? = null) = Contact(
            jid, name, SubscriptionState.BOTH, false, emptyList(), false, false, true, null, "On the range", null, null,
        )
    }
}

@OptIn(ExperimentalCoroutinesApi::class)
class ProfileLogicTest {
    private val bob = "bob@chord.localhost"
    private fun TestScope.vm(api: FakeProfileApi, subject: ProfileSubject = ProfileSubject(bob, "Bob")): ProfileViewModel {
        val vm = ProfileViewModel(subject, api, backgroundScope)
        runCurrent()
        return vm
    }

    @Test
    fun membersGroupLikeTheDesktop() {
        val groups = groupMembers(DrawerFixtures.members)
        assertEquals(listOf("Owners", "Admins", "Online", "Visitors", "Offline"), groups.map { it.label })
        assertEquals(listOf(1, 1, 2, 1, 2), groups.map { it.items.size })
        assertEquals(listOf("owners", "admins", "online", "visitors", "offline"), groups.map { it.key })
    }

    @Test
    fun emptyGroupsAreDropped() {
        val only = DrawerFixtures.members.filter { !it.online }
        assertEquals(listOf("Offline"), groupMembers(only).map { it.label })
        assertTrue(groupMembers(emptyList()).isEmpty())
    }

    @Test
    fun onlyModeratorsGetTheShield() {
        val m = DrawerFixtures.members
        assertTrue(isModerator(m[0]))
        assertFalse(isModerator(m[2]))
    }

    @Test
    fun presenceFollowsShow() {
        assertEquals(Presence.Offline, presenceFor(false, "away"))
        assertEquals(Presence.Dnd, presenceFor(true, "dnd"))
        assertEquals(Presence.Away, presenceFor(true, "xa"))
        assertEquals(Presence.Online, presenceFor(true, null))
    }

    @Test
    fun findsAMemberByIdOrAddress() {
        val m = DrawerFixtures.members
        assertEquals("alice", findMember(m, "alice")?.id)
        assertEquals("bob", findMember(m, "bob@chord.localhost/phone")?.id)
        assertNull(findMember(m, "zed@chord.localhost"))
    }

    @Test
    fun menuPlanForSelfStrangerAndContact() {
        val stranger = ProfileState(bob, "Bob", invitable = listOf(SpaceItem("s", "n", "N", null)))
        val p = personMenuPlan(stranger, showMessage = true)
        assertTrue(p.message && p.invite && p.contact && p.block && p.copy)
        assertFalse(p.rename)
        assertTrue(personMenuPlan(stranger.copy(isContact = true), true).rename)
        assertFalse(personMenuPlan(stranger, showMessage = false).message)
        val me = personMenuPlan(stranger.copy(isMe = true), true)
        assertFalse(me.message || me.invite || me.contact || me.block)
        val hidden = personMenuPlan(stranger.copy(hidden = true), true)
        assertFalse(hidden.block || hidden.contact || hidden.copy || hidden.invite)
    }

    @Test
    fun loadsContactAndBlockState() = runTest {
        val api = FakeProfileApi(contacts = listOf(FakeProfileApi.contact(bob, "Bobby")), blocked = listOf(bob))
        val s = vm(api).state.value
        assertTrue(s.isContact)
        assertTrue(s.isBlocked)
        assertFalse(s.isMe)
        assertEquals("Bobby", s.name)
        assertEquals(Presence.Online, s.presence)
        assertEquals("Full Name", s.fullName)
    }

    @Test
    fun ownProfileIsMe() = runTest {
        val s = vm(FakeProfileApi(), ProfileSubject("me@chord.localhost", "Me")).state.value
        assertTrue(s.isMe)
        assertNull(s.fullName)
    }

    @Test
    fun hiddenOccupantAsksForNothing() = runTest {
        val api = FakeProfileApi()
        val s = vm(api, ProfileSubject("room@muc.chord.localhost/Zed", "Zed")).state.value
        assertTrue(s.hidden)
        assertFalse(s.isMe)
        assertEquals("Zed", s.name)
    }

    @Test
    fun addAndBlockUpdateState() = runTest {
        val api = FakeProfileApi()
        val vm = vm(api)
        vm.addContact()
        runCurrent()
        assertTrue(vm.state.value.isContact)
        assertEquals(ProfileNotice.ContactAdded, vm.notice.value)
        vm.block()
        runCurrent()
        assertTrue(vm.state.value.isBlocked)
        vm.unblock()
        runCurrent()
        assertFalse(vm.state.value.isBlocked)
        assertEquals(listOf("add $bob", "block $bob", "unblock $bob"), api.calls)
    }

    @Test
    fun failedActionShowsFailedNotice() = runTest {
        val api = FakeProfileApi(fail = true)
        val vm = vm(api)
        vm.addContact()
        runCurrent()
        assertEquals(ProfileNotice.Failed, vm.notice.value)
        assertFalse(vm.state.value.busy)
    }

    @Test
    fun sharedSpacesComeFromSpaceMembers() = runTest {
        val a = SpaceItem("p", "a", "Alpha", null)
        val b = SpaceItem("p", "b", "Beta", null)
        val c = SpaceItem("p", "c", "Closed", null)
        val api = FakeProfileApi(members = mapOf("a" to listOf(bob), "b" to listOf("x@y.z")))
        val vm = vm(api)
        vm.setSpaces(listOf(a, b, c))
        runCurrent()
        assertEquals(listOf(a), vm.state.value.sharedSpaces)
        // Beta and Closed: not a member, and unknown (no access). Both can get an invite.
        assertEquals(listOf(b, c), vm.state.value.invitable)
        vm.invite(b)
        runCurrent()
        assertEquals(ProfileNotice.Invited, vm.notice.value)
        assertEquals("invite b $bob", api.calls.last())
    }
}

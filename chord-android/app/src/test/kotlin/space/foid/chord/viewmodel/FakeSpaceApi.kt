package space.foid.chord.viewmodel

import space.foid.chord.data.SpaceApi
import uniffi.chord_ffi.ChannelItem
import uniffi.chord_ffi.ChannelKind
import uniffi.chord_ffi.ChannelScope
import uniffi.chord_ffi.Contact
import uniffi.chord_ffi.JoinRequest
import uniffi.chord_ffi.NotificationLevel
import uniffi.chord_ffi.NotificationSetting
import uniffi.chord_ffi.SpaceAccess
import uniffi.chord_ffi.SpaceMember
import uniffi.chord_ffi.SpaceRef

fun room(jid: String, name: String = jid.substringBefore('@'), unread: Int = 0, joined: Boolean = true) = ChannelItem(
    jid = jid, name = name, kind = ChannelKind.Room, category = null, joined = joined, lastActivity = null,
    unread = unread.toUInt(), blocked = false, members = null,
)

/** A [SpaceApi] that records the calls. Set [failWith] to make the calls fail. */
class FakeSpaceApi : SpaceApi {
    val calls = ArrayList<String>()
    var failWith: Exception? = null
    var owner = true
    var channelList: List<ChannelItem> = emptyList()
    var homeList: List<ChannelItem> = emptyList()
    var contactList: List<Contact> = emptyList()
    var members: List<SpaceMember> = listOf(SpaceMember("me@example.org", "owner"))
    var levels = HashMap<String, NotificationLevel>()
    val failRooms = HashSet<String>()

    private fun rec(call: String) {
        failWith?.let { throw it }
        calls += call
    }

    override suspend fun channels(scope: ChannelScope): List<ChannelItem> =
        (if (scope is ChannelScope.Home) homeList else channelList).also { rec("channels $scope") }

    override suspend fun contacts(): List<Contact> = contactList.also { rec("contacts") }

    override suspend fun spaceMembers(service: String, node: String): List<SpaceMember> {
        rec("members $node")
        if (!owner) throw uniffi.chord_ffi.ChordException.Server("forbidden")
        return members
    }

    override suspend fun addSpaceMember(service: String, node: String, jid: String) = rec("addMember $node $jid")
    override suspend fun removeSpaceMember(service: String, node: String, jid: String) = rec("removeMember $node $jid")
    override suspend fun banSpaceMember(service: String, node: String, jid: String) = rec("banMember $node $jid")
    override suspend fun spaceDescription(service: String, node: String): String = "About it".also { rec("description $node") }
    override suspend fun configureSpace(service: String, node: String, name: String?, description: String?) =
        rec("configureSpace $node name=$name description=$description")

    override suspend fun setSpaceImage(service: String, node: String, banner: Boolean, mime: String, data: ByteArray, width: Int, height: Int) =
        rec("image $node banner=$banner $mime ${data.size} ${width}x$height")

    override suspend fun joinRequests(service: String, node: String): List<JoinRequest> {
        rec("requests $node")
        if (!owner) throw uniffi.chord_ffi.ChordException.Server("forbidden")
        return listOf(JoinRequest("pat@example.org", null))
    }

    override suspend fun answerJoinRequest(service: String, node: String, jid: String, approve: Boolean) =
        rec("answer $node $jid approve=$approve")

    override suspend fun deleteSpace(service: String, node: String) = rec("deleteSpace $node")
    override suspend fun leaveSpace(service: String, node: String) = rec("leaveSpace $node")
    override suspend fun createSpace(name: String, description: String?, access: SpaceAccess): SpaceRef {
        rec("createSpace $name description=$description access=$access")
        return SpaceRef("pubsub.example.org", "new-space")
    }

    var failJoin = false

    override suspend fun joinRoom(room: String, nick: String) {
        if (failJoin) throw uniffi.chord_ffi.ChordException.Server("forbidden")
        rec("joinRoom $room nick=$nick")
    }
    override suspend fun addRoomToSpace(service: String, node: String, room: String, name: String) = rec("addRoom $node $room $name")
    override suspend fun removeRoomFromSpace(service: String, node: String, room: String) = rec("removeRoom $node $room")
    override suspend fun changeNick(room: String, nick: String) {
        if (room in failRooms) throw uniffi.chord_ffi.ChordException.Server("conflict")
        rec("nick $room $nick")
    }

    override suspend fun leaveRoom(room: String) {
        if (room in failRooms) throw uniffi.chord_ffi.ChordException.Server("item-not-found")
        rec("leaveRoom $room")
    }

    override suspend fun setRoomSubject(room: String, subject: String) = rec("subject $room $subject")
    override suspend fun configureRoom(room: String, name: String) {
        if (!owner) throw uniffi.chord_ffi.ChordException.Server("forbidden")
        rec("configureRoom $room $name")
    }

    override suspend fun sendMessage(to: String, body: String) = rec("send $to $body")
    override suspend fun markRead(jid: String) = rec("markRead $jid")
    override suspend fun notificationSetting(peer: String): NotificationSetting =
        NotificationSetting(levels[peer] ?: NotificationLevel.ALL, null).also { rec("setting $peer") }

    override suspend fun setNotification(peer: String, level: NotificationLevel, muteUntil: Long?) {
        rec("notify $peer $level until=$muteUntil")
        levels[peer] = level
    }

    override fun account(): String = "me@example.org"
}

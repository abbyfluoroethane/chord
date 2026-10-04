package space.foid.chord.viewmodel

import space.foid.chord.data.ConversationApi
import uniffi.chord_ffi.Contact
import uniffi.chord_ffi.JoinOutcome
import uniffi.chord_ffi.NotificationLevel
import uniffi.chord_ffi.NotificationSetting
import uniffi.chord_ffi.PendingSpaceJoin
import uniffi.chord_ffi.SpaceInfo
import uniffi.chord_ffi.SubscriptionState

fun contact(jid: String, name: String? = null) = Contact(
    jid = jid, name = name, subscription = SubscriptionState.BOTH, ask = false, groups = emptyList(),
    approved = false, blocked = false, online = false, show = null, status = null, idleSince = null, activity = null,
)

/** A [ConversationApi] that records the calls. Set [failWith] to make the next calls fail. */
class FakeConversationApi : ConversationApi {
    val calls = ArrayList<String>()
    var failWith: Exception? = null
    var joinFailures: ArrayDeque<Exception> = ArrayDeque()
    var contactList: List<Contact> = emptyList()
    var spaceList: List<SpaceInfo> = emptyList()
    var pending: List<PendingSpaceJoin> = emptyList()
    var spaceOutcome = JoinOutcome.JOINED
    var level = NotificationLevel.ALL
    var own = "me@example.org"

    private fun rec(call: String) {
        failWith?.let { throw it }
        calls += call
    }

    override suspend fun joinRoom(room: String, nick: String?, password: String?) {
        calls += "join $room nick=$nick password=$password"
        joinFailures.removeFirstOrNull()?.let { throw it }
        failWith?.let { throw it }
    }

    override suspend fun leaveRoom(room: String) = rec("leave $room")
    override suspend fun contacts(): List<Contact> = contactList.also { rec("contacts") }
    override suspend fun addContact(jid: String, name: String?) = rec("addContact $jid name=$name")
    override suspend fun removeContact(jid: String) = rec("removeContact $jid")
    override suspend fun approveSubscription(jid: String, addBack: Boolean) = rec("approve $jid addBack=$addBack")
    override suspend fun denySubscription(jid: String) = rec("deny $jid")
    override suspend fun declineRoomInvite(room: String, from: String) = rec("decline $room $from")
    override suspend fun browseSpaces(): List<SpaceInfo> = spaceList.also { rec("browse") }
    override suspend fun joinSpace(service: String, node: String): JoinOutcome = spaceOutcome.also { rec("joinSpace $service $node") }
    override suspend fun pendingSpaceJoins(): List<PendingSpaceJoin> = pending.also { rec("pending") }
    override suspend fun notificationLevel(peer: String): NotificationLevel = level.also { rec("level $peer") }
    override suspend fun setNotificationLevel(peer: String, level: NotificationLevel) = rec("setLevel $peer $level")
    override suspend fun markRead(jid: String) = rec("markRead $jid")
    var muteUntil: Long? = null
    override suspend fun notificationSetting(peer: String): NotificationSetting =
        NotificationSetting(level, muteUntil).also { rec("level $peer") }

    override suspend fun setNotification(peer: String, level: NotificationLevel, muteUntil: Long?) {
        rec(if (muteUntil == null) "setLevel $peer $level" else "mute $peer $level until=$muteUntil")
        this.level = level
        this.muteUntil = muteUntil
    }

    override suspend fun setRoomSubject(room: String, subject: String) = rec("subject $room $subject")
    override suspend fun renameRoom(room: String, name: String) = rec("rename $room $name")
    override fun account(): String = own
}

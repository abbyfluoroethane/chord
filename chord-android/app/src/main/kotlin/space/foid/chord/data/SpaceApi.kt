package space.foid.chord.data

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.withTimeout
import uniffi.chord_ffi.ChannelDiff
import uniffi.chord_ffi.ChannelItem
import uniffi.chord_ffi.ChannelListListener
import uniffi.chord_ffi.ChannelScope
import uniffi.chord_ffi.ChordClient
import uniffi.chord_ffi.Contact
import uniffi.chord_ffi.JoinRequest
import uniffi.chord_ffi.NotificationLevel
import uniffi.chord_ffi.NotificationSetting
import uniffi.chord_ffi.RoomSettings
import uniffi.chord_ffi.SpaceAccess
import uniffi.chord_ffi.SpaceMember
import uniffi.chord_ffi.SpaceRef

/**
 * The calls of the space menus, the space dialogs and the channel menus. It is a small
 * interface over [ChordClient], like [ChatApi], so that [space.foid.chord.viewmodel.SpaceViewModel]
 * runs in JVM tests without the native library. It holds no protocol logic.
 */
interface SpaceApi {
    /** The channels of Home or of one space now. One read: no subscription stays open. */
    suspend fun channels(scope: ChannelScope): List<ChannelItem>

    suspend fun contacts(): List<Contact>

    // The space and its people. The owner can read and change all of it.
    suspend fun spaceMembers(service: String, node: String): List<SpaceMember>
    suspend fun addSpaceMember(service: String, node: String, jid: String)
    suspend fun removeSpaceMember(service: String, node: String, jid: String)
    suspend fun banSpaceMember(service: String, node: String, jid: String)
    suspend fun spaceDescription(service: String, node: String): String
    suspend fun configureSpace(service: String, node: String, name: String?, description: String?)
    suspend fun setSpaceImage(service: String, node: String, banner: Boolean, mime: String, data: ByteArray, width: Int, height: Int)
    suspend fun joinRequests(service: String, node: String): List<JoinRequest>
    suspend fun answerJoinRequest(service: String, node: String, jid: String, approve: Boolean)
    suspend fun deleteSpace(service: String, node: String)
    suspend fun leaveSpace(service: String, node: String)
    suspend fun createSpace(name: String, description: String?, access: SpaceAccess): SpaceRef

    // Channels.
    suspend fun joinRoom(room: String, nick: String)
    suspend fun addRoomToSpace(service: String, node: String, room: String, name: String)
    suspend fun removeRoomFromSpace(service: String, node: String, room: String)
    suspend fun changeNick(room: String, nick: String)
    suspend fun leaveRoom(room: String)
    suspend fun setRoomSubject(room: String, subject: String)

    /** Change the name of a room. Only an owner can. */
    suspend fun configureRoom(room: String, name: String)

    // Messages, read marks and notifications.
    suspend fun sendMessage(to: String, body: String)
    suspend fun markRead(jid: String)
    suspend fun notificationSetting(peer: String): NotificationSetting
    suspend fun setNotification(peer: String, level: NotificationLevel, muteUntil: Long?)

    /** The address of our account. */
    fun account(): String
}

/** [SpaceApi] on a [ChordClient]. */
class ClientSpaceApi(private val client: ChordClient) : SpaceApi {
    override suspend fun channels(scope: ChannelScope): List<ChannelItem> {
        val first = CompletableDeferred<List<ChannelItem>>()
        val sub = client.subscribeChannelList(
            scope,
            object : ChannelListListener {
                override fun onDiff(diff: ChannelDiff) {
                    if (diff is ChannelDiff.Reset) first.complete(diff.items)
                }
            },
        )
        try {
            return withTimeout(8_000) { first.await() }
        } finally {
            sub.cancel()
            sub.close()
        }
    }

    override suspend fun contacts(): List<Contact> = client.contacts()
    override suspend fun spaceMembers(service: String, node: String): List<SpaceMember> = client.spaceMembers(service, node)
    override suspend fun addSpaceMember(service: String, node: String, jid: String) = client.addSpaceMember(service, node, jid)
    override suspend fun removeSpaceMember(service: String, node: String, jid: String) = client.removeSpaceMember(service, node, jid)
    override suspend fun banSpaceMember(service: String, node: String, jid: String) = client.banSpaceMember(service, node, jid)

    override suspend fun spaceDescription(service: String, node: String): String =
        client.spaceConfig(service, node).firstOrNull { it.`var` == "pubsub#description" }?.value.orEmpty()

    override suspend fun configureSpace(service: String, node: String, name: String?, description: String?) =
        client.configureSpace(service, node, name, description)

    override suspend fun setSpaceImage(
        service: String, node: String, banner: Boolean, mime: String, data: ByteArray, width: Int, height: Int,
    ) {
        if (banner) client.setSpaceBanner(service, node, mime, data, width.toUShort(), height.toUShort())
        else client.setSpaceAvatar(service, node, mime, data, width.toUShort(), height.toUShort())
    }

    override suspend fun joinRequests(service: String, node: String): List<JoinRequest> = client.spaceJoinRequests(service, node)

    override suspend fun answerJoinRequest(service: String, node: String, jid: String, approve: Boolean) =
        if (approve) client.approveSpaceJoin(service, node, jid) else client.denySpaceJoin(service, node, jid)

    override suspend fun deleteSpace(service: String, node: String) = client.deleteSpace(service, node)
    override suspend fun leaveSpace(service: String, node: String) = client.leaveSpace(service, node)

    override suspend fun createSpace(name: String, description: String?, access: SpaceAccess): SpaceRef =
        client.createSpaceDescribed(name, description, access)

    override suspend fun joinRoom(room: String, nick: String) = client.joinRoom(room, nick, null)
    override suspend fun addRoomToSpace(service: String, node: String, room: String, name: String) =
        client.addRoomToSpace(service, node, room, name)

    override suspend fun removeRoomFromSpace(service: String, node: String, room: String) =
        client.removeRoomFromSpace(service, node, room)

    override suspend fun changeNick(room: String, nick: String) = client.changeNick(room, nick)
    override suspend fun leaveRoom(room: String) = client.leaveRoom(room)
    override suspend fun setRoomSubject(room: String, subject: String) = client.setRoomSubject(room, subject)
    override suspend fun configureRoom(room: String, name: String) = client.configureRoom(room, RoomSettings(name, null, null))

    override suspend fun sendMessage(to: String, body: String) {
        client.sendChat(to, body)
    }

    override suspend fun markRead(jid: String) =
        if ('/' in jid) client.markReadPrivate(jid.substringBefore('/'), jid.substringAfter('/')) else client.markRead(jid)

    override suspend fun notificationSetting(peer: String): NotificationSetting = client.notificationLevel(peer)
    override suspend fun setNotification(peer: String, level: NotificationLevel, muteUntil: Long?) =
        client.setNotificationLevel(peer, level, muteUntil)

    override fun account(): String = client.account()
}

/** The [SpaceApi] of the current client of the session, or null when nobody is signed in. */
fun ChordSession.spaceApi(): SpaceApi? = client.value?.let(::ClientSpaceApi)

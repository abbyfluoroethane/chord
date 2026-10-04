package space.foid.chord.data

import uniffi.chord_ffi.ChordClient
import uniffi.chord_ffi.Contact
import uniffi.chord_ffi.JoinOutcome
import uniffi.chord_ffi.NotificationLevel
import uniffi.chord_ffi.NotificationSetting
import uniffi.chord_ffi.RoomSettings
import uniffi.chord_ffi.PendingSpaceJoin
import uniffi.chord_ffi.SpaceInfo

/**
 * The calls that start, join and leave conversations, and that answer requests and invites.
 * It is a small interface over [ChordClient], like [ChatApi], so that the ViewModels run in
 * JVM tests without the native library. [ClientConversationApi] is the real one. It holds no
 * protocol logic.
 */
interface ConversationApi {
    /** Join a room. A blank [nick] means the default nick of the core. Then bookmark it. */
    suspend fun joinRoom(room: String, nick: String?, password: String?)

    /** Leave a room. The core also removes its bookmark. */
    suspend fun leaveRoom(room: String)

    suspend fun contacts(): List<Contact>
    suspend fun addContact(jid: String, name: String?)
    suspend fun removeContact(jid: String)

    suspend fun approveSubscription(jid: String, addBack: Boolean)
    suspend fun denySubscription(jid: String)
    suspend fun declineRoomInvite(room: String, from: String)

    suspend fun browseSpaces(): List<SpaceInfo>
    suspend fun joinSpace(service: String, node: String): JoinOutcome
    suspend fun pendingSpaceJoins(): List<PendingSpaceJoin>

    suspend fun notificationLevel(peer: String): NotificationLevel
    suspend fun setNotificationLevel(peer: String, level: NotificationLevel)
    suspend fun markRead(jid: String)

    /** The level and the end of a timed mute (Unix time in ms) of a chat. */
    suspend fun notificationSetting(peer: String): NotificationSetting

    /** Set the level. With [muteUntil] the chat is muted until that time and keeps its level. */
    suspend fun setNotification(peer: String, level: NotificationLevel, muteUntil: Long?)

    /** Set the topic of a room. An empty text clears it. */
    suspend fun setRoomSubject(room: String, subject: String)

    /** Change the name of a room that we own. */
    suspend fun renameRoom(room: String, name: String)

    /** The address of our own account. */
    fun account(): String
}

/** [ConversationApi] on a [ChordClient]. */
class ClientConversationApi(private val client: ChordClient) : ConversationApi {
    override suspend fun joinRoom(room: String, nick: String?, password: String?) {
        val own = nick?.trim().orEmpty()
        if (own.isEmpty()) client.joinRoomDefaultNick(room, password, null) else client.joinRoom(room, own, password)
        // A bookmark makes the room come back at the next sign-in. A failure here is not fatal.
        try {
            client.addBookmark(room, null, true, own.ifEmpty { null })
        } catch (e: Exception) {
            logWarn("ConversationApi", "bookmark failed for $room", e)
        }
    }

    override suspend fun leaveRoom(room: String) = client.leaveRoom(room)
    override suspend fun contacts(): List<Contact> = client.contacts()
    override suspend fun addContact(jid: String, name: String?) = client.addContact(jid, name)
    override suspend fun removeContact(jid: String) = client.removeContact(jid)

    override suspend fun approveSubscription(jid: String, addBack: Boolean) =
        if (addBack) client.approveSubscriptionAndAddBack(jid) else client.approveSubscription(jid)

    override suspend fun denySubscription(jid: String) = client.denySubscription(jid)
    override suspend fun declineRoomInvite(room: String, from: String) = client.declineRoomInvite(room, from, null)

    override suspend fun browseSpaces(): List<SpaceInfo> = client.browseSpaces()
    override suspend fun joinSpace(service: String, node: String): JoinOutcome = client.joinSpace(service, node)
    override suspend fun pendingSpaceJoins(): List<PendingSpaceJoin> = client.pendingSpaceJoins()

    override suspend fun notificationLevel(peer: String): NotificationLevel = client.notificationLevel(peer).level
    override suspend fun setNotificationLevel(peer: String, level: NotificationLevel) =
        client.setNotificationLevel(peer, level, null)

    override suspend fun markRead(jid: String) =
        if ('/' in jid) client.markReadPrivate(jid.substringBefore('/'), jid.substringAfter('/')) else client.markRead(jid)

    override suspend fun notificationSetting(peer: String): NotificationSetting = client.notificationLevel(peer)
    override suspend fun setNotification(peer: String, level: NotificationLevel, muteUntil: Long?) =
        client.setNotificationLevel(peer, level, muteUntil)

    override suspend fun setRoomSubject(room: String, subject: String) = client.setRoomSubject(room, subject)
    override suspend fun renameRoom(room: String, name: String) = client.configureRoom(room, RoomSettings(name, null, null))

    override fun account(): String = client.account()
}

/** The [ConversationApi] of the current client of the session, or null when nobody is signed in. */
fun ChordSession.conversationApi(): ConversationApi? = client.value?.let(::ClientConversationApi)

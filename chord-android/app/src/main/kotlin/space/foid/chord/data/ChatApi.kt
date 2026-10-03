package space.foid.chord.data

import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import uniffi.chord_ffi.ChannelDiff
import uniffi.chord_ffi.ChannelListListener
import uniffi.chord_ffi.ChannelScope
import uniffi.chord_ffi.ChordClient
import uniffi.chord_ffi.MemberDiff
import uniffi.chord_ffi.MemberListListener
import uniffi.chord_ffi.SpaceDiff
import uniffi.chord_ffi.SpaceListListener
import uniffi.chord_ffi.TimelineDiff
import uniffi.chord_ffi.TimelineListener

/** A running subscription. [close] stops it, and no diff arrives after it returns. */
interface ViewHandle : AutoCloseable

/** A running timeline subscription. It can also load older messages. */
interface TimelineHandle : ViewHandle {
    /** Show [count] older messages. The new items arrive as diffs. */
    suspend fun paginateBack(count: Int)
}

/** What a timeline shows: a room or 1:1 chat, or the private messages with one room occupant. */
sealed interface TimelineTarget {
    data class Room(val jid: String) : TimelineTarget
    data class Private(val room: String, val nick: String) : TimelineTarget
}

/**
 * The calls that the ViewModels make on the core. This is a small interface over
 * [ChordClient] so that the ViewModel logic runs in JVM tests without the native library.
 * [ClientChatApi] is the real implementation. It holds no protocol logic.
 *
 * Listener callbacks arrive on core threads.
 */
interface ChatApi {
    suspend fun spaces(sink: (SpaceDiff) -> Unit): ViewHandle
    suspend fun channels(scope: ChannelScope, sink: (ChannelDiff) -> Unit): ViewHandle
    suspend fun members(room: String, sink: (MemberDiff) -> Unit): ViewHandle
    suspend fun timeline(target: TimelineTarget, sink: (TimelineDiff) -> Unit): TimelineHandle

    suspend fun send(target: TimelineTarget, body: String)
    suspend fun reply(itemId: String, body: String)
    suspend fun edit(itemId: String, body: String)
    suspend fun retract(itemId: String)
    suspend fun toggleReaction(itemId: String, emoji: String)
    suspend fun markRead(target: TimelineTarget)
    suspend fun setTyping(target: TimelineTarget, typing: Boolean)
}

/** [ChatApi] on a [ChordClient]. */
class ClientChatApi(private val client: ChordClient) : ChatApi {
    override suspend fun spaces(sink: (SpaceDiff) -> Unit): ViewHandle =
        client.subscribeSpaceList(object : SpaceListListener {
            override fun onDiff(diff: SpaceDiff) = sink(diff)
        }).let { sub -> handle { sub.cancel(); sub.close() } }

    override suspend fun channels(scope: ChannelScope, sink: (ChannelDiff) -> Unit): ViewHandle =
        client.subscribeChannelList(scope, object : ChannelListListener {
            override fun onDiff(diff: ChannelDiff) = sink(diff)
        }).let { sub -> handle { sub.cancel(); sub.close() } }

    override suspend fun members(room: String, sink: (MemberDiff) -> Unit): ViewHandle =
        client.subscribeMemberList(room, object : MemberListListener {
            override fun onDiff(diff: MemberDiff) = sink(diff)
        }).let { sub -> handle { sub.cancel(); sub.close() } }

    override suspend fun timeline(target: TimelineTarget, sink: (TimelineDiff) -> Unit): TimelineHandle {
        val listener = object : TimelineListener {
            override fun onDiff(diff: TimelineDiff) = sink(diff)
        }
        val sub = when (target) {
            is TimelineTarget.Room -> client.subscribeTimeline(target.jid, listener)
            is TimelineTarget.Private -> client.subscribePrivateTimeline(target.room, target.nick, listener)
        }
        return object : TimelineHandle {
            override suspend fun paginateBack(count: Int) = sub.paginateBack(count.toUInt())
            override fun close() {
                sub.cancel()
                sub.close()
            }
        }
    }

    override suspend fun send(target: TimelineTarget, body: String) {
        when (target) {
            is TimelineTarget.Room -> client.sendChat(target.jid, body)
            is TimelineTarget.Private -> client.sendPrivate(target.room, target.nick, body)
        }
    }

    override suspend fun reply(itemId: String, body: String) = client.reply(itemId, body)
    override suspend fun edit(itemId: String, body: String) = client.editMessage(itemId, body)
    override suspend fun retract(itemId: String) = client.retractMessage(itemId)
    override suspend fun toggleReaction(itemId: String, emoji: String) = client.toggleReaction(itemId, emoji)

    override suspend fun markRead(target: TimelineTarget) = when (target) {
        is TimelineTarget.Room -> client.markRead(target.jid)
        is TimelineTarget.Private -> client.markReadPrivate(target.room, target.nick)
    }

    override suspend fun setTyping(target: TimelineTarget, typing: Boolean) = when (target) {
        is TimelineTarget.Room -> client.setTyping(target.jid, typing)
        is TimelineTarget.Private -> client.setTyping("${target.room}/${target.nick}", typing)
    }

    private fun handle(onClose: () -> Unit) = object : ViewHandle {
        override fun close() = onClose()
    }
}

/** The [ChatApi] of the current client of the session: null when nobody is signed in. */
fun ChordSession.chatApi(): Flow<ChatApi?> = client.map { it?.let(::ClientChatApi) }

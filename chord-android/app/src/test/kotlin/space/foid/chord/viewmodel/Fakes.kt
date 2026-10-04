package space.foid.chord.viewmodel

import space.foid.chord.data.ChatApi
import space.foid.chord.data.TimelineHandle
import space.foid.chord.data.TimelineTarget
import space.foid.chord.data.ViewHandle
import uniffi.chord_ffi.ChannelDiff
import uniffi.chord_ffi.ChannelItem
import uniffi.chord_ffi.ChannelKind
import uniffi.chord_ffi.ChannelScope
import uniffi.chord_ffi.DeliveryStatus
import uniffi.chord_ffi.MemberDiff
import uniffi.chord_ffi.SpaceDiff
import uniffi.chord_ffi.TimelineDiff
import uniffi.chord_ffi.TimelineItem

fun msg(id: String, body: String = "text $id") = TimelineItem(
    id = id,
    stanzaId = null,
    originId = null,
    sender = "alice@example.org",
    senderName = "Alice",
    avatar = null,
    body = body,
    timestamp = 0L,
    outgoing = false,
    sameSenderAsPrevious = false,
    edited = false,
    retracted = false,
    reactions = emptyList(),
    replyTo = null,
    attachment = null,
    status = DeliveryStatus.SENT,
)

fun chan(jid: String) = ChannelItem(
    jid = jid,
    name = jid,
    kind = ChannelKind.Room,
    category = null,
    joined = true,
    lastActivity = null,
    unread = 0u,
    blocked = false,
    members = null,
)

/** A [ChatApi] that records the calls and lets a test push diffs. */
class FakeChatApi : ChatApi {
    class Opened<D>(val param: Any?, val sink: (D) -> Unit) : ViewHandle {
        var closed = false
        override fun close() {
            closed = true
        }
    }

    val timelines = ArrayList<FakeTimeline>()
    val channelSubs = ArrayList<Opened<ChannelDiff>>()
    val spaceSubs = ArrayList<Opened<SpaceDiff>>()
    val memberSubs = ArrayList<Opened<MemberDiff>>()
    val calls = ArrayList<String>()
    var failWith: Exception? = null

    inner class FakeTimeline(val target: TimelineTarget, val sink: (TimelineDiff) -> Unit) : TimelineHandle {
        var closed = false
        var paginations = 0
        var onPaginate: suspend (FakeTimeline) -> Unit = {}
        override suspend fun paginateBack(count: Int) {
            paginations++
            calls += "paginate $count"
            onPaginate(this)
        }
        override fun close() {
            closed = true
        }
    }

    override suspend fun spaces(sink: (SpaceDiff) -> Unit): ViewHandle =
        Opened(null, sink).also { spaceSubs += it }

    override suspend fun channels(scope: ChannelScope, sink: (ChannelDiff) -> Unit): ViewHandle =
        Opened(scope, sink).also { channelSubs += it }

    override suspend fun members(room: String, sink: (MemberDiff) -> Unit): ViewHandle =
        Opened(room, sink).also { memberSubs += it }

    override suspend fun timeline(target: TimelineTarget, sink: (TimelineDiff) -> Unit): TimelineHandle {
        failWith?.let { throw it }
        return FakeTimeline(target, sink).also { timelines += it }
    }

    private fun record(call: String) {
        failWith?.let { throw it }
        calls += call
    }

    override suspend fun send(target: TimelineTarget, body: String) = record("send $body")
    override suspend fun reply(itemId: String, body: String) = record("reply $itemId $body")
    override suspend fun edit(itemId: String, body: String) = record("edit $itemId $body")
    override suspend fun retract(itemId: String) = record("retract $itemId")
    override suspend fun moderate(itemId: String) = record("moderate $itemId")
    override suspend fun toggleReaction(itemId: String, emoji: String) = record("react $itemId $emoji")
    override suspend fun markRead(target: TimelineTarget) = record("markRead")
    override suspend fun setTyping(target: TimelineTarget, typing: Boolean) = record("typing $typing")

    /** Set to suspend an upload, for example to see the pending state. */
    var onUpload: suspend () -> Unit = {}

    override suspend fun upload(target: TimelineTarget, filename: String, contentType: String, data: ByteArray): String {
        onUpload()
        record("upload $filename $contentType ${data.size}")
        return "https://upload.example/$filename"
    }
}

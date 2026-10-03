package space.foid.chord.ui.timeline

import androidx.compose.runtime.Immutable
import space.foid.chord.ui.text.FormattedMessage

/** How far a message got. PENDING is local only: the core has no state for a message in flight. */
enum class SendState { SENT, PENDING, FAILED }

/** The quoted message above a reply. [snippet] is one line of the quoted body. */
@Immutable
data class ReplyUi(
    /** The id of the quoted message, or null when the core could not find it. */
    val targetId: String?,
    val senderName: String,
    val snippet: String,
)

/** One reaction chip: an emoji with its count. [mine] is true when the user reacted with it. */
@Immutable
data class ReactionUi(
    val emoji: String,
    val count: Int,
    val mine: Boolean,
)

/**
 * One message, ready to draw. No FFI types: build it with `TimelineItem.toMessageUi` in
 * MessageMapping.kt. Whether the row is a continuation is not part of the model: the caller
 * decides with [continuesGroup] because it depends on the previous row.
 */
@Immutable
data class MessageUi(
    /** The "m:<row id>" id that the core commands take. Also the LazyColumn key. */
    val id: String,
    /** The address of the sender. Grouping compares it. */
    val senderId: String,
    val senderName: String,
    val avatarUrl: String?,
    val body: String,
    /** Unix time in ms. */
    val timestamp: Long,
    /** Short clock label, for example "14:05". */
    val timeLabel: String,
    /** The time for the header line: "today 13:24". Empty when the mapping did not set it. */
    val stamp: String = "",
    /** The domain of a sender on another server, shown as "@other.example". Null for our server. */
    val foreignDomain: String? = null,
    val outgoing: Boolean,
    /** The core says the previous row has the same sender and is close in time. */
    val sameSenderAsPrevious: Boolean,
    val edited: Boolean,
    val retracted: Boolean,
    val reactions: List<ReactionUi> = emptyList(),
    val reply: ReplyUi? = null,
    /** The URL of an attachment, if any. */
    val attachment: String? = null,
    val state: SendState = SendState.SENT,
    /**
     * The body, parsed once in the mapping step (see `formatMessage`). Null when the mapping had
     * no palette, for example in a test. MessageRow then formats the body itself, once.
     */
    val formatted: FormattedMessage? = null,
)

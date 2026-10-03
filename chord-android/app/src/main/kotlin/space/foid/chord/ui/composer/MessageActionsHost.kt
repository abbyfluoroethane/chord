package space.foid.chord.ui.composer

import android.content.ClipData
import android.widget.Toast
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalClipboard
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.platform.toClipEntry
import androidx.compose.ui.res.stringResource
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import kotlinx.coroutines.launch
import space.foid.chord.R
import space.foid.chord.data.TimelineTarget
import space.foid.chord.ui.screens.timelineTargetFor
import space.foid.chord.ui.sheets.DeleteConfirmDialog
import space.foid.chord.ui.sheets.MessageActionsSheet
import space.foid.chord.ui.sheets.ReactionPickerSheet
import space.foid.chord.ui.sheets.RecentEmoji
import space.foid.chord.ui.timeline.MessageUi
import space.foid.chord.viewmodel.ChordViewModels
import space.foid.chord.viewmodel.TimelineViewModel
import uniffi.chord_ffi.ChannelScope

private const val NAME_MARK = "\u0001"

private enum class Stage { Actions, Picker, Forward, Confirm }

/**
 * Everything that follows a long press on a message: the action sheet, the reaction picker, the
 * forward sheet and the delete question. TimelineScreen only shows it and says what to do for
 * reply and edit.
 *
 * @param target the chat of the message. A private chat with a room occupant has no chat link.
 * @param direct the chat is a 1:1 chat.
 * @param moderator the account moderates this room.
 * @param messageId the ID that another person can use (stanza id, origin id, or the row id).
 */
@Composable
fun MessageActionsHost(
    message: MessageUi,
    vm: TimelineViewModel,
    target: TimelineTarget,
    direct: Boolean,
    moderator: Boolean,
    messageId: String,
    onDismiss: () -> Unit,
    onReply: () -> Unit,
    onEdit: () -> Unit,
) {
    val context = LocalContext.current
    val clipboard = LocalClipboard.current
    val uri = LocalUriHandler.current
    val scope = rememberCoroutineScope()
    val recent = remember { RecentEmoji.of(context) }
    var stage by remember { mutableStateOf(Stage.Actions) }

    fun toast(text: String) = Toast.makeText(context, text, Toast.LENGTH_SHORT).show()
    fun copy(text: String, done: String) {
        scope.launch { clipboard.setClipEntry(ClipData.newPlainText("chord", text).toClipEntry()) }
        toast(done)
    }
    fun react(emoji: String) {
        recent.add(emoji)
        vm.toggleReaction(message.id, emoji)
    }

    val chatJid = when (target) {
        is TimelineTarget.Room -> target.jid
        is TimelineTarget.Private -> null
    }
    val actions = messageActions(message, moderator, chatJid != null)
    val own = message.outgoing && !message.retracted
    val linkCopied = stringResource(R.string.msg_link_copied)
    val textCopied = stringResource(R.string.msg_text_copied)
    val idCopied = stringResource(R.string.msg_id_copied)
    val noApp = stringResource(R.string.msg_no_app_for_link)
    val forwarded = stringResource(R.string.forward_done, NAME_MARK)

    when (stage) {
        Stage.Actions -> MessageActionsSheet(
            message = message,
            actions = actions,
            quick = quickReactions(recent.list()),
            links = if (message.retracted) emptyList() else linksIn(message.body),
            channelLinkLabel = stringResource(if (direct) R.string.msg_copy_contact_link else R.string.msg_copy_channel_link),
            onDismiss = { if (stage == Stage.Actions) onDismiss() },
            onReact = { e ->
                react(e)
                onDismiss()
            },
            onMoreReactions = { stage = Stage.Picker },
            onOpenLink = { l ->
                runCatching { uri.openUri(l) }.onFailure { toast(noApp) }
                onDismiss()
            },
            onCopyLink = { l ->
                copy(l, linkCopied)
                onDismiss()
            },
            onAction = { a ->
                when (a) {
                    MessageAction.Reply -> {
                        onReply()
                        onDismiss()
                    }
                    MessageAction.Edit -> {
                        onEdit()
                        onDismiss()
                    }
                    MessageAction.Forward -> stage = Stage.Forward
                    MessageAction.Delete, MessageAction.Remove -> stage = Stage.Confirm
                    MessageAction.CopyText -> {
                        copy(message.body, textCopied)
                        onDismiss()
                    }
                    MessageAction.CopyChannelLink -> {
                        chatJid?.let { copy(channelLink(it, direct), linkCopied) }
                        onDismiss()
                    }
                    MessageAction.CopyId -> {
                        copy(messageId, idCopied)
                        onDismiss()
                    }
                }
            },
        )
        Stage.Picker -> ReactionPickerSheet(
            onDismiss = onDismiss,
            onPick = { e ->
                react(e)
                onDismiss()
            },
        )
        Stage.Forward -> ForwardHost(message, onDismiss) { to, label ->
            vm.forward(timelineTargetFor(to.jid), forwardText(message.body, message.attachment))
            toast(forwarded.replace(NAME_MARK, (if (to.direct) "" else "#") + label))
            onDismiss()
        }
        Stage.Confirm -> DeleteConfirmDialog(
            message = message,
            remove = !own,
            onConfirm = {
                if (own) vm.retract(message.id) else vm.moderate(message.id)
                onDismiss()
            },
            onCancel = onDismiss,
        )
    }
}

@Composable
private fun ForwardHost(message: MessageUi, onDismiss: () -> Unit, onPick: (ForwardTarget, String) -> Unit) {
    val vm: space.foid.chord.viewmodel.ChannelListViewModel =
        viewModel(key = "forward", factory = ChordViewModels.channelList(ChannelScope.Home))
    val channels by vm.channels.collectAsStateWithLifecycle()
    val targets = remember(channels) { forwardTargets(channels) }
    ForwardSheet(
        senderName = message.senderName,
        summary = forwardSummary(message.body, message.attachment),
        targets = targets,
        onDismiss = onDismiss,
        onForward = { t -> onPick(t, t.label) },
    )
}

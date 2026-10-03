package space.foid.chord.ui.sheets

import androidx.compose.runtime.Composable
import space.foid.chord.ui.timeline.MessageUi

// STUBS. Another agent owns the real sheets and replaces this file. Keep the signatures.

@Composable
fun MessageActionsSheet(
    message: MessageUi,
    canEdit: Boolean,
    canRetract: Boolean,
    onDismiss: () -> Unit,
    onReply: () -> Unit,
    onEdit: () -> Unit,
    onRetract: () -> Unit,
    onCopy: () -> Unit,
    onReact: (String) -> Unit,
    onMoreReactions: () -> Unit,
) {
}

@Composable
fun ReactionPickerSheet(onDismiss: () -> Unit, onPick: (String) -> Unit) {
}

@Composable
fun AttachmentSheet(onDismiss: () -> Unit, onPickImage: () -> Unit, onPickFile: () -> Unit) {
}

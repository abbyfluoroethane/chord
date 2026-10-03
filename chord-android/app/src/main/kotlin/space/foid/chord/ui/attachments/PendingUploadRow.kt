package space.foid.chord.ui.attachments

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import space.foid.chord.R
import space.foid.chord.ui.sheets.LineIcon
import space.foid.chord.ui.sheets.SheetIcon
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordRadius
import space.foid.chord.ui.theme.ChordSpace
import space.foid.chord.ui.theme.ChordType
import space.foid.chord.viewmodel.UploadStage
import space.foid.chord.viewmodel.UploadUi

/** The attachments that wait for the upload or failed. It shows nothing for an empty list. */
@Composable
fun PendingUploads(
    uploads: List<UploadUi>,
    onRetry: (Long) -> Unit,
    onDismiss: (Long) -> Unit,
    modifier: Modifier = Modifier,
) {
    if (uploads.isEmpty()) return
    Column(modifier.fillMaxWidth().padding(horizontal = ChordSpace.s3, vertical = ChordSpace.s1), verticalArrangement = Arrangement.spacedBy(ChordSpace.s1)) {
        for (u in uploads) PendingUploadRow(u, onRetry = { onRetry(u.id) }, onDismiss = { onDismiss(u.id) })
    }
}

/**
 * One pending attachment. The core has no progress for an upload, so the bar is indeterminate.
 * A failed upload shows the reason, "Try again" and "Remove".
 */
@Composable
fun PendingUploadRow(upload: UploadUi, onRetry: () -> Unit, onDismiss: () -> Unit, modifier: Modifier = Modifier) {
    val colors = Chord.colors
    val failed = upload.stage == UploadStage.FAILED
    val shape = RoundedCornerShape(ChordRadius.md)
    Column(
        modifier
            .fillMaxWidth()
            .background(colors.surface300, shape)
            .border(1.dp, if (failed) colors.danger else colors.line, shape)
            .padding(ChordSpace.s3)
            .testTag("pending_upload"),
        verticalArrangement = Arrangement.spacedBy(ChordSpace.s2),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(ChordSpace.s3)) {
            LineIcon(SheetIcon.File, if (failed) colors.danger else colors.accent, size = 24.dp)
            val text = when (upload.stage) {
                UploadStage.PREPARING -> stringResource(R.string.upload_preparing, upload.name)
                UploadStage.UPLOADING -> stringResource(R.string.upload_sending, upload.name)
                UploadStage.FAILED -> upload.name
            }
            Text(text, style = ChordType.bodySmall, color = colors.ink, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
        }
        if (failed) {
            Text(
                stringResource(R.string.upload_failed, upload.error.orEmpty()),
                style = ChordType.bodySmall,
                color = colors.danger,
            )
            Row(horizontalArrangement = Arrangement.spacedBy(ChordSpace.s4)) {
                Text(
                    stringResource(R.string.upload_retry),
                    style = ChordType.bodySmall.copy(textDecoration = TextDecoration.Underline),
                    color = colors.danger,
                    modifier = Modifier.clickable(role = Role.Button, onClick = onRetry).testTag("upload_retry"),
                )
                Text(
                    stringResource(R.string.upload_dismiss),
                    style = ChordType.bodySmall.copy(textDecoration = TextDecoration.Underline),
                    color = colors.inkMuted,
                    modifier = Modifier.clickable(role = Role.Button, onClick = onDismiss).testTag("upload_dismiss"),
                )
            }
        } else {
            LinearProgressIndicator(
                modifier = Modifier.fillMaxWidth().height(3.dp),
                color = colors.brand,
                trackColor = colors.line,
            )
        }
    }
}

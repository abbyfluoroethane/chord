package space.foid.chord.ui.attachments

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
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

/**
 * The attachment tray above the composer, as on the desktop: one tile for each file, with a file
 * icon, the name, and a remove button. It shows nothing for an empty list.
 */
@Composable
fun PendingUploads(
    uploads: List<UploadUi>,
    onRetry: (Long) -> Unit,
    onDismiss: (Long) -> Unit,
    modifier: Modifier = Modifier,
) {
    if (uploads.isEmpty()) return
    val colors = Chord.colors
    Column(modifier.fillMaxWidth().background(colors.surface200)) {
        Box(Modifier.fillMaxWidth().height(1.dp).background(colors.line))
        Row(
            Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(ChordSpace.s3),
            horizontalArrangement = Arrangement.spacedBy(ChordSpace.s3),
        ) {
            for (u in uploads) PendingUploadRow(u, onRetry = { onRetry(u.id) }, onDismiss = { onDismiss(u.id) })
        }
    }
}

/**
 * One tile of the tray. The core has no progress for an upload, so the bar is indeterminate.
 * A failed upload shows the reason and "Retry". The remove button shows unless the upload runs.
 */
@Composable
fun PendingUploadRow(upload: UploadUi, onRetry: () -> Unit, onDismiss: () -> Unit, modifier: Modifier = Modifier) {
    val colors = Chord.colors
    val failed = upload.stage == UploadStage.FAILED
    val busy = !failed
    val shape = RoundedCornerShape(ChordRadius.md)
    Column(modifier.width(TILE).testTag("pending_upload")) {
        Box(
            Modifier.fillMaxWidth().height(THUMB).clip(shape).background(colors.surface300)
                .border(1.dp, if (failed) colors.danger else colors.line, shape),
            contentAlignment = Alignment.Center,
        ) {
            LineIcon(SheetIcon.File, colors.inkMuted, size = 28.dp)
            if (busy) {
                Column(
                    Modifier.fillMaxSize().background(colors.surface300).padding(ChordSpace.s2),
                    verticalArrangement = Arrangement.Center,
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    Text(stringResource(R.string.tray_uploading), style = ChordType.bodySmall, color = colors.ink)
                    LinearProgressIndicator(
                        modifier = Modifier.padding(top = ChordSpace.s2).fillMaxWidth().height(3.dp),
                        color = colors.brand,
                        trackColor = colors.line,
                    )
                }
            } else {
                Column(
                    Modifier.fillMaxSize().background(colors.surface300).padding(start = ChordSpace.s2, end = ChordSpace.s2, top = ChordSpace.s6, bottom = ChordSpace.s2),
                    verticalArrangement = Arrangement.Center,
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    Text(
                        upload.error.orEmpty(),
                        style = ChordType.caption, color = colors.danger, textAlign = TextAlign.Center,
                        maxLines = 2, overflow = TextOverflow.Ellipsis,
                    )
                    Text(
                        stringResource(R.string.tray_retry),
                        style = ChordType.bodySmall.copy(textDecoration = TextDecoration.Underline),
                        color = colors.danger,
                        modifier = Modifier.padding(top = ChordSpace.s1).clickable(role = Role.Button, onClick = onRetry).testTag("upload_retry"),
                    )
                }
            }
            if (failed || upload.stage != UploadStage.UPLOADING) {
                val remove = stringResource(R.string.tray_remove, upload.name)
                Box(
                    Modifier.align(Alignment.TopEnd).padding(4.dp).size(24.dp)
                        .background(colors.surface100, CircleShape)
                        .semantics { contentDescription = remove; role = Role.Button }
                        .clickable(onClick = onDismiss).testTag("upload_dismiss"),
                    contentAlignment = Alignment.Center,
                ) {
                    Box(
                        Modifier.size(9.dp).drawBehind {
                            val w = 1.6.dp.toPx()
                            drawLine(colors.inkMuted, Offset(0f, 0f), Offset(size.width, size.height), w, StrokeCap.Round)
                            drawLine(colors.inkMuted, Offset(size.width, 0f), Offset(0f, size.height), w, StrokeCap.Round)
                        },
                    )
                }
            }
        }
        Text(
            upload.name, style = ChordType.caption, color = colors.ink, maxLines = 1, overflow = TextOverflow.Ellipsis,
            modifier = Modifier.padding(top = ChordSpace.s1),
        )
    }
}

private val TILE = 120.dp
private val THUMB = 88.dp

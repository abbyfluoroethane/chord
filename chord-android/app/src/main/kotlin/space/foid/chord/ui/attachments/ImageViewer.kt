package space.foid.chord.ui.attachments

import android.widget.Toast
import androidx.compose.animation.core.Animatable
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.calculatePan
import androidx.compose.foundation.gestures.calculateZoom
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.positionChanged
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import kotlinx.coroutines.launch
import space.foid.chord.R
import space.foid.chord.ui.theme.ChordRadius
import space.foid.chord.ui.theme.ChordSpace
import space.foid.chord.ui.theme.ChordType
import kotlin.math.abs

private const val MAX_ZOOM = 5f
private const val CLOSE_DISTANCE_DP = 120

/**
 * The full-screen viewer of an image: pinch to zoom, double tap to zoom in and out, drag down to
 * close, and Share and Save in the top bar. Save is missing before Android 10.
 */
@Composable
fun ImageViewer(url: String, outgoing: Boolean, onDismiss: () -> Unit) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val saved = stringResource(R.string.viewer_saved)
    val saveFailed = stringResource(R.string.viewer_save_failed)
    Dialog(
        onDismissRequest = onDismiss,
        properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false),
    ) {
        ImageViewerContent(
            state = rememberImageState(url),
            title = fileNameOfUrl(url),
            canSave = ImageSaver.canSave,
            onClose = onDismiss,
            onShare = { ImageSaver.share(context, url) },
            onSave = {
                scope.launch {
                    val ok = runCatching { ImageSaver.save(context, url, allowHttp = outgoing) }.isSuccess
                    Toast.makeText(context, if (ok) saved else saveFailed, Toast.LENGTH_SHORT).show()
                }
            },
        )
    }
}

/** The viewer, stateless. */
@Composable
fun ImageViewerContent(
    state: ImageState,
    title: String,
    canSave: Boolean,
    onClose: () -> Unit,
    onShare: () -> Unit,
    onSave: () -> Unit,
    modifier: Modifier = Modifier,
) {
    var scale by remember { mutableFloatStateOf(1f) }
    var pan by remember { mutableStateOf(Offset.Zero) }
    val drag = remember { Animatable(0f) }
    var box by remember { mutableStateOf(IntSize.Zero) }
    val scope = rememberCoroutineScope()
    val density = androidx.compose.ui.platform.LocalDensity.current
    val closeAt = with(density) { CLOSE_DISTANCE_DP.dp.toPx() }

    // The black fades as the image is dragged away.
    val fade = (1f - abs(drag.value) / (closeAt * 3f)).coerceIn(0.2f, 1f)

    fun clampPan(p: Offset, s: Float): Offset {
        val maxX = (box.width * (s - 1f)) / 2f
        val maxY = (box.height * (s - 1f)) / 2f
        return Offset(p.x.coerceIn(-maxX, maxX), p.y.coerceIn(-maxY, maxY))
    }

    Box(
        modifier
            .fillMaxSize()
            .background(Color.Black.copy(alpha = fade))
            .onSizeChanged { box = it }
            .testTag("image_viewer"),
    ) {
        val painter = state.painter
        if (painter != null) {
            Image(
                painter = painter,
                contentDescription = title.ifEmpty { null },
                contentScale = ContentScale.Fit,
                modifier = Modifier
                    .fillMaxSize()
                    .pointerInput(Unit) {
                        detectTapGestures(onDoubleTap = {
                            if (scale > 1.05f) {
                                scale = 1f
                                pan = Offset.Zero
                            } else {
                                scale = 2.5f
                            }
                        })
                    }
                    .pointerInput(closeAt) {
                        awaitEachGesture {
                            awaitFirstDown(requireUnconsumed = false)
                            var dragY = drag.value
                            do {
                                val event = awaitPointerEvent()
                                val zoom = event.calculateZoom()
                                val move = event.calculatePan()
                                if (zoom != 1f || move != Offset.Zero) {
                                    scale = (scale * zoom).coerceIn(1f, MAX_ZOOM)
                                    if (scale > 1.02f) {
                                        pan = clampPan(pan + move, scale)
                                        dragY = 0f
                                    } else if (event.changes.size == 1) {
                                        // Not zoomed: a vertical drag moves the image to close it.
                                        pan = Offset.Zero
                                        dragY += move.y
                                        scope.launch { drag.snapTo(dragY) }
                                    }
                                    event.changes.forEach { if (it.positionChanged()) it.consume() }
                                }
                            } while (event.changes.any { it.pressed })
                            if (scale < 1.05f) {
                                scale = 1f
                                pan = Offset.Zero
                            }
                            if (scale == 1f && abs(dragY) > closeAt) {
                                onClose()
                            } else {
                                scope.launch { drag.animateTo(0f) }
                            }
                        }
                    }
                    .graphicsLayer {
                        scaleX = scale
                        scaleY = scale
                        translationX = pan.x
                        translationY = pan.y + drag.value
                    },
            )
        } else {
            Text(
                stringResource(
                    if (state.status == ImageStatus.FAILED) R.string.viewer_image_failed else R.string.attachment_image_loading,
                ),
                style = ChordType.body,
                color = Color.White,
                modifier = Modifier.align(Alignment.Center),
            )
        }
        Row(
            Modifier
                .fillMaxWidth()
                .background(Color.Black.copy(alpha = 0.45f * fade))
                .statusBarsPadding()
                .height(56.dp)
                .padding(horizontal = ChordSpace.s2),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(ChordSpace.s1),
        ) {
            ViewerButton(stringResource(R.string.viewer_close), onClose, Modifier.testTag("viewer_close"))
            Text(
                title,
                style = ChordType.label,
                color = Color.White,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f).padding(horizontal = ChordSpace.s2),
            )
            ViewerButton(stringResource(R.string.viewer_share), onShare, Modifier.testTag("viewer_share"))
            if (canSave) ViewerButton(stringResource(R.string.viewer_save), onSave, Modifier.testTag("viewer_save"))
        }
    }
}

@Composable
private fun ViewerButton(label: String, onClick: () -> Unit, modifier: Modifier = Modifier) {
    Box(
        modifier
            .height(40.dp)
            .background(Color.White.copy(alpha = 0.14f), RoundedCornerShape(ChordRadius.md))
            .clickable(role = Role.Button, onClick = onClick)
            .padding(horizontal = ChordSpace.s3),
        contentAlignment = Alignment.Center,
    ) {
        Text(label, style = ChordType.label, color = Color.White, maxLines = 1)
    }
}

package space.foid.chord.ui.attachments

import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.widget.Toast
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.Image
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import coil3.compose.AsyncImagePainter
import coil3.compose.rememberAsyncImagePainter
import coil3.request.ImageRequest
import space.foid.chord.R
import space.foid.chord.ui.sheets.LineIcon
import space.foid.chord.ui.sheets.SheetIcon
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordRadius
import space.foid.chord.ui.theme.ChordSpace
import space.foid.chord.ui.theme.ChordType

/**
 * The attachment of a message: an inline image for an image URL, a file chip for the rest.
 *
 * @param outgoing true for the user's own message. An image of someone else never loads over
 *   plain http (see [mayLoadImage]).
 * @param onImageClick opens the full-screen viewer: gets the image URL.
 */
@Composable
fun AttachmentView(
    url: String,
    outgoing: Boolean,
    modifier: Modifier = Modifier,
    onImageClick: (String) -> Unit = {},
) {
    val context = LocalContext.current
    when (attachmentKind(url)) {
        AttachmentKind.IMAGE ->
            if (mayLoadImage(url, outgoing)) {
                ImageAttachment(url, modifier, onImageClick)
            } else {
                FileChipContent(
                    name = fileNameOfUrl(url).ifEmpty { stringResource(R.string.attachment_image) },
                    detail = stringResource(R.string.attachment_image_insecure),
                    action = stringResource(R.string.attachment_open),
                    onClick = { openLink(context, url) },
                    modifier = modifier,
                )
            }
        else -> FileChip(url, modifier)
    }
}

/** The state of the image at [url], from the app source (Coil) or a test source. */
@Composable
fun rememberImageState(url: String): ImageState {
    val source = LocalImageSource.current
    if (source != null) return source.stateFor(url)
    val context = LocalContext.current
    val painter = rememberAsyncImagePainter(
        model = ImageRequest.Builder(context).data(url).build(),
        imageLoader = ChordImages.loader(context),
    )
    val state by painter.state.collectAsState()
    return when (state) {
        is AsyncImagePainter.State.Success -> ImageState(painter, ImageStatus.LOADED)
        is AsyncImagePainter.State.Error -> ImageState(null, ImageStatus.FAILED)
        else -> ImageState(null, ImageStatus.LOADING)
    }
}

/** An inline image that loads from [url]. */
@Composable
fun ImageAttachment(url: String, modifier: Modifier = Modifier, onClick: (String) -> Unit = {}) {
    val context = LocalContext.current
    ImageThumbContent(
        state = rememberImageState(url),
        url = url,
        onClick = { onClick(url) },
        onOpenLink = { openLink(context, url) },
        modifier = modifier,
    )
}

/**
 * The inline image, stateless. A rounded thumbnail, at most [THUMB_MAX_WIDTH_DP] wide, with the
 * height from the aspect ratio. While it loads there is a placeholder block. On an error the link
 * shows instead.
 */
@Composable
fun ImageThumbContent(
    state: ImageState,
    url: String,
    onClick: () -> Unit,
    onOpenLink: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val colors = Chord.colors
    val shape = RoundedCornerShape(ChordRadius.md)
    if (state.status == ImageStatus.FAILED) {
        Column(modifier.padding(top = ChordSpace.s1)) {
            Text(stringResource(R.string.attachment_image_failed), style = ChordType.caption, color = colors.inkMuted)
            Text(
                url,
                style = ChordType.bodySmall,
                color = colors.accent,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.clickable(role = Role.Button, onClick = onOpenLink).testTag("attachment_link"),
            )
        }
        return
    }
    val painter = state.painter
    val size = painter?.intrinsicSize
    val aspect = if (size != null) thumbnailAspect(size.width, size.height) else DEFAULT_ASPECT
    val description = stringResource(R.string.attachment_image)
    Box(
        modifier
            .padding(top = ChordSpace.s1)
            .widthIn(max = THUMB_MAX_WIDTH_DP.dp)
            .fillMaxWidth()
            .aspectRatio(aspect)
            .clip(shape)
            .background(colors.surface300)
            .border(1.dp, colors.line, shape)
            .semantics { contentDescription = description }
            .clickable(role = Role.Button, onClick = onClick)
            .testTag("attachment_image"),
    ) {
        if (painter != null) {
            Image(painter, contentDescription = null, contentScale = ContentScale.Crop, modifier = Modifier.fillMaxSize())
        } else {
            Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                Text(stringResource(R.string.attachment_image_loading), style = ChordType.caption, color = colors.inkMuted)
            }
        }
    }
}

/** A chip for a file at [url]. A tap opens the link: the browser or another app takes over. */
@Composable
fun FileChip(url: String, modifier: Modifier = Modifier, sizeBytes: Long? = null) {
    val context = LocalContext.current
    val kind = attachmentKind(url)
    val label = when (kind) {
        AttachmentKind.VIDEO -> stringResource(R.string.attachment_kind_video)
        AttachmentKind.AUDIO -> stringResource(R.string.attachment_kind_audio)
        else -> stringResource(R.string.attachment_kind_file)
    }
    val ext = extensionOf(url).uppercase()
    val detail = listOfNotNull(
        if (ext.isNotEmpty()) "$ext $label" else label,
        sizeBytes?.let(::formatBytes),
        hostOfUrl(url).takeIf { sizeBytes == null && it != url },
    ).joinToString(" · ")
    FileChipContent(
        name = fileNameOfUrl(url).ifEmpty { hostOfUrl(url) },
        detail = detail,
        action = stringResource(R.string.attachment_open),
        onClick = { openLink(context, url) },
        modifier = modifier,
    )
}

/** The file chip, stateless: icon, name, one line of detail and the action. */
@Composable
fun FileChipContent(
    name: String,
    detail: String,
    action: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val colors = Chord.colors
    val shape = RoundedCornerShape(ChordRadius.md)
    Row(
        modifier
            .padding(top = ChordSpace.s1)
            .widthIn(max = THUMB_MAX_WIDTH_DP.dp + 40.dp)
            .fillMaxWidth()
            .clip(shape)
            .background(colors.surface300)
            .border(1.dp, colors.line, shape)
            .clickable(role = Role.Button, onClick = onClick)
            .padding(ChordSpace.s3)
            .testTag("attachment_file"),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(ChordSpace.s3),
    ) {
        LineIcon(SheetIcon.File, colors.accent, size = 28.dp)
        Column(Modifier.weight(1f)) {
            Text(name, style = ChordType.name, color = colors.ink, maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (detail.isNotEmpty()) {
                Text(detail, style = ChordType.caption, color = colors.inkMuted, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
        Text(action, style = ChordType.label, color = colors.accent)
    }
}

/** Open [url] in the browser or another app. Only web links. A failure shows a short message. */
fun openLink(context: Context, url: String) {
    if (!isWebUrl(url)) return
    try {
        context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url)).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
    } catch (_: ActivityNotFoundException) {
        Toast.makeText(context, R.string.attachment_open_failed, Toast.LENGTH_SHORT).show()
    }
}

package space.foid.chord.ui.emoji

import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.Placeholder
import androidx.compose.ui.text.PlaceholderVerticalAlign
import androidx.compose.ui.text.TextStyle
import androidx.compose.foundation.text.appendInlineContent
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.em
import androidx.compose.foundation.text.InlineTextContent
import space.foid.chord.ui.text.emojiRuns

/** The images that draw emoji now. Null: the font of the phone draws them. */
val LocalEmojiImages = staticCompositionLocalOf<EmojiImages?> { null }

/** The size of an emoji image next to text, as on the desktop: 1.375em. */
private val GLYPH = 1.375.em

/**
 * Replaces each emoji of [text] that [images] has with an inline image. Styles and links stay.
 * Returns the new text and the content map for `Text(inlineContent = ...)`, or null if no emoji
 * changes.
 */
fun withEmojiImages(text: AnnotatedString, images: EmojiImages): Pair<AnnotatedString, Map<String, InlineTextContent>>? {
    val runs = emojiRuns(text.text).filter { images.has(text.text.substring(it.first, it.last + 1)) }
    if (runs.isEmpty()) return null
    val content = LinkedHashMap<String, InlineTextContent>()
    val built = buildAnnotatedString {
        var pos = 0
        for (run in runs) {
            if (run.first > pos) append(text.subSequence(pos, run.first))
            val emoji = text.text.substring(run.first, run.last + 1)
            appendInlineContent(emoji, emoji)
            if (emoji !in content) {
                content[emoji] = InlineTextContent(Placeholder(GLYPH, GLYPH, PlaceholderVerticalAlign.TextCenter)) {
                    EmojiGlyph(images, emoji)
                }
            }
            pos = run.last + 1
        }
        if (pos < text.length) append(text.subSequence(pos, text.length))
    }
    return built to content
}

/** One emoji image. It stays empty until the bitmap is ready. */
@Composable
fun EmojiGlyph(images: EmojiImages, emoji: String, modifier: Modifier = Modifier) {
    val bmp by produceState(images.cached(emoji), images, emoji) {
        if (value == null) value = images.load(emoji)
    }
    Box(modifier.fillMaxSize()) {
        bmp?.let { Image(it, contentDescription = emoji, modifier = Modifier.fillMaxSize()) }
    }
}

/** [Text] that draws emoji with the chosen pack. Without a pack it is the plain [Text]. */
@Composable
fun EmojiText(
    text: AnnotatedString,
    style: TextStyle,
    color: Color,
    modifier: Modifier = Modifier,
    maxLines: Int = Int.MAX_VALUE,
    overflow: TextOverflow = TextOverflow.Clip,
) {
    val images = LocalEmojiImages.current
    val replaced = remember(text, images) { images?.let { withEmojiImages(text, it) } }
    if (replaced == null) {
        Text(text, modifier, color = color, style = style, maxLines = maxLines, overflow = overflow)
    } else {
        Text(replaced.first, modifier, color = color, style = style, inlineContent = replaced.second, maxLines = maxLines, overflow = overflow)
    }
}

/** An emoji or a short run of emoji as a label, for reaction chips and the like. */
@Composable
fun EmojiLabel(emoji: String, style: TextStyle, color: Color = Color.Unspecified, modifier: Modifier = Modifier) {
    val text = remember(emoji) { AnnotatedString(emoji) }
    EmojiText(text, style, color, modifier)
}

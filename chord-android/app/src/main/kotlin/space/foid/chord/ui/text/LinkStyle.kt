package space.foid.chord.ui.text

import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLinkStyles
import androidx.compose.ui.text.style.TextDecoration

/** The same text with links that keep their colour but have no underline. */
fun AnnotatedString.withoutLinkUnderline(): AnnotatedString = mapAnnotations { range ->
    val item = range.item
    if (item is LinkAnnotation.Url) {
        val styles = item.styles
        val plain = TextLinkStyles(
            style = styles?.style?.copy(textDecoration = TextDecoration.None),
            focusedStyle = styles?.focusedStyle,
            hoveredStyle = styles?.hoveredStyle,
            pressedStyle = styles?.pressedStyle,
        )
        AnnotatedString.Range(LinkAnnotation.Url(item.url, plain, item.linkInteractionListener), range.start, range.end)
    } else range
}

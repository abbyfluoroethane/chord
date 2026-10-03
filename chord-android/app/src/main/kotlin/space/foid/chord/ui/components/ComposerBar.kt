package space.foid.chord.ui.components

import androidx.compose.runtime.produceState
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import space.foid.chord.R
import space.foid.chord.ui.composer.ShortcodeIndex
import space.foid.chord.ui.composer.ShortcodeIndexCache
import space.foid.chord.ui.composer.Suggestion
import space.foid.chord.ui.composer.SuggestionList
import space.foid.chord.ui.composer.TextEdit
import space.foid.chord.ui.composer.findMention
import space.foid.chord.ui.composer.findShortcode
import space.foid.chord.ui.composer.insertAtSelection
import space.foid.chord.ui.composer.insertMention
import space.foid.chord.ui.composer.insertShortcode
import space.foid.chord.ui.composer.suggestNicks
import space.foid.chord.ui.sheets.LineIcon
import space.foid.chord.ui.sheets.ReactionPickerSheet
import space.foid.chord.ui.sheets.SheetIcon
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordRadius
import space.foid.chord.ui.theme.ChordSize
import space.foid.chord.ui.theme.ChordSpace
import space.foid.chord.ui.theme.ChordType

/**
 * The composer: pinned to the bottom of the chat screen, above the keyboard.
 *
 * INSETS. The bar applies the bottom system insets itself, in this order: `imePadding()`, then
 * `navigationBarsPadding()`. Compose consumes what the first one used, so the bar clears the
 * keyboard when it is open, and the navigation bar when it is closed, never both. The bar draws
 * its own background behind the inset, so the area under the navigation bar has the bar colour.
 *
 * The caller must NOT also apply any of these on the bar or on a parent of the bar:
 * `imePadding()`, `navigationBarsPadding()`, `safeDrawingPadding()`, `systemBarsPadding()`,
 * `windowInsetsPadding(WindowInsets.ime / navigationBars / safeDrawing)`, and must not use
 * `Scaffold` bottom content padding for the bar. The Activity is edge-to-edge
 * (`enableEdgeToEdge()`) and the manifest has `windowSoftInputMode="adjustResize"` or the
 * default: do not use `adjustPan`. Put the bar as the last child of a `Column` below the
 * timeline (`weight(1f)`); the timeline shrinks when the bar grows. The caller may add the
 * top and side insets (`statusBarsPadding`, `displayCutoutPadding`) on its own.
 *
 * The text is hoisted. The bar is 6 lines high at most, then the field scrolls.
 *
 * @param replyingTo the name of the sender of the quoted message: shows the "Replying to" strip.
 * @param editing true while the user edits an own message: shows the "Editing message" strip.
 *   [onCancelEdit] must clear the text too, the bar does not touch it.
 * @param mentionNicks the nicks of the room: typing @ offers them. Empty in a 1:1 chat.
 * @param shortcodeIndex the emoji names for :name: null loads the bundled catalog when needed.
 * @param onAttach opens the file picker. The bar only calls it.
 * @param inputModifier extra modifier of the text field (for example a test tag).
 * @param sendModifier extra modifier of the send button.
 */
@Composable
fun ComposerBar(
    text: String,
    onTextChange: (String) -> Unit,
    onSend: () -> Unit,
    modifier: Modifier = Modifier,
    placeholder: String = "Message",
    replyingTo: String? = null,
    onCancelReply: () -> Unit = {},
    editing: Boolean = false,
    onCancelEdit: () -> Unit = {},
    onAttach: () -> Unit = {},
    inputModifier: Modifier = Modifier,
    sendModifier: Modifier = Modifier,
    mentionNicks: List<String> = emptyList(),
    shortcodeIndex: ShortcodeIndex? = null,
) {
    val colors = Chord.colors
    val canSend = text.isNotBlank()
    // The field keeps its own selection. When the text is set from outside (an edit starts), the
    // cursor goes to the end, not to the start.
    var field by remember { mutableStateOf(TextFieldValue(text, TextRange(text.length))) }
    if (field.text != text) field = TextFieldValue(text, TextRange(text.length))
    val focus = remember { FocusRequester() }
    LaunchedEffect(editing, replyingTo != null) {
        if (editing || replyingTo != null) runCatching { focus.requestFocus() }
    }

    // The word under the caret: "@nick" in a room, or ":name" for an emoji.
    val caret = field.selection.let { if (it.collapsed) it.start else -1 }
    val mention = if (caret >= 0 && mentionNicks.isNotEmpty()) findMention(field.text, caret) else null
    val shortcode = if (caret >= 0 && mention == null) findShortcode(field.text, caret) else null
    val index = shortcodeIndex ?: rememberShortcodeIndex(shortcode != null)
    val hits: List<Suggestion> = when {
        mention != null -> suggestNicks(mentionNicks, mention.query, SUGGESTIONS).map { Suggestion.Nick(it) }
        shortcode != null && index != null -> index.suggest(shortcode.query, SUGGESTIONS).map { Suggestion.Emoji(it.name, it.emoji) }
        else -> emptyList()
    }
    fun apply(edit: TextEdit) {
        field = TextFieldValue(edit.text, TextRange(edit.caret))
        onTextChange(edit.text)
    }
    var pickerOpen by remember { mutableStateOf(false) }

    Column(
        modifier
            .fillMaxWidth()
            .background(colors.surface100)
            .imePadding()
            .navigationBarsPadding(),
    ) {
        if (hits.isNotEmpty()) {
            SuggestionList(hits, onPick = { pick ->
                when (pick) {
                    is Suggestion.Nick -> mention?.let { apply(insertMention(field.text, caret, it, pick.nick)) }
                    is Suggestion.Emoji -> shortcode?.let { apply(insertShortcode(field.text, caret, it, pick.emoji)) }
                }
            })
        }
        Box(Modifier.fillMaxWidth().height(1.dp).background(colors.line))
        if (replyingTo != null) {
            Strip(onCancel = onCancelReply, cancelLabel = stringResource(R.string.composer_cancel_reply)) {
                val full = stringResource(R.string.composer_replying_to, replyingTo)
                val at = full.lastIndexOf(replyingTo)
                Text(
                    buildAnnotatedString {
                        append(full)
                        if (at >= 0) addStyle(SpanStyle(fontWeight = FontWeight.SemiBold, color = colors.ink), at, at + replyingTo.length)
                    },
                    style = ChordType.bodySmall,
                    color = colors.inkMuted,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        if (editing) {
            Strip(onCancel = onCancelEdit, cancelLabel = stringResource(R.string.composer_cancel_edit)) {
                Text(stringResource(R.string.composer_editing), style = ChordType.bodySmall, color = colors.brandInk)
            }
        }
        Row(
            Modifier.fillMaxWidth().padding(horizontal = ChordSpace.s2, vertical = ChordSpace.s2),
            verticalAlignment = Alignment.Bottom,
            horizontalArrangement = Arrangement.spacedBy(ChordSpace.s2),
        ) {
            RoundButton(label = stringResource(R.string.composer_attach), onClick = onAttach, background = colors.surface300) { PlusGlyph(colors.inkMuted) }
            BasicTextField(
                value = field,
                onValueChange = {
                    field = it
                    if (it.text != text) onTextChange(it.text)
                },
                modifier = Modifier.weight(1f).focusRequester(focus).then(inputModifier),
                textStyle = ChordType.body.copy(color = colors.ink),
                cursorBrush = SolidColor(colors.brand),
                maxLines = 6,
                keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
                decorationBox = { inner ->
                    Row(
                        Modifier
                            .fillMaxWidth()
                            .heightIn(min = ChordSize.avatar)
                            .background(colors.surface300, RoundedCornerShape(20.dp))
                            .border(1.dp, colors.line, RoundedCornerShape(20.dp))
                            .padding(start = ChordSpace.s4),
                        verticalAlignment = Alignment.Bottom,
                    ) {
                        Box(Modifier.weight(1f).heightIn(min = ChordSize.avatar).padding(vertical = 9.dp), contentAlignment = Alignment.CenterStart) {
                            if (text.isEmpty()) Text(placeholder, style = ChordType.body, color = colors.inkMuted)
                            inner()
                        }
                        val emojiLabel = stringResource(R.string.composer_emoji)
                        Box(
                            Modifier
                                .size(ChordSize.avatar)
                                .semantics { contentDescription = emojiLabel; role = Role.Button }
                                .clickable { pickerOpen = true },
                            contentAlignment = Alignment.Center,
                        ) { LineIcon(SheetIcon.Smile, colors.inkMuted, 22.dp) }
                    }
                },
            )
            RoundButton(
                label = stringResource(if (editing) R.string.composer_save_edit else R.string.composer_send),
                onClick = { if (canSend) onSend() },
                background = if (canSend) colors.brand else colors.surface300,
                enabled = canSend,
                modifier = sendModifier,
            ) { SendGlyph(if (canSend) colors.onBrand else colors.inkMuted) }
        }
    }
    if (pickerOpen) {
        ReactionPickerSheet(
            onDismiss = { pickerOpen = false },
            onPick = { e ->
                pickerOpen = false
                apply(insertAtSelection(field.text, field.selection.start, field.selection.end, e))
            },
        )
    }
}

private const val SUGGESTIONS = 6

/** The shortcode index of the bundled emoji catalog. It loads when [needed] is true the first time. */
@Composable
private fun rememberShortcodeIndex(needed: Boolean): ShortcodeIndex? {
    val context = LocalContext.current
    val index by produceState(ShortcodeIndexCache.now(), needed) {
        if (needed && value == null) value = ShortcodeIndexCache.load(context)
    }
    return index
}

/** A thin strip above the field: a label slot and a cancel button. */
@Composable
private fun Strip(
    onCancel: () -> Unit,
    cancelLabel: String,
    content: @Composable androidx.compose.foundation.layout.RowScope.() -> Unit,
) {
    val colors = Chord.colors
    Row(
        Modifier.fillMaxWidth().background(colors.surface200).padding(start = ChordSpace.s4),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Row(Modifier.weight(1f), verticalAlignment = Alignment.CenterVertically) { content() }
        Box(
            Modifier
                .size(40.dp)
                .semantics { contentDescription = cancelLabel; role = Role.Button }
                .clickable(onClick = onCancel),
            contentAlignment = Alignment.Center,
        ) { CloseGlyph(colors.inkMuted) }
    }
}

@Composable
private fun RoundButton(
    label: String,
    onClick: () -> Unit,
    background: Color,
    enabled: Boolean = true,
    modifier: Modifier = Modifier,
    glyph: @Composable () -> Unit,
) {
    Box(
        modifier
            .size(ChordSize.avatar)
            .background(background, CircleShape)
            .semantics { contentDescription = label; role = Role.Button }
            .clickable(enabled = enabled, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) { glyph() }
}

// The glyphs are drawn, so the app needs no icon library.

@Composable
private fun PlusGlyph(color: Color) {
    Box(
        Modifier.size(18.dp).drawBehind {
            val w = 2.dp.toPx()
            drawLine(color, Offset(0f, size.height / 2), Offset(size.width, size.height / 2), w, StrokeCap.Round)
            drawLine(color, Offset(size.width / 2, 0f), Offset(size.width / 2, size.height), w, StrokeCap.Round)
        },
    )
}

@Composable
private fun CloseGlyph(color: Color) {
    Box(
        Modifier.size(12.dp).drawBehind {
            val w = 2.dp.toPx()
            drawLine(color, Offset(0f, 0f), Offset(size.width, size.height), w, StrokeCap.Round)
            drawLine(color, Offset(size.width, 0f), Offset(0f, size.height), w, StrokeCap.Round)
        },
    )
}

@Composable
private fun SendGlyph(color: Color) {
    // An arrow that points up.
    Box(
        Modifier.size(18.dp).drawBehind {
            val w = 2.dp.toPx()
            val cx = size.width / 2
            drawLine(color, Offset(cx, size.height), Offset(cx, 1.dp.toPx()), w, StrokeCap.Round)
            val head = Path().apply {
                moveTo(size.width * 0.12f, size.height * 0.46f)
                lineTo(cx, 1.dp.toPx())
                lineTo(size.width * 0.88f, size.height * 0.46f)
            }
            drawPath(head, color, style = Stroke(width = w, cap = StrokeCap.Round, join = androidx.compose.ui.graphics.StrokeJoin.Round))
        },
    )
}

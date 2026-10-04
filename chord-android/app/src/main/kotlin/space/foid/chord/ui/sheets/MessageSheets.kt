package space.foid.chord.ui.sheets

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.GridItemSpan
import androidx.compose.foundation.lazy.grid.LazyGridState
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.lazy.grid.rememberLazyGridState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import kotlinx.coroutines.launch
import space.foid.chord.R
import space.foid.chord.ui.composer.MessageAction
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordRadius
import space.foid.chord.ui.theme.ChordSpace
import space.foid.chord.ui.theme.ChordType
import space.foid.chord.ui.timeline.MessageUi

/**
 * The sheet for one message: a preview, the quick reactions and the actions, in the order of the
 * desktop menu. Each action hides the sheet first, then calls its callback and [onDismiss].
 *
 * @param links the links of the message: each gets "Open link" and "Copy link" rows on top.
 * @param quick the emoji of the quick row.
 * @param channelLinkLabel the text of the "copy the link of the chat" row, or null for no row.
 */
@Composable
fun MessageActionsSheet(
    message: MessageUi,
    actions: List<MessageAction>,
    quick: List<String>,
    onDismiss: () -> Unit,
    onAction: (MessageAction) -> Unit,
    onReact: (String) -> Unit,
    onMoreReactions: () -> Unit,
    links: List<String> = emptyList(),
    onOpenLink: (String) -> Unit = {},
    onCopyLink: (String) -> Unit = {},
    channelLinkLabel: String? = null,
) {
    ChordModalSheet(onDismiss) { dismissThen ->
        MessageActionsContent(
            message = message,
            actions = actions,
            quick = quick,
            links = links,
            channelLinkLabel = channelLinkLabel,
            onAction = { a -> dismissThen { onAction(a) } },
            onReact = { e -> dismissThen { onReact(e) } },
            onMoreReactions = { dismissThen(onMoreReactions) },
            onOpenLink = { l -> dismissThen { onOpenLink(l) } },
            onCopyLink = { l -> dismissThen { onCopyLink(l) } },
        )
    }
}

/** The inside of [MessageActionsSheet], with no sheet window. */
@Composable
internal fun MessageActionsContent(
    message: MessageUi,
    actions: List<MessageAction>,
    quick: List<String>,
    onAction: (MessageAction) -> Unit,
    onReact: (String) -> Unit,
    onMoreReactions: () -> Unit,
    modifier: Modifier = Modifier,
    links: List<String> = emptyList(),
    channelLinkLabel: String? = null,
    onOpenLink: (String) -> Unit = {},
    onCopyLink: (String) -> Unit = {},
) {
    val colors = Chord.colors
    Column(modifier.fillMaxWidth().navigationBarsPadding().verticalScroll(rememberScrollState()).padding(bottom = ChordSpace.s2)) {
        MessagePreview(message, Modifier.padding(horizontal = ChordSpace.s4))
        if (!message.retracted) {
            Spacer(Modifier.height(ChordSpace.s3))
            Row(
                Modifier.fillMaxWidth().padding(horizontal = ChordSpace.s4),
                horizontalArrangement = Arrangement.spacedBy(ChordSpace.s2),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                quick.forEach { e ->
                    Box(
                        Modifier.weight(1f).height(48.dp).clip(RoundedCornerShape(ChordRadius.md)).background(colors.surface300)
                            .clickable(role = Role.Button) { onReact(e) }
                            .semantics { contentDescription = "React with $e" },
                        contentAlignment = Alignment.Center,
                    ) { Text(e, fontSize = 26.sp) }
                }
                Box(
                    Modifier.weight(1f).height(48.dp).clip(RoundedCornerShape(ChordRadius.md)).background(colors.surface300)
                        .clickable(role = Role.Button, onClick = onMoreReactions)
                        .semantics { contentDescription = "Add reaction" },
                    contentAlignment = Alignment.Center,
                ) { LineIcon(SheetIcon.Smile, colors.ink, 26.dp) }
            }
        }
        Spacer(Modifier.height(ChordSpace.s2))
        if (links.isNotEmpty()) {
            Divider()
            links.forEach { l ->
                SheetRow(SheetIcon.Open, stringResource(R.string.msg_open_link), { onOpenLink(l) }, subtitle = l)
                SheetRow(SheetIcon.Link, stringResource(R.string.msg_copy_link), { onCopyLink(l) }, subtitle = l)
            }
        }
        fun group(vararg members: MessageAction): List<MessageAction> = members.filter { it in actions }
        // The groups are as on the desktop: edit and reply, then copy, then delete, then the ID.
        listOf(
            group(MessageAction.Edit, MessageAction.Reply, MessageAction.Forward),
            group(MessageAction.CopyText, MessageAction.CopyChannelLink),
            group(MessageAction.Delete, MessageAction.Remove),
            group(MessageAction.CopyId),
        ).filter { it.isNotEmpty() }.forEach { rows ->
            Divider()
            rows.forEach { a ->
                val (icon, title) = when (a) {
                    MessageAction.Edit -> SheetIcon.Edit to stringResource(R.string.msg_edit)
                    MessageAction.Reply -> SheetIcon.Reply to stringResource(R.string.msg_reply)
                    MessageAction.Forward -> SheetIcon.Forward to stringResource(R.string.msg_forward)
                    MessageAction.CopyText -> SheetIcon.Copy to stringResource(R.string.msg_copy_text)
                    MessageAction.CopyChannelLink -> SheetIcon.Link to (channelLinkLabel ?: stringResource(R.string.msg_copy_channel_link))
                    MessageAction.Delete -> SheetIcon.Delete to stringResource(R.string.msg_delete)
                    MessageAction.Remove -> SheetIcon.ShieldX to stringResource(R.string.msg_remove)
                    MessageAction.CopyId -> SheetIcon.Hash to stringResource(R.string.msg_copy_id)
                }
                SheetRow(icon, title, { onAction(a) }, danger = a == MessageAction.Delete || a == MessageAction.Remove)
            }
        }
    }
}

@Composable
private fun Divider() {
    Box(Modifier.fillMaxWidth().height(1.dp).background(Chord.colors.line))
}

/** The sender and the first two lines of the message. */
@Composable
private fun MessagePreview(message: MessageUi, modifier: Modifier = Modifier, surface: androidx.compose.ui.graphics.Color = Chord.colors.surface300) {
    Column(
        modifier.fillMaxWidth().clip(RoundedCornerShape(ChordRadius.md)).background(surface)
            .padding(horizontal = ChordSpace.s3, vertical = ChordSpace.s2),
    ) {
        Text(message.senderName, style = ChordType.name, color = Chord.colors.ink, maxLines = 1, overflow = TextOverflow.Ellipsis)
        val snippet = if (message.retracted) stringResource(R.string.msg_deleted_preview) else message.body
        Text(snippet, style = ChordType.bodySmall, color = Chord.colors.inkMuted, maxLines = 2, overflow = TextOverflow.Ellipsis)
    }
}

/** The question before a delete. [remove]: a moderator removes the message of someone else. */
@Composable
internal fun DeleteConfirmDialog(message: MessageUi, remove: Boolean, onConfirm: () -> Unit, onCancel: () -> Unit) {
    Dialog(onDismissRequest = onCancel) { DeleteConfirmCard(message, remove, onConfirm, onCancel) }
}

/** The card of the delete question, with no window. It reads as the desktop modal does. */
@Composable
internal fun DeleteConfirmCard(message: MessageUi, remove: Boolean, onConfirm: () -> Unit, onCancel: () -> Unit) {
    Column(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(ChordRadius.lg)).background(Chord.colors.surface200)
            .border(1.dp, Chord.colors.line, RoundedCornerShape(ChordRadius.lg))
            .padding(ChordSpace.s6),
    ) {
        Text(
            stringResource(if (remove) R.string.remove_title else R.string.delete_title),
            style = ChordType.title, color = Chord.colors.ink,
        )
        Spacer(Modifier.height(ChordSpace.s2))
        Text(
            stringResource(if (remove) R.string.remove_text else R.string.delete_text),
            style = ChordType.body, color = Chord.colors.inkMuted,
        )
        Spacer(Modifier.height(ChordSpace.s4))
        MessagePreview(message, Modifier.heightIn(max = 140.dp), surface = Chord.colors.surface100)
        Spacer(Modifier.height(ChordSpace.s6))
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End, verticalAlignment = Alignment.CenterVertically) {
            DialogButton(stringResource(R.string.delete_cancel), Chord.colors.ink, Chord.colors.surface300, onCancel)
            Spacer(Modifier.width(ChordSpace.s2))
            DialogButton(
                stringResource(if (remove) R.string.remove_confirm else R.string.delete_confirm),
                Chord.colors.onDanger, Chord.colors.danger, onConfirm,
            )
        }
    }
}

@Composable
private fun DialogButton(text: String, color: androidx.compose.ui.graphics.Color, background: androidx.compose.ui.graphics.Color, onClick: () -> Unit) {
    Box(
        Modifier.height(44.dp).clip(RoundedCornerShape(ChordRadius.md)).background(background)
            .clickable(role = Role.Button, onClick = onClick).padding(horizontal = ChordSpace.s4),
        contentAlignment = Alignment.Center,
    ) { Text(text, style = ChordType.label, color = color) }
}

/** The emoji picker for a reaction. Each pick is saved in the recent list. */
@Composable
fun ReactionPickerSheet(onDismiss: () -> Unit, onPick: (String) -> Unit) {
    val context = LocalContext.current
    val recent = remember { RecentEmoji.of(context) }
    val groups by produceState(EmojiCatalog.now()) { value = EmojiCatalog.load(context) }
    ChordModalSheet(onDismiss) { dismissThen ->
        ReactionPickerContent(
            groups = groups,
            recents = remember { recent.list() },
            onPick = { e ->
                recent.add(e)
                dismissThen { onPick(e) }
            },
            modifier = Modifier.fillMaxHeight(0.7f),
        )
    }
}

/** One cell of the grid: a header or an emoji. */
private sealed interface GridEntry {
    val key: String
    data class Header(val title: String) : GridEntry { override val key get() = "h:$title" }
    data class Cell(val emoji: String, val prefix: String) : GridEntry { override val key get() = "$prefix:$emoji" }
}

/** The inside of [ReactionPickerSheet], with no sheet window. [groups] is null while the data loads. */
@Composable
internal fun ReactionPickerContent(
    groups: List<EmojiGroup>?,
    recents: List<String>,
    onPick: (String) -> Unit,
    modifier: Modifier = Modifier,
    initialQuery: String = "",
) {
    var query by remember { mutableStateOf(initialQuery) }
    val grid = rememberLazyGridState()
    val scope = rememberCoroutineScope()

    // The list is built once for the data, not for each cell.
    val entries = remember(groups, recents) {
        buildList {
            if (recents.isNotEmpty()) {
                add(GridEntry.Header("Recent"))
                recents.forEach { add(GridEntry.Cell(it, "r")) }
            }
            groups?.forEach { g ->
                add(GridEntry.Header(g.label))
                g.emoji.forEach { add(GridEntry.Cell(it.emoji, "g")) }
            }
        }
    }
    val tabs = remember(entries) {
        entries.withIndex().filter { it.value is GridEntry.Header }.map { (it.value as GridEntry.Header).title to it.index }
    }
    val results = remember(groups, query) { if (query.isBlank() || groups == null) null else searchEmoji(groups, query) }

    Column(modifier.fillMaxWidth().navigationBarsPadding()) {
        SearchField(query, { query = it }, Modifier.padding(horizontal = ChordSpace.s4))
        if (results == null) {
            CategoryTabs(grid, tabs, onSelect = { index -> scope.launch { grid.scrollToItem(index) } })
        }
        Box(Modifier.fillMaxWidth().height(1.dp).background(Chord.colors.line))
        when {
            results != null && results.isEmpty() -> Box(Modifier.fillMaxWidth().padding(ChordSpace.s8), contentAlignment = Alignment.TopCenter) {
                Text("No emoji found", style = ChordType.body, color = Chord.colors.inkMuted, textAlign = TextAlign.Center)
            }
            results != null -> LazyVerticalGrid(
                columns = GridCells.Adaptive(CELL),
                modifier = Modifier.weight(1f).padding(horizontal = ChordSpace.s2),
            ) {
                items(results, key = { it.emoji }, contentType = { "cell" }) { e -> EmojiCell(e.emoji, onPick) }
            }
            else -> LazyVerticalGrid(
                columns = GridCells.Adaptive(CELL),
                state = grid,
                modifier = Modifier.weight(1f).padding(horizontal = ChordSpace.s2),
            ) {
                items(
                    entries,
                    key = { it.key },
                    span = { if (it is GridEntry.Header) GridItemSpan(maxLineSpan) else GridItemSpan(1) },
                    contentType = { if (it is GridEntry.Header) "header" else "cell" },
                ) { entry ->
                    when (entry) {
                        is GridEntry.Header -> Text(
                            entry.title, style = ChordType.caption, color = Chord.colors.inkMuted,
                            modifier = Modifier.padding(start = ChordSpace.s2, top = ChordSpace.s3, bottom = ChordSpace.s1),
                        )
                        is GridEntry.Cell -> EmojiCell(entry.emoji, onPick)
                    }
                }
            }
        }
    }
}

private val CELL = 44.dp

/** One emoji. It is a box and a text: no per-cell state. */
@Composable
private fun EmojiCell(emoji: String, onPick: (String) -> Unit) {
    Box(
        Modifier.size(CELL).clickable(role = Role.Button) { onPick(emoji) },
        contentAlignment = Alignment.Center,
    ) { Text(emoji, fontSize = 26.sp) }
}

@Composable
internal fun SearchField(value: String, onChange: (String) -> Unit, modifier: Modifier = Modifier, placeholder: String = "Search emoji") {
    val shape = RoundedCornerShape(ChordRadius.md)
    Row(
        modifier.fillMaxWidth().padding(bottom = ChordSpace.s2).height(44.dp).clip(shape)
            .background(Chord.colors.surface300).border(1.dp, Chord.colors.line, shape)
            .padding(horizontal = ChordSpace.s3),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        LineIcon(SheetIcon.Search, Chord.colors.inkMuted, 20.dp)
        Spacer(Modifier.width(ChordSpace.s2))
        Box(Modifier.weight(1f), contentAlignment = Alignment.CenterStart) {
            if (value.isEmpty()) Text(placeholder, style = ChordType.body, color = Chord.colors.inkMuted)
            BasicTextField(
                value = value,
                onValueChange = onChange,
                singleLine = true,
                textStyle = ChordType.body.copy(color = Chord.colors.ink),
                cursorBrush = SolidColor(Chord.colors.brand),
                keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search),
                modifier = Modifier.fillMaxWidth(),
            )
        }
    }
}

/** The category names. The one that holds the first visible row is selected. */
@Composable
private fun CategoryTabs(grid: LazyGridState, tabs: List<Pair<String, Int>>, onSelect: (Int) -> Unit) {
    val selected by remember(tabs) {
        derivedStateOf {
            val first = grid.firstVisibleItemIndex
            tabs.lastOrNull { it.second <= first }?.second ?: tabs.firstOrNull()?.second ?: -1
        }
    }
    Row(
        Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = ChordSpace.s3, vertical = ChordSpace.s1),
        horizontalArrangement = Arrangement.spacedBy(ChordSpace.s1),
    ) {
        tabs.forEach { (title, index) ->
            val on = index == selected
            Box(
                Modifier.height(36.dp).clip(RoundedCornerShape(ChordRadius.md))
                    .background(if (on) Chord.colors.brandSoft else androidx.compose.ui.graphics.Color.Transparent)
                    .clickable(role = Role.Tab) { onSelect(index) }
                    .padding(horizontal = ChordSpace.s3),
                contentAlignment = Alignment.Center,
            ) {
                Text(title, style = ChordType.label, color = if (on) Chord.colors.brandInk else Chord.colors.inkMuted, maxLines = 1)
            }
        }
    }
}

/** Two big choices: a photo or video, or a file. The pickers come later: these are callbacks only. */
@Composable
fun AttachmentSheet(onDismiss: () -> Unit, onPickImage: () -> Unit, onPickFile: () -> Unit) {
    ChordModalSheet(onDismiss) { dismissThen ->
        AttachmentContent(onPickImage = { dismissThen(onPickImage) }, onPickFile = { dismissThen(onPickFile) })
    }
}

/** The inside of [AttachmentSheet], with no sheet window. */
@Composable
internal fun AttachmentContent(onPickImage: () -> Unit, onPickFile: () -> Unit) {
    Column(Modifier.fillMaxWidth().navigationBarsPadding().padding(bottom = ChordSpace.s4)) {
        Text("Attach", style = ChordType.title, color = Chord.colors.ink, modifier = Modifier.padding(horizontal = ChordSpace.s4, vertical = ChordSpace.s2))
        SheetRow(SheetIcon.Image, "Photo or video", onPickImage, subtitle = "Pick from your gallery", large = true)
        SheetRow(SheetIcon.File, "File", onPickFile, subtitle = "Pick any file from your device", large = true)
    }
}

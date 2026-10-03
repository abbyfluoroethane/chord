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
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordRadius
import space.foid.chord.ui.theme.ChordSpace
import space.foid.chord.ui.theme.ChordType
import space.foid.chord.ui.timeline.MessageUi

/** The six emoji of the quick row. */
internal val QuickReactions = listOf("👍", "❤️", "😂", "😮", "😢", "🙏")

/**
 * The sheet for one message: a preview, quick reactions and the actions. Each action hides the
 * sheet first, then calls its callback and [onDismiss].
 */
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
    var confirming by remember { mutableStateOf(false) }
    ChordModalSheet(onDismiss) { dismissThen ->
        MessageActionsContent(
            message = message,
            canEdit = canEdit,
            canRetract = canRetract,
            onReply = { dismissThen(onReply) },
            onEdit = { dismissThen(onEdit) },
            onDelete = { confirming = true },
            onCopy = { dismissThen(onCopy) },
            onReact = { e -> dismissThen { onReact(e) } },
            onMoreReactions = { dismissThen(onMoreReactions) },
        )
        if (confirming) {
            DeleteConfirmDialog(
                onConfirm = {
                    confirming = false
                    dismissThen(onRetract)
                },
                onCancel = { confirming = false },
            )
        }
    }
}

/** The inside of [MessageActionsSheet], with no sheet window. */
@Composable
internal fun MessageActionsContent(
    message: MessageUi,
    canEdit: Boolean,
    canRetract: Boolean,
    onReply: () -> Unit,
    onEdit: () -> Unit,
    onDelete: () -> Unit,
    onCopy: () -> Unit,
    onReact: (String) -> Unit,
    onMoreReactions: () -> Unit,
) {
    Column(Modifier.fillMaxWidth().navigationBarsPadding().padding(bottom = ChordSpace.s2)) {
        MessagePreview(message, Modifier.padding(horizontal = ChordSpace.s4))
        Spacer(Modifier.height(ChordSpace.s3))
        Row(
            Modifier.fillMaxWidth().padding(horizontal = ChordSpace.s4),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            QuickReactions.forEach { e ->
                Box(
                    Modifier.size(44.dp).clip(RoundedCornerShape(ChordRadius.md)).background(Chord.colors.surface300)
                        .clickable(role = Role.Button) { onReact(e) }
                        .semantics { contentDescription = "React $e" },
                    contentAlignment = Alignment.Center,
                ) { Text(e, fontSize = 24.sp) }
            }
            Box(
                Modifier.size(44.dp).clip(RoundedCornerShape(ChordRadius.md)).background(Chord.colors.surface300)
                    .clickable(role = Role.Button, onClick = onMoreReactions)
                    .semantics { contentDescription = "More reactions" },
                contentAlignment = Alignment.Center,
            ) { LineIcon(SheetIcon.Plus, Chord.colors.ink) }
        }
        Spacer(Modifier.height(ChordSpace.s2))
        Box(Modifier.fillMaxWidth().height(1.dp).background(Chord.colors.line))
        SheetRow(SheetIcon.Reply, "Reply", onReply)
        if (canEdit) SheetRow(SheetIcon.Edit, "Edit", onEdit)
        SheetRow(SheetIcon.Copy, "Copy text", onCopy)
        if (canRetract) SheetRow(SheetIcon.Delete, "Delete", onDelete, danger = true)
    }
}

/** The sender and the first two lines of the message. */
@Composable
private fun MessagePreview(message: MessageUi, modifier: Modifier = Modifier) {
    Column(
        modifier.fillMaxWidth().clip(RoundedCornerShape(ChordRadius.md)).background(Chord.colors.surface300)
            .padding(horizontal = ChordSpace.s3, vertical = ChordSpace.s2),
    ) {
        Text(message.senderName, style = ChordType.name, color = Chord.colors.ink, maxLines = 1, overflow = TextOverflow.Ellipsis)
        val snippet = if (message.retracted) "This message was deleted" else message.body
        Text(snippet, style = ChordType.bodySmall, color = Chord.colors.inkMuted, maxLines = 2, overflow = TextOverflow.Ellipsis)
    }
}

/** The question before a delete. */
@Composable
internal fun DeleteConfirmDialog(onConfirm: () -> Unit, onCancel: () -> Unit) {
    Dialog(onDismissRequest = onCancel) { DeleteConfirmCard(onConfirm, onCancel) }
}

/** The card of the delete question, with no window. */
@Composable
internal fun DeleteConfirmCard(onConfirm: () -> Unit, onCancel: () -> Unit) {
    Column(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(ChordRadius.lg)).background(Chord.colors.surface200)
            .border(1.dp, Chord.colors.line, RoundedCornerShape(ChordRadius.lg))
            .padding(ChordSpace.s6),
    ) {
        Text("Delete this message?", style = ChordType.title, color = Chord.colors.ink)
        Spacer(Modifier.height(ChordSpace.s2))
        Text("Everyone in the chat will see that it was deleted. You cannot undo this.", style = ChordType.body, color = Chord.colors.inkMuted)
        Spacer(Modifier.height(ChordSpace.s6))
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End, verticalAlignment = Alignment.CenterVertically) {
            DialogButton("Cancel", Chord.colors.ink, Chord.colors.surface300, onCancel)
            Spacer(Modifier.width(ChordSpace.s2))
            DialogButton("Delete", Chord.colors.onDanger, Chord.colors.danger, onConfirm)
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
private fun SearchField(value: String, onChange: (String) -> Unit, modifier: Modifier = Modifier) {
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
            if (value.isEmpty()) Text("Search emoji", style = ChordType.body, color = Chord.colors.inkMuted)
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

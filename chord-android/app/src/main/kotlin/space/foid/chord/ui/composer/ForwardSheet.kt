package space.foid.chord.ui.composer

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import space.foid.chord.R
import space.foid.chord.ui.sheets.ChordModalSheet
import space.foid.chord.ui.sheets.LineIcon
import space.foid.chord.ui.sheets.SearchField
import space.foid.chord.ui.sheets.SheetIcon
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordRadius
import space.foid.chord.ui.theme.ChordSpace
import space.foid.chord.ui.theme.ChordType

/**
 * Forward a message: pick a chat from a searchable list, then press Forward. The text goes as a
 * new message ([onForward] gets the target). [targets] is the chat list.
 */
@Composable
fun ForwardSheet(
    senderName: String,
    summary: String,
    targets: List<ForwardTarget>,
    onDismiss: () -> Unit,
    onForward: (ForwardTarget) -> Unit,
) {
    ChordModalSheet(onDismiss) { dismissThen ->
        ForwardContent(
            senderName = senderName,
            summary = summary,
            targets = targets,
            onCancel = { dismissThen {} },
            onForward = { t -> dismissThen { onForward(t) } },
            modifier = Modifier.fillMaxHeight(0.85f),
        )
    }
}

/** The inside of [ForwardSheet], with no sheet window. */
@Composable
internal fun ForwardContent(
    senderName: String,
    summary: String,
    targets: List<ForwardTarget>,
    onCancel: () -> Unit,
    onForward: (ForwardTarget) -> Unit,
    modifier: Modifier = Modifier,
    initialQuery: String = "",
    initialSelected: String? = null,
) {
    val colors = Chord.colors
    var query by remember { mutableStateOf(initialQuery) }
    var selected by remember { mutableStateOf(initialSelected) }
    val hits = remember(targets, query) { filterForwardTargets(targets, query) }
    // The choice stays only while the row is in the list.
    val chosen = hits.firstOrNull { it.jid == selected }
    Column(modifier.fillMaxWidth().imePadding().navigationBarsPadding()) {
        Text(
            stringResource(R.string.forward_title), style = ChordType.title, color = colors.ink,
            modifier = Modifier.padding(horizontal = ChordSpace.s4, vertical = ChordSpace.s2),
        )
        Row(
            Modifier.fillMaxWidth().padding(horizontal = ChordSpace.s4).clip(RoundedCornerShape(ChordRadius.md))
                .background(colors.surface100).height(IntrinsicSize.Min),
        ) {
            Box(Modifier.width(3.dp).fillMaxHeight().background(colors.brand))
            Column(Modifier.padding(horizontal = ChordSpace.s3, vertical = ChordSpace.s2)) {
                Text(senderName, style = ChordType.name, color = colors.ink, maxLines = 1, overflow = TextOverflow.Ellipsis)
                Text(summary, style = ChordType.bodySmall, color = colors.inkMuted, maxLines = 3, overflow = TextOverflow.Ellipsis)
            }
        }
        Spacer(Modifier.height(ChordSpace.s3))
        SearchField(
            query, { query = it }, Modifier.padding(horizontal = ChordSpace.s4),
            placeholder = stringResource(R.string.forward_search),
        )
        LazyColumn(Modifier.weight(1f).fillMaxWidth()) {
            if (hits.isEmpty()) {
                item {
                    Text(
                        stringResource(R.string.forward_none), style = ChordType.body, color = colors.inkMuted,
                        modifier = Modifier.padding(ChordSpace.s4),
                    )
                }
            }
            items(hits, key = { it.jid }) { t ->
                val on = t.jid == chosen?.jid
                Row(
                    Modifier.fillMaxWidth().height(52.dp).background(if (on) colors.selected else androidx.compose.ui.graphics.Color.Transparent)
                        .clickable(role = Role.RadioButton) { selected = t.jid }
                        .padding(horizontal = ChordSpace.s4),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    LineIcon(if (t.direct) SheetIcon.Reply else SheetIcon.Hash, colors.inkMuted, 20.dp)
                    Spacer(Modifier.width(ChordSpace.s3))
                    Text(t.label, style = ChordType.body, color = colors.ink, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
                    Spacer(Modifier.width(ChordSpace.s2))
                    Text(t.hint, style = ChordType.bodySmall, color = colors.inkMuted, maxLines = 1)
                    if (on) {
                        Spacer(Modifier.width(ChordSpace.s2))
                        LineIcon(SheetIcon.Check, colors.brandInk, 20.dp)
                    }
                }
            }
        }
        Box(Modifier.fillMaxWidth().height(1.dp).background(colors.line))
        Row(
            Modifier.fillMaxWidth().padding(ChordSpace.s4),
            horizontalArrangement = Arrangement.End,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            FooterButton(stringResource(R.string.delete_cancel), colors.ink, colors.surface300, true, onCancel)
            Spacer(Modifier.width(ChordSpace.s2))
            FooterButton(
                stringResource(R.string.forward_button),
                colors.onBrand, if (chosen != null) colors.brand else colors.surface300,
                chosen != null,
            ) { chosen?.let(onForward) }
        }
    }
}

@Composable
private fun FooterButton(
    text: String,
    color: androidx.compose.ui.graphics.Color,
    background: androidx.compose.ui.graphics.Color,
    enabled: Boolean,
    onClick: () -> Unit,
) {
    Box(
        Modifier.height(44.dp).clip(RoundedCornerShape(ChordRadius.md)).background(background)
            .clickable(enabled = enabled, role = Role.Button, onClick = onClick).padding(horizontal = ChordSpace.s6),
        contentAlignment = Alignment.Center,
    ) { Text(text, style = ChordType.label, color = if (enabled) color else Chord.colors.inkMuted) }
}

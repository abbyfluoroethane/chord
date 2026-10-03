package space.foid.chord.ui.inbox

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import space.foid.chord.R
import space.foid.chord.ui.components.Avatar
import space.foid.chord.ui.join.JoinButton
import space.foid.chord.ui.join.JoinError
import space.foid.chord.ui.join.JoinTextButton
import space.foid.chord.ui.join.JoinTitle
import space.foid.chord.ui.sheets.ChordModalSheet
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordRadius
import space.foid.chord.ui.theme.ChordSpace
import space.foid.chord.ui.theme.ChordType
import space.foid.chord.viewmodel.InboxState
import space.foid.chord.viewmodel.InboxViewModel
import space.foid.chord.viewmodel.InviteItem
import space.foid.chord.viewmodel.RequestItem

/** What the buttons of the inbox sheet do. */
class InboxCallbacks(
    val onAcceptRequest: (RequestItem) -> Unit = {},
    val onDenyRequest: (RequestItem) -> Unit = {},
    val onAddBack: (RequestItem, Boolean) -> Unit = { _, _ -> },
    val onAcceptInvite: (InviteItem) -> Unit = {},
    val onDeclineInvite: (InviteItem) -> Unit = {},
) {
    companion object {
        fun of(vm: InboxViewModel) = InboxCallbacks(
            onAcceptRequest = vm::accept,
            onDenyRequest = vm::deny,
            onAddBack = { r, on -> vm.setAddBack(r.jid, on) },
            onAcceptInvite = vm::accept,
            onDeclineInvite = vm::decline,
        )
    }
}

/** The sheet with the contact requests and room invitations that wait for an answer. */
@Composable
fun InboxSheet(state: InboxState, callbacks: InboxCallbacks, onDismiss: () -> Unit) {
    ChordModalSheet(onDismiss) { _ -> InboxContent(state, callbacks) }
}

/** The inside of [InboxSheet], with no sheet window. */
@Composable
fun InboxContent(state: InboxState, callbacks: InboxCallbacks, modifier: Modifier = Modifier) {
    val c = Chord.colors
    Column(
        modifier
            .fillMaxWidth()
            .navigationBarsPadding()
            .heightIn(max = 560.dp)
            .verticalScroll(rememberScrollState())
            .padding(horizontal = ChordSpace.s4)
            .padding(bottom = ChordSpace.s4)
            .testTag("inbox_sheet"),
    ) {
        JoinTitle(stringResource(R.string.inbox_title))
        Spacer(Modifier.height(ChordSpace.s3))
        if (state.isEmpty) {
            Text(
                stringResource(R.string.inbox_empty), style = ChordType.body, color = c.inkMuted,
                modifier = Modifier.fillMaxWidth().padding(vertical = ChordSpace.s6).testTag("inbox_empty"),
            )
        }
        if (state.requests.isNotEmpty()) {
            Section(stringResource(R.string.inbox_requests))
            state.requests.forEach { r -> RequestCard(r, r.id in state.busy, callbacks) }
        }
        if (state.invites.isNotEmpty()) {
            Section(stringResource(R.string.inbox_invites))
            state.invites.forEach { i -> InviteCard(i, i.id in state.busy, callbacks) }
        }
        if (state.pendingSpaces.isNotEmpty()) {
            Section(stringResource(R.string.inbox_pending))
            state.pendingSpaces.forEach { s ->
                Card(Modifier.testTag("pending_space_${s.node}")) {
                    Text(s.name.ifBlank { s.node }, style = ChordType.name, color = c.ink, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    Text(stringResource(R.string.inbox_pending_text), style = ChordType.bodySmall, color = c.inkMuted)
                }
            }
        }
        if (state.error != null) {
            Spacer(Modifier.height(ChordSpace.s2))
            JoinError(state.error, "inbox_error")
        }
    }
}

@Composable
private fun Section(title: String) {
    Text(
        title.uppercase(), style = ChordType.caption, color = Chord.colors.inkMuted,
        modifier = Modifier.fillMaxWidth().padding(top = ChordSpace.s3, bottom = ChordSpace.s2),
    )
}

@Composable
private fun Card(modifier: Modifier = Modifier, content: @Composable () -> Unit) {
    Column(
        modifier.fillMaxWidth().padding(bottom = ChordSpace.s2)
            .clip(RoundedCornerShape(ChordRadius.md)).background(Chord.colors.surface300).padding(ChordSpace.s3),
    ) { content() }
}

@Composable
private fun RequestCard(r: RequestItem, busy: Boolean, cb: InboxCallbacks) {
    val c = Chord.colors
    val name = r.jid.substringBefore('@')
    Card(Modifier.testTag("request_${r.jid}")) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(ChordSpace.s3)) {
            Avatar(r.jid, name = name, size = 40.dp, cut = c.surface300)
            Column(Modifier.weight(1f)) {
                Text(name, style = ChordType.name, color = c.ink, maxLines = 1, overflow = TextOverflow.Ellipsis)
                Text(r.jid, style = ChordType.caption, color = c.inkMuted, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
        Text(
            stringResource(R.string.inbox_request_text), style = ChordType.bodySmall, color = c.inkMuted,
            modifier = Modifier.padding(top = ChordSpace.s2),
        )
        AddBackToggle(r.addBack, enabled = !busy) { cb.onAddBack(r, it) }
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(ChordSpace.s2)) {
            JoinTextButton(
                stringResource(R.string.inbox_deny), { cb.onDenyRequest(r) }, "request_deny_${r.jid}",
                Modifier.weight(1f), enabled = !busy,
            )
            JoinButton(
                stringResource(R.string.inbox_accept), { cb.onAcceptRequest(r) }, "request_accept_${r.jid}",
                Modifier.weight(1f), enabled = !busy, busy = busy, compact = true,
            )
        }
    }
}

@Composable
private fun AddBackToggle(on: Boolean, enabled: Boolean, onChange: (Boolean) -> Unit) {
    val c = Chord.colors
    val desc = stringResource(R.string.inbox_add_back)
    Row(
        Modifier.fillMaxWidth().height(44.dp)
            .clickable(enabled = enabled, role = Role.Checkbox) { onChange(!on) }
            .semantics { contentDescription = desc },
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(
            Modifier.size(20.dp).clip(RoundedCornerShape(ChordRadius.sm))
                .background(if (on) c.brand else androidx.compose.ui.graphics.Color.Transparent)
                .drawBehind {
                    if (!on) {
                        drawRoundRect(
                            c.lineStrong, cornerRadius = androidx.compose.ui.geometry.CornerRadius(4.dp.toPx()),
                            style = Stroke(2.dp.toPx()),
                        )
                    } else {
                        val k = size.width / 24f
                        val p = Path().apply {
                            moveTo(6 * k, 12.5f * k); lineTo(10.5f * k, 17 * k); lineTo(18 * k, 8 * k)
                        }
                        drawPath(p, c.onBrand, style = Stroke(2.4f * k, cap = StrokeCap.Round, join = StrokeJoin.Round))
                    }
                },
        )
        Spacer(Modifier.width(ChordSpace.s3))
        Text(desc, style = ChordType.body, color = c.ink)
    }
}

@Composable
private fun InviteCard(i: InviteItem, busy: Boolean, cb: InboxCallbacks) {
    val c = Chord.colors
    val name = i.room.substringBefore('@')
    Card(Modifier.testTag("invite_${i.room}")) {
        Text("#$name", style = ChordType.name, color = c.ink, maxLines = 1, overflow = TextOverflow.Ellipsis)
        Text(i.room, style = ChordType.caption, color = c.inkMuted, maxLines = 1, overflow = TextOverflow.Ellipsis)
        Text(
            stringResource(R.string.inbox_invite_from, i.from), style = ChordType.bodySmall, color = c.inkMuted,
            modifier = Modifier.padding(top = ChordSpace.s2),
        )
        if (!i.reason.isNullOrBlank()) {
            Text("“${i.reason}”", style = ChordType.bodySmall, color = c.ink, modifier = Modifier.padding(top = ChordSpace.s1))
        }
        Spacer(Modifier.height(ChordSpace.s3))
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(ChordSpace.s2)) {
            JoinTextButton(
                stringResource(R.string.inbox_decline), { cb.onDeclineInvite(i) }, "invite_decline_${i.room}",
                Modifier.weight(1f), enabled = !busy,
            )
            JoinButton(
                stringResource(R.string.inbox_join), { cb.onAcceptInvite(i) }, "invite_accept_${i.room}",
                Modifier.weight(1f), enabled = !busy, busy = busy, compact = true,
            )
        }
    }
}

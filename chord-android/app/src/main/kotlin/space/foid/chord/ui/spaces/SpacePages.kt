package space.foid.chord.ui.spaces

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import space.foid.chord.R
import space.foid.chord.ui.components.Avatar
import space.foid.chord.ui.join.JoinButton
import space.foid.chord.ui.join.JoinError
import space.foid.chord.ui.join.JoinField
import space.foid.chord.ui.join.JoinTextButton
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordRadius
import space.foid.chord.ui.theme.ChordSpace
import space.foid.chord.ui.theme.ChordType
import space.foid.chord.viewmodel.SpaceState
import uniffi.chord_ffi.ChannelItem
import uniffi.chord_ffi.Contact
import uniffi.chord_ffi.JoinRequest
import uniffi.chord_ffi.SpaceMember

/**
 * A full-screen page, for what is too big for a sheet: the invite list and the space settings.
 * [bottom] is pinned under the scrolling [content]. The window is drawn by [SpacePage].
 */
@Composable
fun SpacePageFrame(
    title: String,
    onClose: () -> Unit,
    modifier: Modifier = Modifier,
    bottom: (@Composable () -> Unit)? = null,
    content: @Composable ColumnScope.() -> Unit,
) {
    val c = Chord.colors
    Column(modifier.fillMaxSize().background(c.surface100).statusBarsPadding().navigationBarsPadding().imePadding()) {
        Row(Modifier.fillMaxWidth().height(56.dp), verticalAlignment = Alignment.CenterVertically) {
            Box(
                Modifier.size(56.dp).clickable(role = Role.Button, onClick = onClose)
                    .semantics { contentDescription = "Close" }.testTag("page_close"),
                contentAlignment = Alignment.Center,
            ) { SpaceGlyph(SpaceIcon.Close, c.ink) }
            Text(title, style = ChordType.title, color = c.ink, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        SheetDivider()
        Column(Modifier.weight(1f).fillMaxWidth().verticalScroll(rememberScrollState()), content = content)
        if (bottom != null) {
            SheetDivider()
            Box(Modifier.fillMaxWidth().padding(ChordSpace.s4)) { bottom() }
        }
    }
}

/** [SpacePageFrame] in a window that covers the screen. */
@Composable
fun SpacePage(
    title: String,
    onClose: () -> Unit,
    bottom: (@Composable () -> Unit)? = null,
    content: @Composable ColumnScope.() -> Unit,
) {
    space.foid.chord.ui.components.MotionDialog(onClose, space.foid.chord.ui.components.DialogMotion.Slide) { close ->
        SpacePageFrame(title, close, bottom = bottom, content = content)
    }
}

@Composable
private fun Hint(text: String, modifier: Modifier = Modifier) {
    Text(
        text, style = ChordType.bodySmall, color = Chord.colors.inkMuted,
        modifier = modifier.fillMaxWidth().padding(horizontal = ChordSpace.s4, vertical = ChordSpace.s2),
    )
}

// ---- Invite ----

/** The inside of the "Invite people" page: contacts to pick, and the link to copy. */
@Composable
fun InviteContent(
    target: SpaceTarget,
    contacts: List<Contact>?,
    picked: Set<String>,
    query: String,
    copied: Boolean,
    error: String?,
    onQuery: (String) -> Unit,
    onToggle: (String) -> Unit,
    onCopy: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val c = Chord.colors
    val link = spaceInviteLink(target.service, target.node)
    Column(modifier.fillMaxWidth().testTag("invite_page")) {
        Hint(stringResource(R.string.spaces_invite_hint, target.name))
        JoinField(
            query, onQuery, stringResource(R.string.spaces_invite_search), "invite_search",
            modifier = Modifier.padding(horizontal = ChordSpace.s4),
        )
        Spacer(Modifier.height(ChordSpace.s2))
        when {
            contacts == null -> Hint(stringResource(R.string.spaces_loading))
            else -> {
                val shown = filterContacts(contacts, query)
                if (shown.isEmpty()) Hint(stringResource(if (contacts.isEmpty()) R.string.spaces_invite_no_contacts else R.string.spaces_invite_no_match))
                shown.forEach { ct ->
                    val on = ct.jid in picked
                    Row(
                        Modifier.fillMaxWidth().height(60.dp)
                            .clickable(role = Role.Checkbox) { onToggle(ct.jid) }
                            .semantics { selected = on }
                            .padding(horizontal = ChordSpace.s4).testTag("invite_${ct.jid}"),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(ChordSpace.s3),
                    ) {
                        Avatar(ct.jid, name = ct.shownName(), size = 36.dp)
                        Column(Modifier.weight(1f)) {
                            Text(ct.shownName(), style = ChordType.body, color = c.ink, maxLines = 1, overflow = TextOverflow.Ellipsis)
                            Text(ct.jid, style = ChordType.caption, color = c.inkMuted, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        }
                        CheckBox(on)
                    }
                }
            }
        }
        SheetDivider()
        Hint(stringResource(R.string.spaces_invite_or_copy))
        Row(
            Modifier.fillMaxWidth().padding(horizontal = ChordSpace.s4),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(ChordSpace.s2),
        ) {
            Text(
                link, style = ChordType.bodySmall.copy(fontFamily = FontFamily.Monospace), color = c.ink,
                maxLines = 2, overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f).clip(RoundedCornerShape(ChordRadius.md)).background(c.surface300).padding(ChordSpace.s3)
                    .testTag("invite_link"),
            )
            JoinTextButton(
                stringResource(if (copied) R.string.spaces_copied else R.string.spaces_copy), onCopy, "invite_copy",
            )
        }
        if (error != null) JoinError(error, "invite_error", Modifier.padding(ChordSpace.s4))
        Spacer(Modifier.height(ChordSpace.s4))
    }
}

@Composable
internal fun CheckBox(on: Boolean) {
    val c = Chord.colors
    Box(
        Modifier.size(22.dp).clip(RoundedCornerShape(5.dp))
            .background(if (on) c.brand else androidx.compose.ui.graphics.Color.Transparent)
            .border(2.dp, if (on) c.brand else c.lineStrong, RoundedCornerShape(5.dp)),
        contentAlignment = Alignment.Center,
    ) { if (on) SpaceGlyph(SpaceIcon.Check, c.onBrand, size = 16.dp) }
}

/** The button under the invite list. */
@Composable
fun InviteButton(count: Int, busy: Boolean, onSend: () -> Unit) {
    JoinButton(
        text = if (count > 1) pluralStringResource(R.plurals.spaces_send_invites, count, count) else stringResource(R.string.spaces_send_invite),
        onClick = onSend, tag = "invite_send", modifier = Modifier.fillMaxWidth(), enabled = count > 0, busy = busy,
    )
}

// ---- Settings ----

/** What the settings page does. The defaults do nothing, for previews. */
class SettingsCallbacks(
    val onName: (String) -> Unit = {},
    val onDescription: (String) -> Unit = {},
    val onPickImage: (banner: Boolean) -> Unit = {},
    val onMember: (jid: String, ban: Boolean) -> Unit = { _, _ -> },
    val onRequest: (jid: String, approve: Boolean) -> Unit = { _, _ -> },
    val onRemoveChannel: (room: String) -> Unit = {},
    val onDelete: () -> Unit = {},
)

/**
 * The inside of the "Space settings" page: name, description, images, join requests, members,
 * channels and the danger zone. Without [SpaceState.owner] it is read only.
 */
@Composable
fun SettingsContent(
    state: SpaceState,
    name: String,
    description: String,
    cb: SettingsCallbacks,
    modifier: Modifier = Modifier,
) {
    val c = Chord.colors
    val owner = state.owner == true
    val enabled = owner && !state.busy
    Column(modifier.fillMaxWidth().testTag("settings_page")) {
        if (!owner && state.owner == false) Hint(stringResource(R.string.spaces_settings_owner_only))
        Column(Modifier.padding(horizontal = ChordSpace.s4), verticalArrangement = Arrangement.spacedBy(ChordSpace.s3)) {
            Spacer(Modifier.height(ChordSpace.s2))
            JoinField(name, cb.onName, stringResource(R.string.spaces_settings_name), "settings_space_name", enabled = enabled)
            JoinField(
                description, cb.onDescription, stringResource(R.string.spaces_settings_description), "settings_space_description",
                enabled = enabled, singleLine = false, minLines = 3,
            )
        }
        if (owner) {
            SheetSectionLabel(stringResource(R.string.spaces_settings_images))
            SpaceSheetRow(SpaceIcon.Image, stringResource(R.string.spaces_settings_avatar), { cb.onPickImage(false) }, Modifier.testTag("settings_avatar"), enabled = !state.busy)
            SpaceSheetRow(SpaceIcon.Image, stringResource(R.string.spaces_settings_banner), { cb.onPickImage(true) }, Modifier.testTag("settings_banner"), enabled = !state.busy)
            SheetSectionLabel(stringResource(R.string.spaces_settings_requests))
            if (state.requests.isEmpty()) Hint(stringResource(R.string.spaces_settings_no_requests))
            state.requests.forEach { r -> RequestRow(r, !state.busy, cb.onRequest) }
            SheetSectionLabel(stringResource(R.string.spaces_settings_members))
            state.members.forEach { m -> MemberRow(m, !state.busy, cb.onMember) }
            SheetSectionLabel(stringResource(R.string.spaces_settings_channels))
            val rooms = state.rooms
            if (rooms.isEmpty()) Hint(stringResource(R.string.spaces_settings_no_channels))
            rooms.forEach { ch -> ChannelRow(ch, !state.busy, cb.onRemoveChannel) }
            SheetSectionLabel(stringResource(R.string.spaces_settings_danger))
            JoinButton(
                stringResource(R.string.spaces_settings_delete), cb.onDelete, "settings_delete",
                Modifier.padding(horizontal = ChordSpace.s4), enabled = !state.busy, danger = true, compact = true,
            )
        }
        if (state.error != null) JoinError(state.error, "settings_error", Modifier.padding(ChordSpace.s4))
        Spacer(Modifier.height(ChordSpace.s6))
    }
}

@Composable
private fun PersonLine(text: String, label: String?, modifier: Modifier = Modifier, actions: @Composable () -> Unit) {
    val c = Chord.colors
    Row(
        modifier.fillMaxWidth().padding(horizontal = ChordSpace.s4, vertical = ChordSpace.s1),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(ChordSpace.s2),
    ) {
        Column(Modifier.weight(1f)) {
            Text(text, style = ChordType.bodySmall.copy(fontFamily = FontFamily.Monospace), color = c.ink, maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (label != null) Text(label, style = ChordType.caption, color = c.inkMuted)
        }
        actions()
    }
}

@Composable
private fun RequestRow(r: JoinRequest, enabled: Boolean, onRequest: (String, Boolean) -> Unit) {
    PersonLine(r.jid, null, Modifier.testTag("request_${r.jid}")) {
        JoinButton(stringResource(R.string.spaces_approve), { onRequest(r.jid, true) }, "approve_${r.jid}", enabled = enabled, compact = true)
        JoinTextButton(stringResource(R.string.spaces_deny), { onRequest(r.jid, false) }, "deny_${r.jid}", enabled = enabled)
    }
}

@Composable
private fun MemberRow(m: SpaceMember, enabled: Boolean, onMember: (String, Boolean) -> Unit) {
    PersonLine(m.jid, affiliationLabel(m.affiliation), Modifier.testTag("member_${m.jid}")) {
        if (m.affiliation != "owner") {
            JoinTextButton(
                stringResource(if (m.affiliation == "outcast") R.string.spaces_unban else R.string.spaces_remove),
                { onMember(m.jid, false) }, "remove_${m.jid}", enabled = enabled,
            )
            if (m.affiliation != "outcast") {
                JoinTextButton(
                    stringResource(R.string.spaces_ban), { onMember(m.jid, true) }, "ban_${m.jid}",
                    enabled = enabled, color = Chord.colors.danger,
                )
            }
        }
    }
}

@Composable
private fun ChannelRow(ch: ChannelItem, enabled: Boolean, onRemove: (String) -> Unit) {
    PersonLine("#" + ch.name.ifBlank { ch.jid.substringBefore('@') }, null, Modifier.testTag("channel_${ch.jid}")) {
        JoinTextButton(stringResource(R.string.spaces_remove_from_space), { onRemove(ch.jid) }, "remove_channel_${ch.jid}", enabled = enabled)
    }
}

/** The Save button of the settings page. */
@Composable
fun SettingsSaveButton(enabled: Boolean, busy: Boolean, onSave: () -> Unit) {
    JoinButton(
        stringResource(R.string.spaces_save), onSave, "settings_save", Modifier.fillMaxWidth(),
        enabled = enabled && !busy, busy = busy,
    )
}

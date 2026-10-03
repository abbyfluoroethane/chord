package space.foid.chord.ui.home

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
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
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.em
import androidx.compose.ui.unit.sp
import space.foid.chord.R
import space.foid.chord.data.stableKey
import space.foid.chord.ui.avatar.rememberAvatarBitmap
import space.foid.chord.ui.components.ChannelKind
import space.foid.chord.ui.components.ChannelListItem
import space.foid.chord.ui.components.CountBadge
import space.foid.chord.ui.contacts.contactPresence
import space.foid.chord.ui.screens.bareJid
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordRadius
import space.foid.chord.ui.theme.ChordSpace
import space.foid.chord.ui.theme.ChordType
import uniffi.chord_ffi.ChannelItem
import uniffi.chord_ffi.Contact
import uniffi.chord_ffi.ChannelKind as FfiChannelKind

/** The subtitle of a group chat: "4 Members". Null for a direct chat or when the core gave no count. */
fun groupMemberCount(ch: ChannelItem): Int? =
    if (ch.kind is FfiChannelKind.Room) ch.members?.toInt() else null

/**
 * The Home scope of the channel drawer, as on the desktop (ChannelSidebar.svelte): a "Find or
 * start a chat" button, the "Contacts" row with the number of requests, then "Messages".
 *
 * @param contacts the roster, for the presence dots of the direct chats
 * @param pending the number of contact requests that wait for an answer
 * @param onFind opens the full-screen search. [onContacts] opens the contacts page.
 */
@Composable
fun HomeListContent(
    channels: List<ChannelItem>,
    loaded: Boolean,
    selectedJid: String?,
    contacts: List<Contact>,
    pending: Int,
    onFind: () -> Unit,
    onContacts: () -> Unit,
    onSelect: (ChannelItem) -> Unit,
    onLongPress: (ChannelItem) -> Unit,
    modifier: Modifier = Modifier,
) {
    val c = Chord.colors
    val byJid = remember(contacts) { contacts.associateBy { it.jid.lowercase() } }
    Column(modifier) {
        FindButton(onFind)
        LazyColumn(Modifier.weight(1f).fillMaxWidth().testTag("channel_list")) {
            item(key = "contacts") { ContactsRow(pending, onContacts) }
            item(key = "header/messages") { MessagesHeader() }
            val shown = channels
            if (shown.isEmpty()) {
                item(key = "empty") {
                    Text(
                        stringResource(if (loaded) R.string.home_empty else R.string.home_loading),
                        style = ChordType.bodySmall, color = c.inkMuted,
                        modifier = Modifier.padding(horizontal = ChordSpace.s4, vertical = ChordSpace.s2),
                    )
                }
            }
            items(shown, key = { it.stableKey() }) { ch ->
                val bare = bareJid(ch.jid)
                val members = groupMemberCount(ch)
                val onRow = { onSelect(ch) }
                val onLong = { onLongPress(ch) }
                val tag = Modifier.testTag("channel_item_$bare")
                if (ch.kind is FfiChannelKind.Room) {
                    GroupRow(
                        name = ch.name.ifBlank { bare }, selected = ch.jid == selectedJid, unread = ch.unread.toInt(),
                        members = members, onClick = onRow, onLongClick = onLong, modifier = tag,
                    )
                } else {
                    ChannelListItem(
                        name = ch.name.ifBlank { bare },
                        selected = ch.jid == selectedJid,
                        onClick = onRow,
                        onLongClick = onLong,
                        kind = ChannelKind.Dm,
                        jid = bare,
                        image = if (ch.kind is FfiChannelKind.PrivateMessage) null else rememberAvatarBitmap(bare, null, 36.dp),
                        presence = byJid[bare.lowercase()]?.contactPresence(),
                        unread = ch.unread.toInt(),
                        modifier = tag,
                    )
                }
            }
        }
    }
}

/** The search button at the top: it looks like a field and opens the full-screen search. */
@Composable
internal fun FindButton(onClick: () -> Unit, modifier: Modifier = Modifier) {
    val c = Chord.colors
    val label = stringResource(R.string.home_find)
    Row(
        modifier
            .fillMaxWidth()
            .padding(horizontal = ChordSpace.s2, vertical = ChordSpace.s1)
            .height(44.dp)
            .clip(RoundedCornerShape(ChordRadius.md))
            .background(c.surface100)
            .clickable(role = Role.Button, onClick = onClick)
            .padding(horizontal = ChordSpace.s3)
            .testTag("home_find"),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(ChordSpace.s2),
    ) {
        HomeGlyphIcon(HomeGlyph.Search, c.inkMuted, size = 18.dp)
        Text(label, style = ChordType.label, color = c.inkMuted, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

/** The "Contacts" row with the badge of pending requests. */
@Composable
internal fun ContactsRow(pending: Int, onClick: () -> Unit, modifier: Modifier = Modifier) {
    val c = Chord.colors
    val desc = if (pending > 0) pluralStringResource(R.plurals.home_contacts_pending, pending, pending)
    else stringResource(R.string.home_contacts)
    Box(modifier.fillMaxWidth().padding(horizontal = ChordSpace.s2, vertical = 1.dp)) {
        Row(
            Modifier
                .fillMaxWidth()
                .height(48.dp)
                .clip(RoundedCornerShape(ChordRadius.sm))
                .clickable(role = Role.Button, onClick = onClick)
                .padding(horizontal = ChordSpace.s2)
                .semantics(mergeDescendants = true) { contentDescription = desc }
                .testTag("home_contacts"),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(ChordSpace.s3),
        ) {
            Box(Modifier.size(36.dp), contentAlignment = Alignment.Center) { HomeGlyphIcon(HomeGlyph.People, c.inkMuted, size = 22.dp) }
            Text(
                stringResource(R.string.home_contacts),
                style = ChordType.body.copy(fontWeight = FontWeight.Medium, lineHeight = 20.sp), color = c.inkMuted,
                modifier = Modifier.weight(1f),
            )
            if (pending > 0) CountBadge(pending, modifier = Modifier.testTag("home_contacts_badge"))
        }
    }
}

@Composable
private fun MessagesHeader() {
    Text(
        stringResource(R.string.home_messages).uppercase(),
        style = ChordType.caption.copy(fontWeight = FontWeight.Bold, letterSpacing = 0.06.em),
        color = Chord.colors.inkMuted,
        modifier = Modifier.fillMaxWidth().padding(start = ChordSpace.s4, end = ChordSpace.s3, top = ChordSpace.s4, bottom = ChordSpace.s1),
    )
}

/**
 * A group chat outside a space: a round people icon, the name and "N Members", and a badge for
 * the unread messages. It has the size of a direct chat row (56dp).
 */
@Composable
internal fun GroupRow(
    name: String,
    selected: Boolean,
    unread: Int,
    members: Int?,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    onLongClick: (() -> Unit)? = null,
) {
    val c = Chord.colors
    val isUnread = unread > 0
    val ink = if (selected || isUnread) c.ink else c.inkMuted
    val subtitle = members?.let { pluralStringResource(R.plurals.home_members, it, it) }
    val weight = if (selected || isUnread) FontWeight.SemiBold else FontWeight.Medium
    val desc = name + (subtitle?.let { ", $it" } ?: "") + if (unread > 0) ", $unread unread" else ""
    Box(
        modifier
            .fillMaxWidth()
            .padding(horizontal = ChordSpace.s2, vertical = 1.dp)
            .semantics(mergeDescendants = true) { contentDescription = desc; this.selected = selected },
    ) {
        if (isUnread && !selected) {
            Box(
                Modifier.align(Alignment.CenterStart).size(width = 4.dp, height = 8.dp)
                    .background(c.ink, RoundedCornerShape(topEnd = ChordRadius.sm, bottomEnd = ChordRadius.sm)),
            )
        }
        Row(
            Modifier
                .fillMaxWidth()
                .height(56.dp)
                .clip(RoundedCornerShape(ChordRadius.sm))
                .background(if (selected) c.selected else androidx.compose.ui.graphics.Color.Transparent)
                .combinedClickable(role = Role.Button, onLongClick = onLongClick, onClick = onClick)
                .padding(horizontal = ChordSpace.s2),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(ChordSpace.s2),
        ) {
            Box(Modifier.size(36.dp).clip(CircleShape).background(c.surface300), contentAlignment = Alignment.Center) {
                HomeGlyphIcon(HomeGlyph.People, c.inkMuted, size = 20.dp)
            }
            Column(Modifier.weight(1f)) {
                Text(name, style = ChordType.body.copy(fontWeight = weight, lineHeight = 20.sp), color = ink, maxLines = 1, overflow = TextOverflow.Ellipsis)
                if (subtitle != null) {
                    Text(subtitle, style = ChordType.caption, color = if (selected) c.ink else c.inkMuted, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
            if (unread > 0) CountBadge(unread)
        }
    }
}

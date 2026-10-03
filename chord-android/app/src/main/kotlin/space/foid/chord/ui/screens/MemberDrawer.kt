package space.foid.chord.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.collectAsState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.em
import androidx.lifecycle.viewmodel.compose.viewModel
import space.foid.chord.data.stableKey
import space.foid.chord.ui.avatar.JidAvatar
import space.foid.chord.ui.components.Presence
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordSpace
import space.foid.chord.ui.theme.ChordType
import space.foid.chord.viewmodel.ChordViewModels
import space.foid.chord.viewmodel.MemberListViewModel
import uniffi.chord_ffi.MemberItem

/** A group of the member list: a label and the people under it. */
data class MemberGroup(val label: String, val items: List<MemberItem>)

/**
 * Members grouped as on the desktop (groupMembers): online owners, online admins, other online
 * people, online visitors (they can read and cannot speak), then everyone offline. Empty groups are dropped.
 */
fun groupMembers(members: List<MemberItem>): List<MemberGroup> {
    val online = members.filter { it.online }
    fun aff(m: MemberItem) = m.affiliation.lowercase()
    fun staff(m: MemberItem) = aff(m) == "owner" || aff(m) == "admin"
    fun visitor(m: MemberItem) = m.role.equals("visitor", ignoreCase = true)
    return listOf(
        MemberGroup("Owners", online.filter { aff(it) == "owner" }),
        MemberGroup("Admins", online.filter { aff(it) == "admin" }),
        MemberGroup("Online", online.filter { !staff(it) && !visitor(it) }),
        MemberGroup("Visitors", online.filter { !staff(it) && visitor(it) }),
        MemberGroup("Offline", members.filter { !it.online }),
    ).filter { it.items.isNotEmpty() }
}

/** The presence of a member, from the core's `show` value. */
fun memberPresence(m: MemberItem): Presence = when {
    !m.online -> Presence.Offline
    m.show == "dnd" -> Presence.Dnd
    m.show == "away" || m.show == "xa" -> Presence.Away
    else -> Presence.Online
}

/**
 * The right drawer: the people of the room, or both people of a 1:1 chat, in groups.
 * [roomName] is the title. It has no ViewModel: [MemberDrawer] feeds it.
 */
@Composable
fun MemberDrawerContent(
    roomName: String,
    members: List<MemberItem>,
    loaded: Boolean,
    modifier: Modifier = Modifier,
) {
    val c = Chord.colors
    val groups = groupMembers(members)
    Column(modifier.fillMaxSize().background(c.surfaceSide)) {
        Column(Modifier.statusBarsPadding().fillMaxWidth().height(56.dp), verticalArrangement = Arrangement.Center) {
            Text(
                "Members", style = ChordType.title, color = c.ink,
                modifier = Modifier.padding(horizontal = ChordSpace.s4),
            )
        }
        LazyColumn(Modifier.weight(1f).fillMaxWidth().testTag("member_list").semantics { contentDescription = "Members of $roomName" }) {
            if (groups.isEmpty()) {
                item(key = "empty") {
                    Text(
                        if (loaded) "Nobody here" else "Loading",
                        style = ChordType.bodySmall, color = c.inkMuted, modifier = Modifier.padding(ChordSpace.s4),
                    )
                }
            }
            groups.forEach { g ->
                item(key = "header/${g.label}") {
                    Text(
                        "${g.label.uppercase()} — ${g.items.size}",
                        style = ChordType.caption.copy(fontWeight = FontWeight.Bold, letterSpacing = 0.06.em),
                        color = c.inkMuted,
                        modifier = Modifier.fillMaxWidth()
                            .padding(start = ChordSpace.s4, end = ChordSpace.s2, top = ChordSpace.s4, bottom = ChordSpace.s1),
                    )
                }
                items(g.items, key = { it.stableKey() }) { m -> MemberRow(m) }
            }
        }
    }
}

@Composable
private fun MemberRow(m: MemberItem) {
    val c = Chord.colors
    val presence = memberPresence(m)
    val name = m.name.ifBlank { m.jid ?: m.id }
    Row(
        Modifier
            .fillMaxWidth()
            .height(56.dp)
            .padding(horizontal = ChordSpace.s4)
            .alpha(if (m.online) 1f else 0.6f)
            .semantics(mergeDescendants = true) { contentDescription = "$name, ${presence.label}" }
            .testTag("member_${m.id}"),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(ChordSpace.s3),
    ) {
        JidAvatar(owner = m.jid ?: m.id, name = name, size = 36.dp, presence = presence, hash = m.avatar, cut = c.surfaceSide)
        Column(Modifier.weight(1f)) {
            Text(name, style = ChordType.name, color = c.ink, maxLines = 1, overflow = TextOverflow.Ellipsis)
            val status = m.status?.takeIf { it.isNotBlank() }
            if (status != null) {
                Text(status, style = ChordType.caption, color = c.inkMuted, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
    }
}

/** [MemberDrawerContent] fed by a [MemberListViewModel] with `key = room`. */
@Composable
fun MemberDrawer(room: String, roomName: String, modifier: Modifier = Modifier) {
    val vm: MemberListViewModel = viewModel(key = "members/$room", factory = ChordViewModels.memberList(room))
    val members by vm.members.collectAsState()
    val loaded by vm.loaded.collectAsState()
    MemberDrawerContent(roomName, members, loaded, modifier)
}

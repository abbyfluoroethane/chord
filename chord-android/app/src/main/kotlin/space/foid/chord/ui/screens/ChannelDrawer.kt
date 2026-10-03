package space.foid.chord.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.saveable.listSaver
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.snapshots.SnapshotStateList
import androidx.compose.runtime.toMutableStateList
import space.foid.chord.ui.components.ConnectionDot
import space.foid.chord.ui.components.ConnectionNotice
import space.foid.chord.ui.components.notice
import space.foid.chord.ui.components.rememberConnectionState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.em
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.lifecycle.viewmodel.compose.viewModel
import space.foid.chord.viewmodel.ChannelListViewModel
import space.foid.chord.viewmodel.ChordViewModels
import space.foid.chord.viewmodel.SpaceListViewModel
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.size
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import space.foid.chord.data.stableKey
import space.foid.chord.ui.avatar.JidAvatar
import space.foid.chord.ui.avatar.rememberAvatarBitmap
import space.foid.chord.ui.components.ChannelKind
import space.foid.chord.ui.components.ChannelListItem
import space.foid.chord.ui.components.RailIconKind
import space.foid.chord.ui.join.DrawerHeaderActions
import space.foid.chord.ui.components.SpaceRailIcon
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordSize
import space.foid.chord.ui.theme.ChordSpace
import space.foid.chord.ui.theme.ChordType
import uniffi.chord_ffi.ChannelItem
import uniffi.chord_ffi.ChannelScope
import uniffi.chord_ffi.SpaceItem
import space.foid.chord.ui.components.CountBadge

/** The account in the user panel. */
data class AccountUi(val jid: String, val name: String = jid.substringBefore('@'))

/**
 * A header and the rows under it in the channel list. [title] is null for a group with no header.
 * [key] is stable and names the section for the collapse state.
 */
data class ChannelSection(val title: String?, val items: List<ChannelItem>, val key: String = title.orEmpty())

/** The jid of a channel without a resource. For a private chat the item jid is `room/nick`. */
fun bareJid(jid: String): String = jid.substringBefore('/')

/**
 * The sections of the channel list, as on the desktop (ChannelSidebar.svelte and categories.ts).
 *
 * - Home is one flat list, "Messages". Group chats sit among the direct chats, as on Discord.
 * - A space has the header "Channels". The rooms with no category come first, under it. Each
 *   category then has a header of its own, in the order in which it first appears.
 * - The order inside a section is the order of [channels]. The core sorts it by last activity,
 *   newest first, and the desktop does not sort again.
 * - A room that is not joined shows like any other row.
 *
 * With no channels there is no section: the drawer shows an empty note.
 */
fun channelSections(scope: ChannelScope, channels: List<ChannelItem>): List<ChannelSection> {
    if (channels.isEmpty()) return emptyList()
    if (scope is ChannelScope.Home) return listOf(ChannelSection("Messages", channels, "messages"))
    val plain = channels.filter { it.category.isNullOrBlank() }
    val named = channels.filter { !it.category.isNullOrBlank() }.groupBy { it.category!!.trim() }
    return buildList {
        add(ChannelSection("Channels", plain, "channels"))
        named.forEach { (name, items) -> add(ChannelSection(name, items, "category/$name")) }
    }
}

private fun ChannelItem.rowKind(inSpace: Boolean): ChannelKind = when {
    kind is uniffi.chord_ffi.ChannelKind.Direct -> ChannelKind.Dm
    kind is uniffi.chord_ffi.ChannelKind.PrivateMessage -> ChannelKind.Dm
    inSpace -> ChannelKind.Room
    else -> ChannelKind.Group
}

private fun sameScope(a: ChannelScope, b: ChannelScope): Boolean = when {
    a is ChannelScope.Home && b is ChannelScope.Home -> true
    a is ChannelScope.Space && b is ChannelScope.Space -> a.service == b.service && a.node == b.node
    else -> false
}

/**
 * The left drawer: the space rail beside the channel list of the selected scope, with the user
 * panel under both. It has no ViewModel: [ChannelDrawer] feeds it.
 *
 * @param spaceUnread unread count per space, keyed by [SpaceItem.stableKey]. Shown as a badge on the rail.
 * @param homeUnread unread count of the direct chats, as a badge on Home.
 * @param connection the notice of the connection, for the dot on the account avatar. Null: connected.
 * @param onOpenSettings the \"Settings\" item of the account menu.
 * @param onSelect a tap on a channel row.
 * @param onLongPress a long press on a channel row.
 * @param inboxCount the number on the badge of the inbox button. [onInbox] and [onNew] are the header buttons.
 */
@Composable
fun ChannelDrawerContent(
    spaces: List<SpaceItem>,
    scope: ChannelScope,
    onScope: (ChannelScope) -> Unit,
    channels: List<ChannelItem>,
    loaded: Boolean,
    selectedJid: String?,
    onSelect: (ChannelItem) -> Unit,
    account: AccountUi?,
    onSignOut: () -> Unit,
    modifier: Modifier = Modifier,
    onOpenSettings: () -> Unit = {},
    connection: ConnectionNotice? = null,
    spaceUnread: Map<String, Int> = emptyMap(),
    homeUnread: Int = 0,
    inboxCount: Int = 0,
    onInbox: () -> Unit = {},
    onNew: () -> Unit = {},
    onLongPress: (ChannelItem) -> Unit = {},
) {
    val c = Chord.colors
    val isHome = scope is ChannelScope.Home
    val space = (scope as? ChannelScope.Space)?.let { s -> spaces.firstOrNull { it.service == s.service && it.node == s.node } }
    Column(modifier.fillMaxSize().background(c.surfaceSide)) {
        Row(Modifier.weight(1f).fillMaxWidth()) {
            // The rail.
            LazyColumn(
                Modifier.width(ChordSize.rail).fillMaxHeight().background(c.surfaceRail).statusBarsPadding(),
                contentPadding = androidx.compose.foundation.layout.PaddingValues(vertical = ChordSpace.s2),
            ) {
                item(key = "home") {
                    SpaceRailIcon(
                        name = "Home", selected = isHome, onClick = { onScope(ChannelScope.Home) },
                        kind = RailIconKind.Home, mentions = homeUnread,
                        modifier = Modifier.testTag("rail_home"),
                    )
                }
                items(spaces, key = { it.stableKey() }) { s ->
                    val n = spaceUnread[s.stableKey()] ?: 0
                    // The avatar owner of a space is `service/node`.
                    val image = rememberAvatarBitmap("${s.service}/${s.node}", s.avatar, 44.dp)
                    SpaceRailIcon(
                        name = s.name,
                        selected = scope is ChannelScope.Space && sameScope(scope, ChannelScope.Space(s.service, s.node)),
                        onClick = { onScope(ChannelScope.Space(s.service, s.node)) },
                        image = image,
                        seed = s.stableKey(), unread = n, mentions = n,
                        modifier = Modifier.testTag("rail_space_${s.node}"),
                    )
                }
            }
            // The channel list.
            Column(Modifier.weight(1f).fillMaxHeight()) {
                Row(
                    Modifier.fillMaxWidth().statusBarsPadding().height(ChordSize.bar + 8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(
                        if (isHome) "Home" else space?.name ?: "",
                        style = ChordType.title, color = c.ink, maxLines = 1, overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f).padding(start = ChordSpace.s4, end = ChordSpace.s2),
                    )
                    DrawerHeaderActions(inboxCount, onInbox, onNew, Modifier.padding(end = ChordSpace.s1))
                }
                val sections = channelSections(scope, channels)
                // The collapsed sections, per scope. They survive rotation and process death.
                val scopeId = if (scope is ChannelScope.Space) "${scope.service}|${scope.node}" else "home"
                val collapsed = rememberSaveable(
                    saver = listSaver<SnapshotStateList<String>, String>(save = { it.toList() }, restore = { it.toMutableStateList() }),
                ) { mutableStateListOf<String>() }
                LazyColumn(Modifier.weight(1f).fillMaxWidth().testTag("channel_list")) {
                    if (sections.isEmpty()) {
                        item(key = "empty") {
                            Text(
                                if (!loaded) "Loading" else if (isHome) "No chats yet" else "No channels",
                                style = ChordType.bodySmall, color = c.inkMuted,
                                modifier = Modifier.padding(ChordSpace.s4),
                            )
                        }
                    }
                    sections.forEach { sec ->
                        val id = "$scopeId/${sec.key}"
                        val isCollapsed = id in collapsed
                        if (sec.title != null) {
                            item(key = "header/${sec.key}") {
                                SectionHeader(
                                    sec.title, collapsed = isCollapsed,
                                    unread = sec.items.sumOf { it.unread.toInt() },
                                    onToggle = { if (isCollapsed) collapsed.remove(id) else collapsed.add(id) },
                                )
                            }
                        }
                        items(if (isCollapsed) emptyList() else sec.items, key = { it.stableKey() }) { ch ->
                            ChannelListItem(
                                name = ch.name.ifBlank { bareJid(ch.jid) },
                                selected = ch.jid == selectedJid,
                                onClick = { onSelect(ch) },
                                onLongClick = { onLongPress(ch) },
                                kind = ch.rowKind(!isHome),
                                jid = bareJid(ch.jid),
                                image = if (ch.kind is uniffi.chord_ffi.ChannelKind.PrivateMessage || ch.rowKind(!isHome) == ChannelKind.Room) null
                                    else rememberAvatarBitmap(bareJid(ch.jid), null, 36.dp),
                                unread = ch.unread.toInt(),
                                mentions = 0,
                                subtitle = if (isHome) groupSubtitle(ch) else null,
                                modifier = Modifier.testTag("channel_item_${bareJid(ch.jid)}"),
                            )
                        }
                    }
                }
            }
        }
        if (account != null) UserPanel(account, onSignOut, onOpenSettings, connection)
    }
}

private fun groupSubtitle(ch: ChannelItem): String? {
    val n = ch.members?.toInt() ?: return null
    if (ch.kind !is uniffi.chord_ffi.ChannelKind.Room) return null
    return "$n ${if (n == 1) "member" else "members"}"
}

/**
 * [ChannelDrawerContent] fed by the ViewModels. [scope] is hoisted: the container remembers it.
 * [onSelect] gets the tapped channel, [onLongPress] the one with a long press.
 */
@Composable
fun ChannelDrawer(
    scope: ChannelScope,
    onScope: (ChannelScope) -> Unit,
    selectedJid: String?,
    onSelect: (ChannelItem) -> Unit,
    account: AccountUi?,
    onSignOut: () -> Unit,
    modifier: Modifier = Modifier,
    onOpenSettings: () -> Unit = {},
    inboxCount: Int = 0,
    onInbox: () -> Unit = {},
    onNew: () -> Unit = {},
    onLongPress: (ChannelItem) -> Unit = {},
) {
    val spaceVm: SpaceListViewModel = viewModel(factory = ChordViewModels.spaceList)
    val channelVm: ChannelListViewModel = viewModel(factory = ChordViewModels.channelList(scope))
    LaunchedEffect(scope) { channelVm.setScope(scope) }
    val spaces by spaceVm.spaces.collectAsState()
    val channels by channelVm.channels.collectAsState()
    val loaded by channelVm.loaded.collectAsState()
    val shown by channelVm.currentScope.collectAsState()
    // Until the ViewModel follows the new scope, the list is empty: no stale rows.
    val current = sameScope(shown, scope)
    ChannelDrawerContent(
        spaces = spaces, scope = scope, onScope = onScope,
        channels = if (current) channels else emptyList(),
        loaded = loaded && current,
        selectedJid = selectedJid, onSelect = onSelect, account = account, onSignOut = onSignOut,
        homeUnread = if (scope is ChannelScope.Home && current) channels.sumOf { it.unread.toInt() } else 0,
        onOpenSettings = onOpenSettings,
        connection = rememberConnectionState().notice(),
        inboxCount = inboxCount, onInbox = onInbox, onNew = onNew, onLongPress = onLongPress,
        modifier = modifier,
    )
}

@Composable
private fun SectionHeader(title: String, collapsed: Boolean, unread: Int, onToggle: () -> Unit) {
    val c = Chord.colors
    Row(
        Modifier
            .fillMaxWidth()
            .clickable(role = Role.Button, onClickLabel = if (collapsed) "Expand" else "Collapse", onClick = onToggle)
            .padding(start = ChordSpace.s4, end = ChordSpace.s3, top = ChordSpace.s4, bottom = ChordSpace.s1)
            .semantics { contentDescription = title; stateDescription = if (collapsed) "Collapsed" else "Expanded" }
            .testTag("section_header_$title"),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(ChordSpace.s1),
    ) {
        Chevron(c.inkMuted, collapsed)
        Text(
            title.uppercase(),
            style = ChordType.caption.copy(fontWeight = androidx.compose.ui.text.font.FontWeight.Bold, letterSpacing = 0.06.em),
            color = c.inkMuted,
            modifier = Modifier.weight(1f),
        )
        if (collapsed && unread > 0) CountBadge(unread)
    }
}

@Composable
private fun Chevron(color: Color, collapsed: Boolean) {
    Canvas(Modifier.size(12.dp)) {
        val w = 1.6.dp.toPx()
        val m = size.width / 2
        val d = 3.dp.toPx()
        // Right when collapsed, down when open.
        if (collapsed) {
            drawLine(color, Offset(m - d / 2, m - d), Offset(m + d / 2, m), w, StrokeCap.Round)
            drawLine(color, Offset(m + d / 2, m), Offset(m - d / 2, m + d), w, StrokeCap.Round)
        } else {
            drawLine(color, Offset(m - d, m - d / 2), Offset(m, m + d / 2), w, StrokeCap.Round)
            drawLine(color, Offset(m, m + d / 2), Offset(m + d, m - d / 2), w, StrokeCap.Round)
        }
    }
}

/** The account at the bottom of the left drawer, with a menu for sign-out. */
@Composable
private fun UserPanel(account: AccountUi, onSignOut: () -> Unit, onOpenSettings: () -> Unit, connection: ConnectionNotice?) {
    val c = Chord.colors
    var menu by remember { mutableStateOf(false) }
    Box(Modifier.fillMaxWidth().background(c.surfaceRail).navigationBarsPadding()) {
        Row(
            Modifier
                .fillMaxWidth()
                .height(64.dp)
                .clickable(role = Role.Button, onClickLabel = "Account menu") { menu = true }
                .padding(horizontal = ChordSpace.s4)
                .testTag("user_panel"),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(ChordSpace.s3),
        ) {
            Box {
                JidAvatar(account.jid, name = account.name, size = 36.dp, cut = c.surfaceRail)
                ConnectionDot(connection, cut = c.surfaceRail, modifier = Modifier.align(Alignment.BottomEnd).offset(3.dp, 3.dp))
            }
            Column(Modifier.weight(1f)) {
                Text(account.name, style = ChordType.name, color = c.ink, maxLines = 1, overflow = TextOverflow.Ellipsis)
                Text(account.jid, style = ChordType.caption, color = c.inkMuted, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            GearGlyph(c.inkMuted)
        }
        DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
            DropdownMenuItem(
                text = { Text("Settings") },
                onClick = { menu = false; onOpenSettings() },
                modifier = Modifier.testTag("open_settings"),
            )
            DropdownMenuItem(
                text = { Text("Sign out") },
                onClick = { menu = false; onSignOut() },
                modifier = Modifier.testTag("sign_out"),
            )
        }
    }
}

@Composable
private fun GearGlyph(color: Color) {
    // Three dots, vertical: the account menu.
    Canvas(Modifier.size(20.dp)) {
        val r = 1.8.dp.toPx()
        for (i in -1..1) drawCircle(color, r, Offset(size.width / 2, size.height / 2 + i * 6.dp.toPx()))
    }
}

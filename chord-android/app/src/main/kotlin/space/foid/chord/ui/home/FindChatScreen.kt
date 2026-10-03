package space.foid.chord.ui.home

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.viewmodel.compose.viewModel
import space.foid.chord.R
import space.foid.chord.ui.avatar.rememberAvatarBitmap
import space.foid.chord.ui.components.Avatar
import space.foid.chord.ui.components.Presence
import space.foid.chord.ui.contacts.placeholderPresence
import space.foid.chord.ui.screens.bareJid
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordRadius
import space.foid.chord.ui.theme.ChordSpace
import space.foid.chord.ui.theme.ChordType
import space.foid.chord.viewmodel.ChannelListViewModel
import space.foid.chord.viewmodel.ChordViewModels
import space.foid.chord.viewmodel.ContactsViewModel
import space.foid.chord.viewmodel.JoinViewModel
import space.foid.chord.viewmodel.SpaceListViewModel
import uniffi.chord_ffi.ChannelItem
import uniffi.chord_ffi.ChannelScope
import uniffi.chord_ffi.Contact
import uniffi.chord_ffi.SpaceItem

/**
 * "Find or start a chat" (QuickSwitcher.svelte), full screen: one field and the hits under it.
 * Chats, rooms of the open space, contacts and spaces match what you type. An address that no
 * chat has gives "Message ..." and "Join the room ...", which reuse the join logic.
 *
 * @param scope the scope that the drawer shows. For a space, its rooms are searched too.
 * @param onOpenChannel a chat or room was picked. [onOpenPerson] starts a chat with a contact.
 * @param onOpenSpace a space was picked.
 */
@Composable
fun FindChatScreen(
    scope: ChannelScope,
    joinVm: JoinViewModel,
    contactsVm: ContactsViewModel,
    onOpenChannel: (ChannelItem) -> Unit,
    onOpenPerson: (jid: String, name: String) -> Unit,
    onOpenSpace: (SpaceItem) -> Unit,
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val homeVm: ChannelListViewModel = viewModel(key = "find_home", factory = ChordViewModels.channelList(ChannelScope.Home))
    val spaceVm: ChannelListViewModel = viewModel(key = "find_space", factory = ChordViewModels.channelList(scope))
    LaunchedEffect(scope) { spaceVm.setScope(scope) }
    val spaceListVm: SpaceListViewModel = viewModel(factory = ChordViewModels.spaceList)
    val home by homeVm.channels.collectAsState()
    val inSpace by spaceVm.channels.collectAsState()
    val shownScope by spaceVm.currentScope.collectAsState()
    val spaces by spaceListVm.spaces.collectAsState()
    val contacts by contactsVm.state.collectAsState()
    var query by rememberSaveable { mutableStateOf("") }
    val spaceRooms = if (scope is ChannelScope.Space && shownScope == scope) inSpace else emptyList()
    val hits = remember(query, home, spaceRooms, contacts.contacts, spaces) {
        findHits(query, home, spaceRooms, contacts.contacts, spaces)
    }
    FindChatContent(
        query = query,
        onQuery = { query = it },
        hits = hits,
        contacts = contacts.contacts,
        onPick = { hit ->
            when (hit) {
                is FindHit.Chat -> onOpenChannel(hit.item)
                is FindHit.Room -> onOpenChannel(hit.item)
                is FindHit.Person -> onOpenPerson(hit.jid, hit.title)
                is FindHit.Space -> onOpenSpace(hit.item)
                is FindHit.MessageAddress -> {
                    // The same code as the "Message" tab of the new conversation sheet.
                    joinVm.onPersonJid(hit.jid)
                    joinVm.messagePerson()
                    onBack()
                }
                is FindHit.JoinAddress -> { joinVm.openXmppUri("xmpp:${hit.jid}?join"); onBack() }
                is FindHit.Link -> { joinVm.openXmppUri(hit.uri); onBack() }
            }
        },
        onBack = onBack,
        modifier = modifier,
    )
}

/** The stateless [FindChatScreen]. */
@Composable
fun FindChatContent(
    query: String,
    onQuery: (String) -> Unit,
    hits: List<FindHit>,
    contacts: List<Contact>,
    onPick: (FindHit) -> Unit,
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val c = Chord.colors
    BackHandler(onBack = onBack)
    val focus = remember { FocusRequester() }
    LaunchedEffect(Unit) { runCatching { focus.requestFocus() } }
    val byJid = remember(contacts) { contacts.associateBy { it.jid.lowercase() } }
    Column(modifier.fillMaxSize().background(c.surface100).statusBarsPadding().imePadding().testTag("find_screen")) {
        Row(
            Modifier.fillMaxWidth().height(56.dp).padding(horizontal = ChordSpace.s1),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            val back = stringResource(R.string.find_back)
            Box(
                Modifier.size(48.dp).clip(CircleShape).clickable(role = Role.Button, onClick = onBack)
                    .semantics { contentDescription = back }.testTag("find_back"),
                contentAlignment = Alignment.Center,
            ) { HomeGlyphIcon(HomeGlyph.Back, c.ink, size = 22.dp) }
            Row(
                Modifier.weight(1f).height(44.dp).clip(RoundedCornerShape(ChordRadius.md)).background(c.surface300)
                    .padding(horizontal = ChordSpace.s3),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(ChordSpace.s2),
            ) {
                HomeGlyphIcon(HomeGlyph.Search, c.inkMuted, size = 18.dp)
                Box(Modifier.weight(1f), contentAlignment = Alignment.CenterStart) {
                    if (query.isEmpty()) {
                        Text(stringResource(R.string.find_placeholder), style = ChordType.body, color = c.inkMuted, maxLines = 1)
                    }
                    BasicTextField(
                        value = query,
                        onValueChange = onQuery,
                        singleLine = true,
                        textStyle = ChordType.body.copy(color = c.ink),
                        cursorBrush = SolidColor(c.brand),
                        keyboardOptions = KeyboardOptions(imeAction = ImeAction.Go),
                        keyboardActions = KeyboardActions(onGo = { hits.firstOrNull()?.let(onPick) }),
                        modifier = Modifier.fillMaxWidth().focusRequester(focus).testTag("find_field"),
                    )
                }
                if (query.isNotEmpty()) {
                    val clear = stringResource(R.string.find_clear)
                    Box(
                        Modifier.size(32.dp).clip(CircleShape).clickable(role = Role.Button) { onQuery("") }
                            .semantics { contentDescription = clear }.testTag("find_clear"),
                        contentAlignment = Alignment.Center,
                    ) { HomeGlyphIcon(HomeGlyph.Close, c.inkMuted, size = 16.dp) }
                }
            }
            Box(Modifier.size(ChordSpace.s2))
        }
        LazyColumn(Modifier.weight(1f).fillMaxWidth().testTag("find_list")) {
            if (hits.isEmpty()) {
                item(key = "empty") {
                    Text(
                        stringResource(R.string.find_empty), style = ChordType.body, color = c.inkMuted,
                        modifier = Modifier.padding(ChordSpace.s4),
                    )
                }
            }
            items(hits, key = { it.key }) { hit -> HitRow(hit, byJid, onClick = { onPick(hit) }) }
            item(key = "tip") {
                Text(
                    stringResource(R.string.find_tip), style = ChordType.caption, color = c.inkMuted,
                    modifier = Modifier.padding(ChordSpace.s4),
                )
            }
        }
    }
}

@Composable
private fun HitRow(hit: FindHit, contacts: Map<String, Contact>, onClick: () -> Unit) {
    val c = Chord.colors
    val title: String
    val hint: String
    var jid: String? = null
    var presence: Presence? = null
    when (hit) {
        is FindHit.Chat -> {
            title = hit.title
            hint = stringResource(if (hit.direct) R.string.find_message else R.string.find_group_chat)
            if (hit.direct) { jid = bareJid(hit.item.jid); presence = contacts[jid.lowercase()]?.placeholderPresence() } else jid = null
        }
        is FindHit.Room -> { title = hit.title; hint = stringResource(R.string.find_room) }
        is FindHit.Person -> {
            title = hit.title; hint = stringResource(R.string.find_message)
            jid = hit.jid; presence = contacts[hit.jid.lowercase()]?.placeholderPresence()
        }
        is FindHit.Space -> { title = hit.title; hint = stringResource(R.string.find_space) }
        is FindHit.MessageAddress -> { title = stringResource(R.string.find_message_address, hit.jid); hint = stringResource(R.string.find_message_address_hint) }
        is FindHit.JoinAddress -> { title = stringResource(R.string.find_join_address, hit.jid); hint = stringResource(R.string.find_join_address_hint) }
        is FindHit.Link -> { title = stringResource(R.string.find_open_link, hit.uri); hint = stringResource(R.string.find_open_link_hint) }
    }
    Row(
        Modifier
            .fillMaxWidth()
            .height(56.dp)
            .clickable(role = Role.Button, onClick = onClick)
            .padding(horizontal = ChordSpace.s4)
            .semantics(mergeDescendants = true) { contentDescription = "$title, $hint" }
            .testTag("find_hit_${hit.key}"),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(ChordSpace.s3),
    ) {
        Box(Modifier.size(36.dp), contentAlignment = Alignment.Center) {
            when {
                jid != null -> Avatar(
                    jid, name = title, size = 36.dp, presence = presence, cut = c.surface100,
                    image = rememberAvatarBitmap(jid, null, 36.dp),
                )
                hit is FindHit.Room -> Text("#", style = ChordType.title, color = c.inkMuted)
                hit is FindHit.Chat -> HomeGlyphIcon(HomeGlyph.People, c.inkMuted, size = 22.dp)
                hit is FindHit.Space -> Avatar(
                    "${hit.item.service}/${hit.item.node}", name = hit.title, size = 36.dp,
                    image = rememberAvatarBitmap("${hit.item.service}/${hit.item.node}", hit.item.avatar, 36.dp),
                )
                hit is FindHit.MessageAddress -> HomeGlyphIcon(HomeGlyph.Message, c.brandInk, size = 22.dp)
                else -> Text("#", style = ChordType.title, color = c.brandInk)
            }
        }
        Text(title, style = ChordType.name, color = c.ink, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
        Text(hint, style = ChordType.caption, color = c.inkMuted, maxLines = 1)
    }
}

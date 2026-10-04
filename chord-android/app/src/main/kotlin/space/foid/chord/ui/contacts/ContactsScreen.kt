package space.foid.chord.ui.contacts

import android.content.ClipData
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
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
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.LocalClipboard
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.platform.toClipEntry
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.em
import androidx.lifecycle.viewmodel.compose.viewModel
import kotlinx.coroutines.launch
import space.foid.chord.R
import space.foid.chord.ui.avatar.rememberAvatarBitmap
import space.foid.chord.ui.components.Avatar
import space.foid.chord.ui.components.CountBadge
import space.foid.chord.ui.home.HomeGlyph
import space.foid.chord.ui.home.HomeGlyphIcon
import space.foid.chord.ui.join.ConfirmDialog
import space.foid.chord.ui.join.JoinButton
import space.foid.chord.ui.join.JoinError
import space.foid.chord.ui.join.JoinField
import space.foid.chord.ui.join.JoinTextButton
import space.foid.chord.ui.sheets.ChordModalSheet
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordRadius
import space.foid.chord.ui.theme.ChordSpace
import space.foid.chord.ui.theme.ChordType
import space.foid.chord.viewmodel.AddResult
import space.foid.chord.viewmodel.ContactsState
import space.foid.chord.viewmodel.ContactsViewModel
import space.foid.chord.viewmodel.InboxViewModel
import space.foid.chord.viewmodel.RequestItem

/** What the buttons of the contacts page do. */
class ContactsCallbacks(
    val onBack: () -> Unit = {},
    val onTab: (ContactsTab) -> Unit = {},
    val onQuery: (String) -> Unit = {},
    val onMessage: (ContactEntry) -> Unit = {},
    val onMore: (ContactEntry) -> Unit = {},
    val onAccept: (ContactEntry, Boolean) -> Unit = { _, _ -> },
    val onIgnore: (ContactEntry) -> Unit = {},
    val onCancelRequest: (ContactEntry) -> Unit = {},
    val onUnblock: (ContactEntry) -> Unit = {},
    val onAdd: (String) -> Unit = {},
    val onAddEdited: () -> Unit = {},
)

/**
 * The contacts page (ContactsPage.svelte), full screen: the tabs Online, All, Pending, Blocked and
 * Add contact, a search field and the rows. A row has the actions of its tab. The "more" button
 * of a contact opens a sheet with rename, block, remove and copy.
 *
 * @param onMessage start or open the chat with this address. The page closes.
 */
@Composable
fun ContactsScreen(
    contactsVm: ContactsViewModel,
    inboxVm: InboxViewModel,
    onBack: () -> Unit,
    onMessage: (jid: String, name: String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val state by contactsVm.state.collectAsState()
    val inbox by inboxVm.state.collectAsState()
    var tab by rememberSaveable { mutableStateOf(ContactsTab.Online) }
    var query by rememberSaveable { mutableStateOf("") }
    var menuJid by rememberSaveable { mutableStateOf<String?>(null) }
    var renaming by rememberSaveable { mutableStateOf<String?>(null) }
    var removing by rememberSaveable { mutableStateOf<String?>(null) }
    var blocking by rememberSaveable { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()
    val clipboard = LocalClipboard.current

    val entries = remember(tab, state.contacts, state.blocked, inbox.requests, query) {
        contactEntries(tab, state.contacts, state.blocked, inbox.requests, query)
    }
    fun nameOf(jid: String) = state.find(jid)?.let(::contactName) ?: localPart(jid)
    val callbacks = ContactsCallbacks(
        onBack = onBack,
        onTab = { tab = it; query = ""; contactsVm.clearAddResult() },
        onQuery = { query = it },
        onMessage = { onMessage(it.jid, it.name) },
        onMore = { menuJid = it.jid },
        onAccept = { e, addBack -> inboxVm.accept(RequestItem(e.jid, addBack)) },
        onIgnore = { e -> inboxVm.deny(RequestItem(e.jid)) },
        onCancelRequest = { contactsVm.remove(it.jid) },
        onUnblock = { contactsVm.unblock(it.jid) },
        onAdd = { contactsVm.add(it) },
        onAddEdited = contactsVm::clearAddResult,
    )
    ContactsContent(
        state = state,
        tab = tab,
        query = query,
        entries = entries,
        pending = inbox.requests.size,
        callbacks = callbacks,
        modifier = modifier,
    )
    menuJid?.let { jid ->
        val contact = state.find(jid)
        if (contact == null) menuJid = null else ChordModalSheet(onDismiss = { menuJid = null }) { dismissThen ->
            ContactMenuContent(
                entry = ContactEntry(EntryKind.Contact, contact.jid, contactName(contact), contact),
                onMessage = { dismissThen { onMessage(contact.jid, contactName(contact)) } },
                onRename = { dismissThen { renaming = contact.jid } },
                onBlock = { dismissThen { blocking = contact.jid } },
                onRemove = { dismissThen { removing = contact.jid } },
                onCopy = {
                    dismissThen {
                        scope.launch { clipboard.setClipEntry(ClipData.newPlainText("address", contact.jid).toClipEntry()) }
                    }
                },
            )
        }
    }
    renaming?.let { jid ->
        RenameDialog(
            current = state.find(jid)?.name.orEmpty(),
            address = jid,
            onSave = { contactsVm.rename(jid, it); renaming = null },
            onCancel = { renaming = null },
        )
    }
    removing?.let { jid ->
        ConfirmDialog(
            title = stringResource(R.string.contacts_remove_title, nameOf(jid)),
            text = stringResource(R.string.contacts_remove_text),
            confirmLabel = stringResource(R.string.contacts_remove_confirm),
            cancelLabel = stringResource(R.string.contacts_cancel),
            onConfirm = { contactsVm.remove(jid); removing = null },
            onCancel = { removing = null },
        )
    }
    blocking?.let { jid ->
        ConfirmDialog(
            title = stringResource(R.string.contacts_block_title, nameOf(jid)),
            text = stringResource(R.string.contacts_block_text),
            confirmLabel = stringResource(R.string.contacts_block_confirm),
            cancelLabel = stringResource(R.string.contacts_cancel),
            onConfirm = { contactsVm.block(jid); blocking = null },
            onCancel = { blocking = null },
        )
    }
}

/** The stateless [ContactsScreen]. */
@Composable
fun ContactsContent(
    state: ContactsState,
    tab: ContactsTab,
    query: String,
    entries: List<ContactEntry>,
    pending: Int,
    callbacks: ContactsCallbacks,
    modifier: Modifier = Modifier,
) {
    val c = Chord.colors
    BackHandler(onBack = callbacks.onBack)
    Column(modifier.fillMaxSize().background(c.surface100).statusBarsPadding().imePadding().testTag("contacts_screen")) {
        Row(Modifier.fillMaxWidth().height(56.dp).padding(horizontal = ChordSpace.s1), verticalAlignment = Alignment.CenterVertically) {
            val back = stringResource(R.string.contacts_back)
            Box(
                Modifier.size(48.dp).clip(CircleShape).clickable(role = Role.Button, onClick = callbacks.onBack)
                    .semantics { contentDescription = back }.testTag("contacts_back"),
                contentAlignment = Alignment.Center,
            ) { HomeGlyphIcon(HomeGlyph.Back, c.ink, size = 22.dp) }
            Text(stringResource(R.string.contacts_title), style = ChordType.title, color = c.ink)
        }
        TabRow(tab, pending, callbacks.onTab)
        if (state.error != null) JoinError(state.error, "contacts_error", Modifier.padding(horizontal = ChordSpace.s4, vertical = ChordSpace.s2))
        if (tab == ContactsTab.Add) {
            AddPane(state, callbacks)
        } else {
            ListPane(state, tab, query, entries, callbacks)
        }
    }
}

@Composable
private fun TabRow(tab: ContactsTab, pending: Int, onTab: (ContactsTab) -> Unit) {
    val c = Chord.colors
    // A phone is too narrow for five tabs: the row scrolls.
    Row(
        Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = ChordSpace.s3, vertical = ChordSpace.s1),
        horizontalArrangement = Arrangement.spacedBy(ChordSpace.s1),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        for (t in ContactsTab.entries) {
            val add = t == ContactsTab.Add
            val selected = tab == t
            val label = stringResource(
                when (t) {
                    ContactsTab.Online -> R.string.contacts_tab_online
                    ContactsTab.All -> R.string.contacts_tab_all
                    ContactsTab.Pending -> R.string.contacts_tab_pending
                    ContactsTab.Blocked -> R.string.contacts_tab_blocked
                    ContactsTab.Add -> R.string.contacts_tab_add
                },
            )
            val fill = when {
                add -> c.brand
                selected -> c.selected
                else -> Color.Transparent
            }
            Row(
                Modifier
                    .height(40.dp)
                    .clip(RoundedCornerShape(ChordRadius.sm))
                    .background(fill)
                    .clickable(role = Role.Tab) { onTab(t) }
                    .padding(horizontal = ChordSpace.s3)
                    .semantics { this.selected = selected }
                    .testTag("contacts_tab_${t.name.lowercase()}"),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(ChordSpace.s2),
            ) {
                Text(
                    label,
                    style = ChordType.label,
                    color = if (add) c.onBrand else if (selected) c.ink else c.inkMuted,
                    maxLines = 1,
                )
                if (t == ContactsTab.Pending && pending > 0) CountBadge(pending)
            }
        }
    }
}

@Composable
private fun ListPane(state: ContactsState, tab: ContactsTab, query: String, entries: List<ContactEntry>, cb: ContactsCallbacks) {
    val c = Chord.colors
    Column(Modifier.fillMaxSize()) {
        SearchBox(query, cb.onQuery)
        val total = entries.size
        LazyColumn(Modifier.weight(1f).fillMaxWidth().navigationBarsPadding().testTag("contacts_list")) {
            if (entries.isNotEmpty()) {
                item(key = "group") {
                    Text(
                        stringResource(
                            when (tab) {
                                ContactsTab.Online -> R.string.contacts_group_online
                                ContactsTab.All -> R.string.contacts_group_all
                                ContactsTab.Pending -> R.string.contacts_group_pending
                                else -> R.string.contacts_group_blocked
                            },
                            total,
                        ).uppercase(),
                        style = ChordType.caption.copy(fontWeight = androidx.compose.ui.text.font.FontWeight.Bold, letterSpacing = 0.06.em),
                        color = c.inkMuted,
                        modifier = Modifier.padding(horizontal = ChordSpace.s4, vertical = ChordSpace.s2),
                    )
                }
            } else {
                item(key = "empty") {
                    val text = when {
                        !state.loaded -> R.string.contacts_loading
                        query.isNotBlank() -> R.string.contacts_empty_search
                        else -> when (tab) {
                            ContactsTab.Online -> R.string.contacts_empty_online
                            ContactsTab.All -> R.string.contacts_empty_all
                            ContactsTab.Pending -> R.string.contacts_empty_pending
                            else -> R.string.contacts_empty_blocked
                        }
                    }
                    Text(
                        stringResource(text), style = ChordType.body, color = c.inkMuted,
                        modifier = Modifier.padding(ChordSpace.s4).testTag("contacts_empty"),
                    )
                }
            }
            items(entries, key = { it.key }) { e -> EntryRow(e, busy = e.jid in state.busy, cb = cb) }
        }
    }
}

@Composable
private fun SearchBox(query: String, onQuery: (String) -> Unit) {
    val c = Chord.colors
    Row(
        Modifier
            .fillMaxWidth()
            .padding(horizontal = ChordSpace.s4, vertical = ChordSpace.s2)
            .height(44.dp)
            .clip(RoundedCornerShape(ChordRadius.md))
            .background(c.surface300)
            .padding(horizontal = ChordSpace.s3),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(ChordSpace.s2),
    ) {
        val label = stringResource(R.string.contacts_search_label)
        Box(Modifier.weight(1f), contentAlignment = Alignment.CenterStart) {
            if (query.isEmpty()) Text(stringResource(R.string.contacts_search), style = ChordType.body, color = c.inkMuted)
            BasicTextField(
                value = query,
                onValueChange = onQuery,
                singleLine = true,
                textStyle = ChordType.body.copy(color = c.ink),
                cursorBrush = SolidColor(c.brand),
                keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search),
                modifier = Modifier.fillMaxWidth().semantics { contentDescription = label }.testTag("contacts_search"),
            )
        }
        HomeGlyphIcon(HomeGlyph.Search, c.inkMuted, size = 18.dp)
    }
}

@Composable
private fun EntryRow(e: ContactEntry, busy: Boolean, cb: ContactsCallbacks) {
    val c = Chord.colors
    val contact = e.contact
    val presence = if (e.kind == EntryKind.Contact) contact?.contactPresence() else null
    val line = when (e.kind) {
        EntryKind.Contact -> listOfNotNull(
            contact?.statusText() ?: presence?.label,
            contact?.activity?.takeIf { it.isNotBlank() }?.let { stringResource(R.string.contacts_listening, it) },
        ).joinToString(" · ")
        EntryKind.Incoming -> stringResource(R.string.contacts_line_incoming)
        EntryKind.Outgoing -> stringResource(R.string.contacts_line_outgoing)
        EntryKind.Blocked -> stringResource(R.string.contacts_line_blocked)
    }
    // A request has only the address to tell who it is from. A contact has a name.
    val second = if (e.kind == EntryKind.Contact) line else e.jid
    Row(
        Modifier
            .fillMaxWidth()
            .height(64.dp)
            .alpha(if (busy) 0.5f else 1f)
            .then(if (e.kind == EntryKind.Contact) Modifier.clickable(role = Role.Button) { cb.onMessage(e) } else Modifier)
            .padding(start = ChordSpace.s4, end = ChordSpace.s2)
            .testTag("contacts_row_${e.jid}"),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(ChordSpace.s3),
    ) {
        Avatar(
            e.jid, name = e.name, size = 40.dp, presence = presence, cut = c.surface100,
            image = rememberAvatarBitmap(e.jid, null, 40.dp),
        )
        Column(Modifier.weight(1f)) {
            Text(e.name, style = ChordType.name, color = c.ink, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(second, style = ChordType.caption, color = c.inkMuted, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        when (e.kind) {
            EntryKind.Contact -> {
                RoundAction(HomeGlyph.Message, stringResource(R.string.contacts_message), c.ink, "contacts_message_${e.jid}") { cb.onMessage(e) }
                RoundAction(HomeGlyph.More, stringResource(R.string.contacts_more), c.ink, "contacts_more_${e.jid}") { cb.onMore(e) }
            }
            EntryKind.Incoming -> {
                RoundAction(HomeGlyph.Check, stringResource(R.string.contacts_accept), c.online, "contacts_accept_${e.jid}") { cb.onAccept(e, false) }
                RoundAction(HomeGlyph.AddUser, stringResource(R.string.contacts_accept_add_back), c.online, "contacts_addback_${e.jid}") { cb.onAccept(e, true) }
                RoundAction(HomeGlyph.Close, stringResource(R.string.contacts_ignore), c.danger, "contacts_ignore_${e.jid}") { cb.onIgnore(e) }
            }
            EntryKind.Outgoing ->
                RoundAction(HomeGlyph.Close, stringResource(R.string.contacts_cancel_request), c.danger, "contacts_cancel_${e.jid}") { cb.onCancelRequest(e) }
            EntryKind.Blocked ->
                RoundAction(HomeGlyph.Block, stringResource(R.string.contacts_unblock), c.ink, "contacts_unblock_${e.jid}") { cb.onUnblock(e) }
        }
    }
}

/** A round 40dp button with a glyph, the "RoundButton" of the desktop. */
@Composable
private fun RoundAction(glyph: HomeGlyph, label: String, tint: Color, tag: String, onClick: () -> Unit) {
    Box(
        Modifier
            .size(40.dp)
            .clip(CircleShape)
            .background(Chord.colors.surface300)
            .clickable(role = Role.Button, onClick = onClick)
            .semantics { contentDescription = label }
            .testTag(tag),
        contentAlignment = Alignment.Center,
    ) { HomeGlyphIcon(glyph, tint, size = 20.dp) }
}

@Composable
private fun AddPane(state: ContactsState, cb: ContactsCallbacks) {
    val c = Chord.colors
    var address by rememberSaveable { mutableStateOf("") }
    val result = state.addResult
    // The field clears after a request went out.
    if (result is AddResult.Sent && address.isNotEmpty()) address = ""
    Column(Modifier.fillMaxSize().padding(ChordSpace.s4).testTag("contacts_add")) {
        Text(stringResource(R.string.contacts_add_title), style = ChordType.title, color = c.ink)
        Text(
            stringResource(R.string.contacts_add_hint), style = ChordType.bodySmall, color = c.inkMuted,
            modifier = Modifier.padding(top = ChordSpace.s1, bottom = ChordSpace.s4),
        )
        JoinField(
            value = address,
            onChange = { address = it; cb.onAddEdited() },
            label = stringResource(R.string.contacts_add_label),
            tag = "contacts_add_field",
            placeholder = stringResource(R.string.contacts_add_placeholder),
            isError = result is AddResult.Failed,
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Email, imeAction = ImeAction.Send),
            keyboardActions = KeyboardActions(onSend = { if (address.isNotBlank()) cb.onAdd(address) }),
        )
        Spacer(Modifier.height(ChordSpace.s3))
        JoinButton(
            stringResource(R.string.contacts_add_send), { cb.onAdd(address) }, "contacts_add_send",
            modifier = Modifier.fillMaxWidth(), enabled = address.isNotBlank(), busy = state.adding,
        )
        Spacer(Modifier.height(ChordSpace.s3))
        when (result) {
            is AddResult.Sent -> Text(
                stringResource(R.string.contacts_add_sent, result.jid), style = ChordType.bodySmall, color = c.online,
                modifier = Modifier.testTag("contacts_add_result"),
            )
            is AddResult.Failed -> JoinError(result.text, "contacts_add_result")
            null -> Unit
        }
    }
}

/** The inside of the sheet of a contact row. It has no sheet window. */
@Composable
fun ContactMenuContent(
    entry: ContactEntry,
    onMessage: () -> Unit,
    onRename: () -> Unit,
    onBlock: () -> Unit,
    onRemove: () -> Unit,
    onCopy: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val c = Chord.colors
    Column(modifier.fillMaxWidth().navigationBarsPadding().padding(bottom = ChordSpace.s2).testTag("contact_menu")) {
        Row(
            Modifier.fillMaxWidth().padding(horizontal = ChordSpace.s4, vertical = ChordSpace.s1),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(ChordSpace.s3),
        ) {
            Avatar(
                entry.jid, name = entry.name, size = 40.dp, cut = c.surface200,
                image = rememberAvatarBitmap(entry.jid, null, 40.dp),
            )
            Column(Modifier.weight(1f)) {
                Text(entry.name, style = ChordType.name, color = c.ink, maxLines = 1, overflow = TextOverflow.Ellipsis)
                Text(entry.jid, style = ChordType.caption, color = c.inkMuted, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
        Spacer(Modifier.height(ChordSpace.s2))
        MenuRow(HomeGlyph.Message, stringResource(R.string.contacts_message), onMessage, "contact_menu_message")
        MenuRow(HomeGlyph.Edit, stringResource(R.string.contacts_menu_rename), onRename, "contact_menu_rename")
        MenuRow(HomeGlyph.Copy, stringResource(R.string.contacts_menu_copy), onCopy, "contact_menu_copy")
        MenuRow(HomeGlyph.Block, stringResource(R.string.contacts_menu_block), onBlock, "contact_menu_block", danger = true)
        MenuRow(HomeGlyph.Trash, stringResource(R.string.contacts_menu_remove), onRemove, "contact_menu_remove", danger = true)
    }
}

@Composable
private fun MenuRow(glyph: HomeGlyph, title: String, onClick: () -> Unit, tag: String, danger: Boolean = false) {
    val color = if (danger) Chord.colors.danger else Chord.colors.ink
    Row(
        Modifier.fillMaxWidth().height(52.dp).clickable(role = Role.Button, onClick = onClick)
            .padding(horizontal = ChordSpace.s4).testTag(tag),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        HomeGlyphIcon(glyph, color, size = 22.dp)
        Spacer(Modifier.width(ChordSpace.s4))
        Text(title, style = ChordType.body, color = color, maxLines = 1)
    }
}

/** The card with the name field of "Rename contact". An empty name removes the roster name. */
@Composable
internal fun RenameCard(
    initial: String,
    address: String,
    onSave: (String) -> Unit,
    onCancel: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val c = Chord.colors
    var name by rememberSaveable { mutableStateOf(initial) }
    Column(
        modifier.fillMaxWidth().clip(RoundedCornerShape(ChordRadius.lg)).background(c.surface200).padding(ChordSpace.s6),
    ) {
        Text(stringResource(R.string.contacts_rename_title), style = ChordType.title, color = c.ink)
        Text(
            stringResource(R.string.contacts_rename_hint), style = ChordType.bodySmall, color = c.inkMuted,
            modifier = Modifier.padding(top = ChordSpace.s1, bottom = ChordSpace.s4),
        )
        JoinField(
            value = name, onChange = { name = it },
            label = stringResource(R.string.contacts_rename_label), tag = "contacts_rename_field",
            placeholder = address.substringBefore('@'),
            keyboardOptions = KeyboardOptions(imeAction = ImeAction.Done),
            keyboardActions = KeyboardActions(onDone = { onSave(name) }),
        )
        Spacer(Modifier.height(ChordSpace.s4))
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End, verticalAlignment = Alignment.CenterVertically) {
            JoinTextButton(stringResource(R.string.contacts_cancel), onCancel, "contacts_rename_cancel")
            Spacer(Modifier.width(ChordSpace.s2))
            JoinButton(stringResource(R.string.contacts_rename_save), { onSave(name) }, "contacts_rename_save", compact = true)
        }
    }
}

@Composable
private fun RenameDialog(current: String, address: String, onSave: (String) -> Unit, onCancel: () -> Unit) {
    androidx.compose.ui.window.Dialog(onDismissRequest = onCancel) { RenameCard(current, address, onSave, onCancel) }
}

package space.foid.chord.ui.profile

import android.content.ClipData
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.LocalClipboard
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.platform.toClipEntry
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.viewmodel.compose.viewModel
import kotlinx.coroutines.launch
import space.foid.chord.R
import space.foid.chord.ui.sheets.ChordModalSheet
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordRadius
import space.foid.chord.ui.theme.ChordSpace
import space.foid.chord.ui.theme.ChordType
import space.foid.chord.viewmodel.ChordViewModels
import space.foid.chord.viewmodel.MemberListViewModel
import space.foid.chord.viewmodel.ProfileState
import space.foid.chord.viewmodel.ProfileSubject
import space.foid.chord.viewmodel.ProfileViewModel
import space.foid.chord.viewmodel.SpaceListViewModel
import uniffi.chord_ffi.MemberItem
import uniffi.chord_ffi.SpaceItem

/** The member of [members] that [address] names: by id (`room/nick`), by real address, or by bare address. */
fun findMember(members: List<MemberItem>, address: String): MemberItem? =
    members.firstOrNull { it.id == address }
        ?: members.firstOrNull { it.jid == address }
        ?: members.firstOrNull { it.jid != null && it.jid == address.substringBefore('/') }

/** The people of [room] (or both people of a 1:1 chat), for a profile that needs the member entry. */
@Composable
private fun rememberRoomMembers(room: String?): List<MemberItem> {
    if (room == null) return emptyList()
    val vm: MemberListViewModel = viewModel(key = "members/$room", factory = ChordViewModels.memberList(room))
    val members by vm.members.collectAsState()
    return members
}

/**
 * The profile of [address] in a bottom sheet. Opened from a member row, a message avatar or name,
 * or the profile rail. [room] is the open room, if any: it gives roles and presence.
 *
 * @param onMessage start or open the chat with this person: gets the address and the name.
 */
@Composable
fun ProfileSheet(
    address: String,
    name: String,
    room: String?,
    onDismiss: () -> Unit,
    onMessage: (address: String, name: String) -> Unit,
    onOpenSettings: () -> Unit,
) {
    val member = findMember(rememberRoomMembers(room), address)
    val subject = ProfileSubject(address, name, member)
    val vm: ProfileViewModel = viewModel(
        key = "profile/$address/${member?.id}", factory = ProfileViewModel.factory(subject),
    )
    ChordModalSheet(onDismiss = onDismiss) { dismissThen ->
        Column(Modifier.navigationBarsPadding().verticalScroll(rememberScrollState())) {
            ProfileWithMenu(
                vm = vm,
                variant = ProfileVariant.Sheet,
                cut = Chord.colors.surface200,
                onMessage = { a, n -> dismissThen { onMessage(a, n) } },
                onEditProfile = { dismissThen(onOpenSettings) },
                onViewFull = null,
            )
        }
    }
}

/** The right drawer of a 1:1 chat: the profile of the peer, where a room shows its members. */
@Composable
fun DmProfilePane(
    address: String,
    name: String,
    onOpenSettings: () -> Unit,
    onViewFull: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val member = findMember(rememberRoomMembers(address), address)
    val vm: ProfileViewModel = viewModel(
        key = "profilepane/$address/${member?.id}", factory = ProfileViewModel.factory(ProfileSubject(address, name, member)),
    )
    Column(modifier.fillMaxSize().background(Chord.colors.surfaceSide)) {
        Column(Modifier.statusBarsPadding().fillMaxWidth().height(56.dp), verticalArrangement = Arrangement.Center) {
            Text(
                stringResource(R.string.people_profile), style = ChordType.title, color = Chord.colors.ink,
                modifier = Modifier.padding(horizontal = ChordSpace.s4),
            )
        }
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).testTag("profile_rail")) {
            ProfileWithMenu(
                vm = vm,
                variant = ProfileVariant.Rail,
                cut = Chord.colors.surfaceSide,
                onMessage = { _, _ -> },
                onEditProfile = onOpenSettings,
                onViewFull = onViewFull,
            )
        }
    }
}

/** The profile of a ViewModel with its actions: the menu, the space picker and the rename dialog. */
@Composable
private fun ProfileWithMenu(
    vm: ProfileViewModel,
    variant: ProfileVariant,
    cut: Color,
    onMessage: (String, String) -> Unit,
    onEditProfile: () -> Unit,
    onViewFull: (() -> Unit)?,
) {
    val state by vm.state.collectAsState()
    val notice by vm.notice.collectAsState()
    val spaceVm: SpaceListViewModel = viewModel(factory = ChordViewModels.spaceList)
    val spaces by spaceVm.spaces.collectAsState()
    LaunchedEffect(spaces) { vm.setSpaces(spaces) }

    val clipboard = LocalClipboard.current
    val scope = rememberCoroutineScope()
    var menuOpen by remember { mutableStateOf(false) }
    var renaming by remember { mutableStateOf(false) }
    val copy: () -> Unit = {
        scope.launch { clipboard.setClipEntry(ClipData.newPlainText("address", state.address).toClipEntry()) }
    }

    ProfileContent(
        state = state,
        variant = variant,
        callbacks = ProfileCallbacks(
            onMessage = { onMessage(state.address, state.name) },
            onEditProfile = onEditProfile,
            onAddContact = vm::addContact,
            onUnblock = vm::unblock,
            onMore = { menuOpen = true },
            onCopyAddress = copy,
            onViewFull = onViewFull,
        ),
        cut = cut,
        notice = notice,
    )
    if (menuOpen) {
        PersonMenuSheet(
            state = state,
            showMessage = variant == ProfileVariant.Sheet,
            onDismiss = { menuOpen = false },
            onMessage = { onMessage(state.address, state.name) },
            onRename = { renaming = true },
            onToggleContact = { if (state.isContact) vm.removeContact() else vm.addContact() },
            onToggleBlock = { if (state.isBlocked) vm.unblock() else vm.block() },
            onInvite = vm::invite,
            onCopy = copy,
        )
    }
    if (renaming) {
        RenameDialog(initial = state.name, onDismiss = { renaming = false }, onSave = { vm.rename(it); renaming = false })
    }
}

/** The items of the person menu (desktop PersonMenu). A space picker replaces them for "Invite to space". */
@Composable
fun PersonMenuSheet(
    state: ProfileState,
    showMessage: Boolean,
    onDismiss: () -> Unit,
    onMessage: () -> Unit,
    onRename: () -> Unit,
    onToggleContact: () -> Unit,
    onToggleBlock: () -> Unit,
    onInvite: (SpaceItem) -> Unit,
    onCopy: () -> Unit,
) {
    ChordModalSheet(onDismiss = onDismiss) { dismissThen ->
        var picking by remember { mutableStateOf(false) }
        Column(Modifier.navigationBarsPadding().padding(bottom = ChordSpace.s2).testTag("person_menu")) {
            if (picking) {
                MenuRow(stringResource(R.string.people_back), onClick = { picking = false })
                state.invitable.forEach { sp -> MenuRow(sp.name, onClick = { dismissThen { onInvite(sp) } }) }
            } else {
                PersonMenuItems(
                    state, showMessage,
                    onMessage = { dismissThen(onMessage) },
                    onInvite = { picking = true },
                    onRename = { dismissThen(onRename) },
                    onToggleContact = { dismissThen(onToggleContact) },
                    onToggleBlock = { dismissThen(onToggleBlock) },
                    onCopy = { dismissThen(onCopy) },
                )
            }
        }
    }
}

/** Which rows the menu shows for [state]. Pure, for tests. */
data class PersonMenuPlan(
    val message: Boolean,
    val invite: Boolean,
    val rename: Boolean,
    val contact: Boolean,
    val block: Boolean,
    val copy: Boolean,
)

fun personMenuPlan(state: ProfileState, showMessage: Boolean) = PersonMenuPlan(
    message = showMessage && !state.isMe,
    invite = !state.isMe && !state.hidden && state.invitable.isNotEmpty(),
    rename = !state.isMe && state.isContact,
    contact = !state.isMe && !state.hidden,
    block = !state.isMe && !state.hidden,
    copy = !state.hidden,
)

@Composable
fun PersonMenuItems(
    state: ProfileState,
    showMessage: Boolean,
    onMessage: () -> Unit,
    onInvite: () -> Unit,
    onRename: () -> Unit,
    onToggleContact: () -> Unit,
    onToggleBlock: () -> Unit,
    onCopy: () -> Unit,
) {
    val plan = personMenuPlan(state, showMessage)
    if (plan.message) MenuRow(stringResource(R.string.people_message), onMessage)
    if (plan.invite) MenuRow(stringResource(R.string.people_invite_to_space) + " ›", onInvite)
    if (plan.rename) MenuRow(stringResource(R.string.people_rename_contact), onRename)
    if (plan.contact) {
        MenuRow(
            stringResource(if (state.isContact) R.string.people_remove_contact else R.string.people_add_contact),
            onToggleContact, enabled = state.isContact || !state.isBlocked,
        )
    }
    if (plan.block) {
        MenuRow(stringResource(if (state.isBlocked) R.string.people_unblock else R.string.people_block), onToggleBlock, danger = true)
    }
    if (plan.copy) MenuRow(stringResource(R.string.people_copy_address), onCopy)
}

@Composable
private fun MenuRow(text: String, onClick: () -> Unit, danger: Boolean = false, enabled: Boolean = true) {
    val c = Chord.colors
    Box(
        Modifier.fillMaxWidth().height(52.dp).clickable(enabled = enabled, role = Role.Button, onClick = onClick)
            .padding(horizontal = ChordSpace.s4),
        contentAlignment = Alignment.CenterStart,
    ) {
        Text(
            text, style = ChordType.body, color = (if (danger) c.danger else c.ink).copy(alpha = if (enabled) 1f else 0.4f),
            maxLines = 1, overflow = TextOverflow.Ellipsis,
        )
    }
}

@Composable
private fun RenameDialog(initial: String, onDismiss: () -> Unit, onSave: (String) -> Unit) {
    val c = Chord.colors
    var text by remember { mutableStateOf(initial) }
    AlertDialog(
        onDismissRequest = onDismiss,
        containerColor = c.surface200,
        title = { Text(stringResource(R.string.people_rename_title), style = ChordType.title, color = c.ink) },
        text = {
            BasicTextField(
                value = text, onValueChange = { text = it }, singleLine = true,
                textStyle = ChordType.body.copy(color = c.ink), cursorBrush = SolidColor(c.brand),
                modifier = Modifier.fillMaxWidth().background(c.surface300, androidx.compose.foundation.shape.RoundedCornerShape(ChordRadius.md))
                    .padding(ChordSpace.s3).testTag("rename_field"),
            )
        },
        confirmButton = { TextButton(onClick = { onSave(text) }) { Text(stringResource(R.string.people_save), color = c.brandInk) } },
        dismissButton = { TextButton(onClick = onDismiss) { Text(stringResource(R.string.people_cancel), color = c.inkMuted) } },
    )
}

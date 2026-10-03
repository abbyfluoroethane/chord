package space.foid.chord.ui.spaces

import android.content.ClipData
import android.content.ContentResolver
import android.graphics.BitmapFactory
import android.net.Uri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalClipboard
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.toClipEntry
import androidx.compose.ui.res.stringResource
import androidx.lifecycle.viewmodel.compose.viewModel
import kotlinx.coroutines.launch
import space.foid.chord.R
import space.foid.chord.ui.join.ConfirmDialog
import space.foid.chord.ui.sheets.ChordModalSheet
import space.foid.chord.viewmodel.JoinEvent
import space.foid.chord.viewmodel.JoinViewModel
import space.foid.chord.viewmodel.SpaceEvent
import space.foid.chord.viewmodel.SpaceViewModel
import space.foid.chord.viewmodel.SpacesState
import uniffi.chord_ffi.SpaceAccess
import uniffi.chord_ffi.SpaceItem

/** Which menu or dialog of the spaces is open. All but [Home] and [Add] are about one space. */
sealed interface SpaceRequest {
    /** The menu of the space header. */
    data class Menu(val target: SpaceTarget) : SpaceRequest

    /** The menu of a long press on a space of the rail. */
    data class Rail(val target: SpaceTarget) : SpaceRequest
    data class Invite(val target: SpaceTarget) : SpaceRequest
    data class Settings(val target: SpaceTarget) : SpaceRequest
    data class CreateChannel(val target: SpaceTarget) : SpaceRequest
    data class Notifications(val target: SpaceTarget) : SpaceRequest
    data class Nickname(val target: SpaceTarget) : SpaceRequest
    data class Leave(val target: SpaceTarget) : SpaceRequest

    /** The menu of a long press on Home. */
    data object Home : SpaceRequest

    /** "Add a space". */
    data object Add : SpaceRequest
}

/** The target of a request, or null for [SpaceRequest.Home] and [SpaceRequest.Add]. */
val SpaceRequest.target: SpaceTarget?
    get() = when (this) {
        is SpaceRequest.Menu -> target
        is SpaceRequest.Rail -> target
        is SpaceRequest.Invite -> target
        is SpaceRequest.Settings -> target
        is SpaceRequest.CreateChannel -> target
        is SpaceRequest.Notifications -> target
        is SpaceRequest.Nickname -> target
        is SpaceRequest.Leave -> target
        SpaceRequest.Home, SpaceRequest.Add -> null
    }

/** The open request. Read it from the callbacks of a sheet: it is live state, never stale. */
class SpaceHostState {
    var request: SpaceRequest? by mutableStateOf(null)
        private set

    fun show(r: SpaceRequest) {
        request = r
    }

    /** Close [r], unless another request has replaced it meanwhile. */
    fun dismiss(r: SpaceRequest) {
        if (request == r) request = null
    }
}

@Composable
fun rememberSpaceHostState(): SpaceHostState = remember { SpaceHostState() }

fun SpaceItem.target() = SpaceTarget(service, node, name)

/**
 * Draws the menu or dialog of [state] and runs it on [SpaceViewModel]. It sits in the channel
 * drawer: the rail and the space header only call [SpaceHostState.show].
 *
 * @param spaces the spaces of the rail, for "Mark every space as read" and the avatar of a tile
 * @param nick the default nickname: the name of the account
 */
@Composable
fun SpaceHost(
    state: SpaceHostState,
    spaces: List<SpaceItem>,
    nick: String,
    modifier: androidx.compose.ui.Modifier = androidx.compose.ui.Modifier,
) {
    val vm: SpaceViewModel = viewModel(factory = SpaceViewModel.factory)
    val joinVm: JoinViewModel = viewModel(factory = JoinViewModel.factory)
    val request = state.request
    val data by vm.state.collectAsState()

    // The ViewModel reads what the open request needs. It starts over for another space.
    LaunchedEffect(request?.target) {
        val t = request?.target
        if (t != null) vm.open(t) else vm.close()
    }
    // A new or joined space ends "Add a space".
    LaunchedEffect(vm) {
        vm.events.collect { if (it is SpaceEvent.OpenSpace) state.dismiss(SpaceRequest.Add) }
    }
    LaunchedEffect(joinVm) {
        joinVm.events.collect { if (it is JoinEvent.OpenSpace) state.dismiss(SpaceRequest.Add) }
    }

    when (request) {
        null -> Unit
        is SpaceRequest.Menu -> ChordModalSheet({ state.dismiss(request) }) { dismissThen ->
            SpaceMenuContent(
                request.target, data.owner,
                onInvite = { dismissThen { state.show(SpaceRequest.Invite(request.target)) } },
                onSettings = { dismissThen { state.show(SpaceRequest.Settings(request.target)) } },
                onCreateChannel = { dismissThen { state.show(SpaceRequest.CreateChannel(request.target)) } },
                onNotifications = { dismissThen { state.show(SpaceRequest.Notifications(request.target)) } },
                onNickname = { dismissThen { state.show(SpaceRequest.Nickname(request.target)) } },
                onLeave = { dismissThen { state.show(SpaceRequest.Leave(request.target)) } },
            )
        }
        is SpaceRequest.Rail -> ChordModalSheet({ state.dismiss(request) }) { dismissThen ->
            RailMenuContent(
                request.target, data.owner, quiet = false, level = data.level,
                onMarkRead = { dismissThen { vm.markSpaceRead(request.target) } },
                onLevel = { vm.setLevel(it) {} },
                onInvite = { dismissThen { state.show(SpaceRequest.Invite(request.target)) } },
                onSettings = { dismissThen { state.show(SpaceRequest.Settings(request.target)) } },
                onLeave = { dismissThen { state.show(SpaceRequest.Leave(request.target)) } },
                error = data.error,
            )
        }
        SpaceRequest.Home -> ChordModalSheet({ state.dismiss(request) }) { dismissThen ->
            HomeMenuContent(
                quietHome = false, quietAll = false,
                onMarkHome = { dismissThen { vm.markAllRead(emptyList(), includeHome = true) } },
                onMarkAll = { dismissThen { vm.markAllRead(spaces, includeHome = false) } },
            )
        }
        is SpaceRequest.Notifications -> ChordModalSheet({ state.dismiss(request) }) { dismissThen ->
            SpaceNotificationsContent(
                request.target, data.level, data.busy, data.error,
                onSave = { level -> vm.setLevel(level) { dismissThen {} } },
            )
        }
        is SpaceRequest.CreateChannel -> ChordModalSheet({ state.dismiss(request) }) { dismissThen ->
            TextPromptContent(
                title = stringResource(R.string.spaces_create_channel_title),
                label = stringResource(R.string.spaces_channel_name),
                initial = "",
                confirmLabel = stringResource(R.string.spaces_create_channel_button),
                onConfirm = { name -> vm.createChannel(name) { dismissThen {} } },
                hint = stringResource(R.string.spaces_create_channel_hint, request.target.name),
                placeholder = stringResource(R.string.spaces_channel_placeholder),
                busy = data.busy, error = data.error, tag = "create_channel",
            )
        }
        is SpaceRequest.Nickname -> ChordModalSheet({ state.dismiss(request) }) { dismissThen ->
            TextPromptContent(
                title = stringResource(R.string.spaces_nickname),
                label = stringResource(R.string.spaces_nickname_label),
                initial = nick,
                confirmLabel = stringResource(R.string.spaces_save),
                onConfirm = { text -> vm.changeNick(text) { dismissThen {} } },
                hint = stringResource(R.string.spaces_nickname_hint, request.target.name),
                busy = data.busy, error = data.error, tag = "nickname",
            )
        }
        is SpaceRequest.Leave -> ConfirmDialog(
            title = stringResource(R.string.spaces_leave_title),
            text = stringResource(R.string.spaces_leave_text, request.target.name) + (data.error?.let { "\n\n$it" } ?: ""),
            confirmLabel = stringResource(R.string.spaces_leave),
            cancelLabel = stringResource(R.string.actions_cancel),
            onConfirm = { vm.leaveSpace { state.dismiss(request) } },
            onCancel = { state.dismiss(request) },
        )
        is SpaceRequest.Invite -> InvitePage(state, request, vm)
        is SpaceRequest.Settings -> SettingsPage(state, request, vm)
        SpaceRequest.Add -> AddSpaceHost(state, vm, joinVm, spaces.mapTo(HashSet()) { "${it.service}|${it.node}" })
    }
}

@Composable
private fun InvitePage(host: SpaceHostState, request: SpaceRequest.Invite, vm: SpaceViewModel) {
    val data by vm.state.collectAsState()
    val clipboard = LocalClipboard.current
    val scope = rememberCoroutineScope()
    var picked by remember { mutableStateOf(emptySet<String>()) }
    var query by rememberSaveable { mutableStateOf("") }
    var copied by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) { vm.loadContacts() }
    val link = spaceInviteLink(request.target.service, request.target.node)
    SpacePage(
        title = stringResource(R.string.spaces_invite),
        onClose = { host.dismiss(request) },
        bottom = { InviteButton(picked.size, data.busy) { vm.invite(picked.toList()) { host.dismiss(request) } } },
    ) {
        InviteContent(
            request.target, data.contacts, picked, query, copied, data.error,
            onQuery = { query = it },
            onToggle = { jid -> picked = if (jid in picked) picked - jid else picked + jid },
            onCopy = {
                scope.launch {
                    clipboard.setClipEntry(ClipData.newPlainText("link", link).toClipEntry())
                    copied = true
                }
            },
        )
    }
}

@Composable
private fun SettingsPage(host: SpaceHostState, request: SpaceRequest.Settings, vm: SpaceViewModel) {
    val data by vm.state.collectAsState()
    val context = LocalContext.current
    var name by remember { mutableStateOf(request.target.name) }
    var description by remember(data.settingsLoaded) { mutableStateOf(data.description) }
    var confirmDelete by remember { mutableStateOf(false) }
    var banner by remember { mutableStateOf(false) }
    var note by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(Unit) { vm.loadSettings() }
    val notImage = stringResource(R.string.spaces_image_not_image)
    val bigAvatar = stringResource(R.string.spaces_image_too_big, 1)
    val bigBanner = stringResource(R.string.spaces_image_too_big, 4)
    val picker = rememberLauncherForActivityResult(ActivityResultContracts.GetContent()) { uri ->
        if (uri != null) {
            when (val r = readSpaceImage(context.contentResolver, uri, banner)) {
                is SpaceImage.Ok -> vm.setImage(banner, r.mime, r.bytes, r.width, r.height)
                SpaceImage.NotImage -> note = notImage
                is SpaceImage.TooBig -> note = if (banner) bigBanner else bigAvatar
            }
        }
    }
    val change = settingsChange(request.target.name, data.description, name, description)
    SpacePage(
        title = stringResource(R.string.spaces_settings),
        onClose = { host.dismiss(request) },
        bottom = if (data.owner == true) {
            { SettingsSaveButton(!change.isEmpty, data.busy) { vm.saveSettings(name, description) { host.dismiss(request) } } }
        } else null,
    ) {
        SettingsContent(
            if (note != null) data.copy(error = note) else data, name, description,
            SettingsCallbacks(
                onName = { name = it }, onDescription = { description = it },
                onPickImage = { b -> banner = b; note = null; picker.launch("image/*") },
                onMember = vm::removeMember, onRequest = vm::answerRequest,
                onRemoveChannel = vm::removeChannel, onDelete = { confirmDelete = true },
            ),
        )
    }
    if (confirmDelete) {
        ConfirmDialog(
            title = stringResource(R.string.spaces_delete_title),
            text = stringResource(R.string.spaces_delete_text, request.target.name),
            confirmLabel = stringResource(R.string.spaces_settings_delete),
            cancelLabel = stringResource(R.string.actions_cancel),
            onConfirm = { confirmDelete = false; vm.deleteSpace { host.dismiss(request) } },
            onCancel = { confirmDelete = false },
        )
    }
}

@Composable
private fun AddSpaceHost(host: SpaceHostState, vm: SpaceViewModel, joinVm: JoinViewModel, joined: Set<String>) {
    val data by vm.state.collectAsState()
    val join by joinVm.state.collectAsState()
    var tab by rememberSaveable { mutableStateOf(AddSpaceTab.Create) }
    var name by rememberSaveable { mutableStateOf("") }
    var description by rememberSaveable { mutableStateOf("") }
    var access by rememberSaveable { mutableStateOf(SpaceAccess.OPEN) }
    var query by rememberSaveable { mutableStateOf("") }
    LaunchedEffect(tab) { if (tab == AddSpaceTab.Join) joinVm.loadSpaces() }
    ChordModalSheet({ host.dismiss(SpaceRequest.Add) }) { dismissThen ->
        AddSpaceContent(
            AddSpaceUi(tab, name, description, access, query, data.busy, data.error),
            join.spaces, joined,
            AddSpaceCallbacks(
                onTab = { tab = it; vm.clearError() },
                onName = { name = it }, onDescription = { description = it }, onAccess = { access = it },
                onCreate = { vm.createSpace(name, description, access) { dismissThen {} } },
                onQuery = { query = it },
                onReload = { joinVm.loadSpaces(force = true) },
                onJoin = joinVm::joinSpace,
            ),
        )
    }
}

/** What reading a picked image gave. */
sealed interface SpaceImage {
    data class Ok(val mime: String, val bytes: ByteArray, val width: Int, val height: Int) : SpaceImage
    data object NotImage : SpaceImage
    data class TooBig(val limitMb: Int) : SpaceImage
}

/** The size limits of the desktop: 1 MB for an avatar, 4 MB for a banner. */
fun imageLimitBytes(banner: Boolean): Int = (if (banner) 4 else 1) * 1024 * 1024

/** Read an image from [uri]. It checks the type and the size, and reads the pixel size. */
fun readSpaceImage(resolver: ContentResolver, uri: Uri, banner: Boolean): SpaceImage {
    val mime = resolver.getType(uri) ?: return SpaceImage.NotImage
    if (!mime.startsWith("image/")) return SpaceImage.NotImage
    val limit = imageLimitBytes(banner)
    val bytes = resolver.openInputStream(uri)?.use { readAtMost(it, limit + 1) } ?: return SpaceImage.NotImage
    if (bytes.size > limit) return SpaceImage.TooBig(limit / (1024 * 1024))
    val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
    if (bounds.outWidth <= 0 || bounds.outHeight <= 0) return SpaceImage.NotImage
    return SpaceImage.Ok(mime, bytes, bounds.outWidth, bounds.outHeight)
}

/** Read up to [max] bytes of [input]. */
private fun readAtMost(input: java.io.InputStream, max: Int): ByteArray {
    val out = java.io.ByteArrayOutputStream()
    val buf = ByteArray(16 * 1024)
    while (out.size() < max) {
        val n = input.read(buf, 0, minOf(buf.size, max - out.size()))
        if (n < 0) break
        out.write(buf, 0, n)
    }
    return out.toByteArray()
}

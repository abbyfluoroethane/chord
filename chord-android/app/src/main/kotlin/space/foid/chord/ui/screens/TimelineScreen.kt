package space.foid.chord.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshots.Snapshot
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.LocalClipboard
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.toClipEntry
import android.content.ClipData
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.mapLatest
import android.widget.Toast
import androidx.compose.ui.res.stringResource
import space.foid.chord.ChordApp
import space.foid.chord.R
import space.foid.chord.ui.text.formatPalette
import space.foid.chord.ui.timeline.ownMentionNames
import space.foid.chord.data.TimelineTarget
import space.foid.chord.data.stableKey
import space.foid.chord.notify.ChordNotifications
import space.foid.chord.ui.components.ConnectionBanner
import space.foid.chord.ui.avatar.JidAvatar
import space.foid.chord.ui.components.ComposerBar
import space.foid.chord.ui.components.MessageRow
import space.foid.chord.ui.attachments.AttachmentReader
import space.foid.chord.ui.attachments.ImageViewer
import space.foid.chord.ui.attachments.PendingUploads
import space.foid.chord.viewmodel.UploadUi
import space.foid.chord.ui.sheets.AttachmentSheet
import android.net.Uri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import space.foid.chord.ui.composer.MessageActionsHost
import space.foid.chord.ui.composer.canModerate
import space.foid.chord.viewmodel.MemberListViewModel
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordRadius
import space.foid.chord.ui.theme.ChordSize
import space.foid.chord.ui.theme.ChordSpace
import space.foid.chord.ui.theme.ChordType
import space.foid.chord.ui.timeline.MessageUi
import space.foid.chord.ui.timeline.DateSeparator
import space.foid.chord.ui.timeline.JumpToPresent
import space.foid.chord.ui.timeline.NewDivider
import space.foid.chord.ui.timeline.RowChrome
import space.foid.chord.ui.timeline.StartOfHistory
import space.foid.chord.ui.timeline.TimelineHeaderContent
import space.foid.chord.ui.timeline.TypingLine
import space.foid.chord.ui.timeline.UnreadBar
import space.foid.chord.ui.timeline.rowChrome
import space.foid.chord.ui.timeline.typerNames
import space.foid.chord.ui.timeline.typingText
import space.foid.chord.ui.timeline.unreadBarText
import space.foid.chord.ui.timeline.unreadCount
import space.foid.chord.ui.timeline.unreadSince
import space.foid.chord.ui.timeline.clockLabel
import space.foid.chord.ui.components.Presence
import java.time.ZoneId
import java.util.Locale
import kotlinx.coroutines.flow.combine
import space.foid.chord.ui.timeline.continuesGroup
import space.foid.chord.ui.timeline.toMessageUi
import space.foid.chord.viewmodel.ChordViewModels
import space.foid.chord.viewmodel.TimelineViewModel
import uniffi.chord_ffi.TimelineItem

/** A message with the decision whether it continues the group above it. Built once per list change. */
@Immutable
data class TimelineRowUi(
    val message: MessageUi,
    val grouped: Boolean,
    /** The chrome above this message: a date separator and the NEW line. */
    val chrome: RowChrome = RowChrome(),
)

/**
 * Newest first, with the grouping flags. [oldestFirst] is the order of the core. This runs once
 * per list change, never in a row.
 */
fun buildTimelineRows(
    oldestFirst: List<MessageUi>,
    firstUnreadId: String? = null,
    zone: ZoneId = ZoneId.systemDefault(),
    locale: Locale = Locale.getDefault(),
): List<TimelineRowUi> {
    val chrome = rowChrome(oldestFirst, firstUnreadId, zone, locale)
    val out = ArrayList<TimelineRowUi>(oldestFirst.size)
    for (i in oldestFirst.indices.reversed()) {
        val prev = if (i > 0) oldestFirst[i - 1] else null
        out.add(TimelineRowUi(oldestFirst[i], continuesGroup(prev, oldestFirst[i], chrome[i].newDivider, zone), chrome[i]))
    }
    return out
}

/** Who a reply or an edit points at. */
private sealed interface Compose {
    data class Reply(val message: MessageUi) : Compose
    data class Edit(val message: MessageUi) : Compose
}

@OptIn(ExperimentalCoroutinesApi::class)
@Composable
fun TimelineScreen(
    target: TimelineTarget,
    title: String,
    onOpenChannels: () -> Unit,
    onOpenMembers: () -> Unit,
    modifier: Modifier = Modifier,
    onXmppLink: ((String) -> Unit)? = null,
    direct: Boolean? = null,
    onOpenProfile: (address: String, name: String) -> Unit = { _, _ -> },
    /** How many messages were unread when the chat was opened. The NEW line goes at the first. */
    unreadOnOpen: Int = 0,
    /** The room subject. The core has no call to read it yet, so callers pass null. */
    topic: String? = null,
) {
    val vm: TimelineViewModel = viewModel(key = target.toString(), factory = ChordViewModels.timeline(target))

    // The core items become rows on a background thread, once per diff.
    // The text is formatted here too, with the colours of the theme. A new theme builds the rows again.
    val context = LocalContext.current
    val palette = Chord.colors.formatPalette()
    val rowsFlow = remember(vm, palette) {
        val account = (context.applicationContext as? ChordApp)?.session?.client?.value?.account()
        combine(vm.items, vm.newFrom) { list, newFrom -> list to newFrom }
            .mapLatest { (list, newFrom) ->
                val names = ownMentionNames(account, list)
                val inRoom = target is TimelineTarget.Private ||
                    (direct?.not() ?: (list.isEmpty() || list.any { '/' in it.sender }))
                val now = System.currentTimeMillis()
                buildTimelineRows(
                    list.map { it.toMessageUi(palette = palette, ownNames = names, now = now, account = account, inRoom = inRoom) },
                    firstUnreadId = newFrom,
                )
            }
            .flowOn(Dispatchers.Default)
    }
    val rows by rowsFlow.collectAsStateWithLifecycle(initialValue = emptyList())
    val loaded by vm.loaded.collectAsStateWithLifecycle()
    val loadingOlder by vm.loadingOlder.collectAsStateWithLifecycle()
    val reachedStart by vm.reachedStart.collectAsStateWithLifecycle()
    val uploads by vm.uploads.collectAsStateWithLifecycle()

    var draft by rememberSaveable { mutableStateOf("") }
    var mode by remember { mutableStateOf<Compose?>(null) }
    var actionsFor by remember { mutableStateOf<MessageUi?>(null) }
    var attachOpen by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    var viewing by remember { mutableStateOf<MessageUi?>(null) }

    // The system pickers need no storage permission. The result is a content URI that we read
    // for the upload; the reader runs off the main thread.
    fun startUpload(uri: Uri?) {
        if (uri == null) return
        val name = AttachmentReader.info(context, uri).name
        vm.upload(name) { AttachmentReader.read(context, uri) }
    }
    val photoPicker = rememberLauncherForActivityResult(ActivityResultContracts.PickVisualMedia()) { startUpload(it) }
    val filePicker = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { startUpload(it) }

    LaunchedEffect(vm) {
        vm.errors.collect { error = it }
    }
    LaunchedEffect(vm, unreadOnOpen) { vm.openWithUnread(unreadOnOpen) }
    val newFrom by vm.newFrom.collectAsStateWithLifecycle()
    var barGone by remember(vm, unreadOnOpen) { mutableStateOf(false) }
    val typers by vm.typers.collectAsStateWithLifecycle()
    LaunchedEffect(error) {
        if (error != null) {
            delay(4_000)
            error = null
        }
    }

    // Notifications for this chat stay quiet while it is on screen and the app is resumed.
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    var resumed by remember { mutableStateOf(lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED)) }
    DisposableEffect(lifecycle) {
        val observer = androidx.lifecycle.LifecycleEventObserver { _, _ ->
            resumed = lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED)
        }
        lifecycle.addObserver(observer)
        onDispose { lifecycle.removeObserver(observer) }
    }
    val peer = when (target) {
        is TimelineTarget.Room -> target.jid
        is TimelineTarget.Private -> "${target.room}/${target.nick}"
    }
    DisposableEffect(peer, resumed) {
        if (resumed) ChordNotifications.activePeer = peer
        onDispose { if (ChordNotifications.activePeer == peer) ChordNotifications.activePeer = null }
    }

    val listState = rememberLazyListState()
    val atBottom by remember(listState) { derivedStateOf { listState.isAtBottom() } }
    val newestId = rows.firstOrNull()?.message?.id
    LaunchedEffect(newestId, atBottom, resumed) {
        if (resumed && atBottom && newestId != null) vm.markRead()
    }

    val clipboard = LocalClipboard.current
    val copyScope = androidx.compose.runtime.rememberCoroutineScope()
    val copiedText = stringResource(R.string.xmpp_address_copied)
    val xmppHandler: (String) -> Unit = onXmppLink ?: { uri ->
        // The default: copy the address. A join flow can replace it with onXmppLink.
        copyScope.launch { clipboard.setClipEntry(ClipData.newPlainText("xmpp", uri).toClipEntry()) }
        Toast.makeText(context, copiedText, Toast.LENGTH_SHORT).show()
    }
    // A 1:1 chat shows no "#". When the caller does not know the kind (a notification tap),
    // a room shows itself by its senders: they are room@service/nick.
    val items by vm.items.collectAsStateWithLifecycle()
    val isRoom = target is TimelineTarget.Room &&
        (direct?.not() ?: (items.isEmpty() || items.any { '/' in it.sender }))
    val unreadBar = remember(rows, newFrom) {
        val oldestFirst = rows.map { it.message }.asReversed()
        val count = unreadCount(oldestFirst, newFrom)
        val since = unreadSince(oldestFirst, newFrom)
        if (count > 0 && since != null) unreadBarText(count, clockLabel(since)) else null
    }
    // The presence of the other person in a 1:1 chat: the member list has both people.
    val peerPresence = if (!isRoom && target is TimelineTarget.Room) {
        val members by viewModel<MemberListViewModel>(
            key = "dm-presence/${target.jid}",
            factory = ChordViewModels.memberList(bareJid(target.jid)),
        ).members.collectAsStateWithLifecycle()
        val account = remember { (context.applicationContext as? ChordApp)?.session?.client?.value?.account() }
        dmPeerPresence(members, account)
    } else null
    // The members of a room: the nicks for @mentions, and whether the account moderates.
    val members = if (isRoom && target is TimelineTarget.Room) {
        val memberVm: MemberListViewModel = viewModel(key = "members/${target.jid}", factory = ChordViewModels.memberList(target.jid))
        memberVm.members.collectAsStateWithLifecycle().value
    } else {
        emptyList()
    }
    val mentionNicks = remember(members) { members.map { it.name } }
    val moderator = remember(members) {
        canModerate(members, (context.applicationContext as? ChordApp)?.session?.client?.value?.account())
    }
    val replying = (mode as? Compose.Reply)?.message
    val editing = (mode as? Compose.Edit)?.message

    TimelineContent(
        rows = rows,
        title = title,
        isRoom = isRoom,
        topic = topic,
        presence = peerPresence,
        unreadBar = unreadBar.takeIf { !barGone },
        onMarkRead = {
            barGone = true
            vm.markRead()
        },
        typing = typingText(typerNames(typers, if (isRoom) null else title)),
        loaded = loaded,
        loadingOlder = loadingOlder,
        reachedStart = reachedStart,
        composerText = draft,
        onComposerChange = {
            draft = it
            vm.onComposerChanged(it)
        },
        onSend = {
            val text = draft.trim()
            if (text.isNotEmpty()) {
                when {
                    editing != null -> vm.edit(editing.id, text)
                    replying != null -> vm.reply(replying.id, text)
                    else -> vm.send(text)
                }
                draft = ""
                mode = null
                vm.onComposerChanged("")
            }
        },
        replyingTo = replying?.senderName,
        onCancelReply = { mode = null },
        editing = editing != null,
        onCancelEdit = {
            mode = null
            draft = ""
        },
        onAttach = { attachOpen = true },
        mentionNicks = mentionNicks,
        onOpenChannels = onOpenChannels,
        onOpenMembers = onOpenMembers,
        onLongPress = { actionsFor = it },
        onXmppLink = xmppHandler,
        onOpenProfile = { onOpenProfile(it.senderId, it.senderName) },
        onReactionClick = { id, emoji -> vm.toggleReaction(id, emoji) },
        onRetry = { vm.send(it.body) },
        onImageClick = { viewing = it },
        uploads = uploads,
        onRetryUpload = vm::retryUpload,
        onDismissUpload = vm::dismissUpload,
        onLoadOlder = vm::loadOlder,
        error = error,
        listState = listState,
        modifier = modifier,
    )

    actionsFor?.let { m ->
        MessageActionsHost(
            message = m,
            vm = vm,
            target = target,
            direct = target is TimelineTarget.Room && !isRoom,
            moderator = moderator,
            messageId = items.firstOrNull { it.id == m.id }?.let { it.stanzaId ?: it.originId ?: it.id } ?: m.id,
            onDismiss = { actionsFor = null },
            onReply = { mode = Compose.Reply(m) },
            onEdit = {
                mode = Compose.Edit(m)
                draft = m.body
            },
        )
    }
    if (attachOpen) {
        AttachmentSheet(
            onDismiss = { attachOpen = false },
            onPickImage = {
                attachOpen = false
                photoPicker.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageAndVideo))
            },
            onPickFile = {
                attachOpen = false
                filePicker.launch(arrayOf("*/*"))
            },
        )
    }
    viewing?.let { m ->
        m.attachment?.let { url -> ImageViewer(url = url, outgoing = m.outgoing, onDismiss = { viewing = null }) }
    }
}

/** The presence of the other person in a 1:1 chat: the member that is not the own account. Null if unknown. */
fun dmPeerPresence(members: List<uniffi.chord_ffi.MemberItem>, account: String?): Presence? {
    val own = account?.let(::bareJid)
    val peer = members.firstOrNull { m -> m.jid?.let(::bareJid) != own } ?: return null
    return memberPresence(peer)
}

/** True when the newest row (index 0 of the reversed list) is at its resting place, bottom edge. */
private fun LazyListState.isAtBottom(): Boolean = firstVisibleItemIndex == 0 && firstVisibleItemScrollOffset < 24

private const val PAGINATE_WITHIN = 15

/**
 * The stateless timeline: top bar, message list, composer. No ViewModel, no native client.
 *
 * LIST DIRECTION. `LazyColumn(reverseLayout = true)` over [rows], newest first. Index 0 is the
 * bottom edge, so (1) the list is anchored to the newest message with no scroll code when the
 * content is shorter than the screen or the keyboard resizes the viewport, (2) older messages
 * are appended at the END of the list (the top of the screen), so the visible items keep their
 * index and position when a page is prepended, with no jump, and (3) a new message at index 0
 * only needs a `scrollToItem(0)` when the user was at the bottom. A normal layout would need
 * `scrollToItem(last)` tricks and offset correction for every prepend.
 *
 * [rows] must be newest first, built by [buildTimelineRows].
 *
 * @param onRowComposed test hook: called with the message id each time a row recomposes.
 */
@Composable
fun TimelineContent(
    rows: List<TimelineRowUi>,
    title: String,
    modifier: Modifier = Modifier,
    isRoom: Boolean = true,
    topic: String? = null,
    presence: Presence? = null,
    /** "13 new messages since 12:14" for the bar at the top, or null for no bar. */
    unreadBar: String? = null,
    onMarkRead: () -> Unit = {},
    /** The typing line, "Bay is typing…", or empty. */
    typing: String = "",
    loaded: Boolean = true,
    loadingOlder: Boolean = false,
    reachedStart: Boolean = false,
    composerText: String = "",
    onComposerChange: (String) -> Unit = {},
    onSend: () -> Unit = {},
    replyingTo: String? = null,
    onCancelReply: () -> Unit = {},
    editing: Boolean = false,
    onCancelEdit: () -> Unit = {},
    onAttach: () -> Unit = {},
    mentionNicks: List<String> = emptyList(),
    onOpenChannels: () -> Unit = {},
    onOpenMembers: () -> Unit = {},
    onLongPress: (MessageUi) -> Unit = {},
    onReactionClick: (messageId: String, emoji: String) -> Unit = { _, _ -> },
    onRetry: (MessageUi) -> Unit = {},
    onImageClick: (MessageUi) -> Unit = {},
    uploads: List<UploadUi> = emptyList(),
    onRetryUpload: (Long) -> Unit = {},
    onDismissUpload: (Long) -> Unit = {},
    onXmppLink: (String) -> Unit = {},
    onOpenProfile: (MessageUi) -> Unit = {},
    onLoadOlder: () -> Unit = {},
    error: String? = null,
    listState: LazyListState = rememberLazyListState(),
    onRowComposed: ((String) -> Unit)? = null,
    /** Shows the "Jump to present" button when the list is scrolled up. Off for the showcase screenshots. */
    jumpToPresent: Boolean = true,
) {
    val colors = Chord.colors
    Column(modifier.fillMaxSize().background(colors.surface100)) {
        TimelineHeaderContent(
            title = title,
            isRoom = isRoom,
            onOpenChannels = onOpenChannels,
            onOpenMembers = onOpenMembers,
            topic = topic,
            presence = presence,
        )
        ConnectionBanner()
        Box(Modifier.weight(1f).fillMaxWidth()) {
            val newestKey = rows.firstOrNull()?.message?.id
            val newestOutgoing = rows.firstOrNull()?.message?.outgoing == true
            // Read in composition, before the list measures the new row: the answer is the
            // position the user had. Not observed, it must not recompose the content.
            val wasAtBottom = remember(newestKey) { Snapshot.withoutReadObservation { listState.isAtBottom() } }
            var firstKey by remember { mutableStateOf<String?>(null) }
            LaunchedEffect(newestKey) {
                if (newestKey != null && newestKey != firstKey) {
                    if (firstKey == null || wasAtBottom || newestOutgoing) listState.scrollToItem(0)
                    firstKey = newestKey
                }
            }

            val onLoadOlderNow by rememberUpdatedState(onLoadOlder)
            val loadingNow by rememberUpdatedState(loadingOlder)
            val reachedNow by rememberUpdatedState(reachedStart)
            LaunchedEffect(listState, loaded) {
                if (!loaded) return@LaunchedEffect
                snapshotFlow {
                    val info = listState.layoutInfo
                    val last = info.visibleItemsInfo.lastOrNull()?.index ?: -1
                    // Near the oldest end: the oldest row is the last item of the reversed list.
                    // Depends on the loading flag, so it fires again when a page arrived.
                    Triple(last, info.totalItemsCount, loadingNow)
                }
                    .distinctUntilChanged()
                    .filter { (last, total, loadingFlag) ->
                        !loadingFlag && !reachedNow && last >= 0 && total - 1 - last <= PAGINATE_WITHIN
                    }
                    .collect { onLoadOlderNow() }
            }

            LazyColumn(
                state = listState,
                reverseLayout = true,
                modifier = Modifier.fillMaxSize().testTag("timeline"),
            ) {
                items(rows, key = { it.message.id }, contentType = { "message" }) { row ->
                    TimelineRow(row, onLongPress, onReactionClick, onRetry, onImageClick, onXmppLink, onOpenProfile, onRowComposed)
                }
                // After the rows: the top of the screen.
                if (loadingOlder) {
                    item(key = "loading-older", contentType = "loading") { LoadingRow() }
                }
                if (reachedStart && rows.isNotEmpty()) {
                    item(key = "beginning", contentType = "beginning") { StartOfHistory(title, isRoom) }
                }
            }

            // An empty chat shows the start of the history at the top, as the desktop does.
            if (loaded && rows.isEmpty()) {
                StartOfHistory(title, isRoom, Modifier.align(Alignment.TopStart))
            }
            if (unreadBar != null) {
                UnreadBar(unreadBar, onMarkRead, Modifier.align(Alignment.TopCenter))
            }

            val away by remember(listState) { derivedStateOf { listState.firstVisibleItemIndex >= 2 } }
            if (away && jumpToPresent) {
                val scope = androidx.compose.runtime.rememberCoroutineScope()
                JumpToPresent(
                    onClick = { scope.launch { listState.animateScrollToItem(0) } },
                    modifier = Modifier.align(Alignment.BottomCenter).padding(bottom = ChordSpace.s3),
                )
            }
            if (error != null) {
                Toast(error, Modifier.align(Alignment.BottomCenter).padding(horizontal = ChordSpace.s4, vertical = ChordSpace.s3))
            }
        }
        PendingUploads(uploads, onRetry = onRetryUpload, onDismiss = onDismissUpload)
        TypingLine(typing)
        ComposerBar(
            text = composerText,
            onTextChange = onComposerChange,
            onSend = onSend,
            placeholder = stringResource(if (isRoom) R.string.composer_placeholder_room else R.string.composer_placeholder_direct, title),
            mentionNicks = mentionNicks,
            replyingTo = replyingTo,
            onCancelReply = onCancelReply,
            editing = editing,
            onCancelEdit = onCancelEdit,
            onAttach = onAttach,
            inputModifier = Modifier.testTag("composer_input"),
            sendModifier = Modifier.testTag("composer_send"),
        )
    }
}

/** One row. Its own scope, so a row whose inputs are equal is skipped. */
@Composable
private fun TimelineRow(
    row: TimelineRowUi,
    onLongPress: (MessageUi) -> Unit,
    onReactionClick: (String, String) -> Unit,
    onRetry: (MessageUi) -> Unit,
    onImageClick: (MessageUi) -> Unit,
    onXmppLink: (String) -> Unit,
    onOpenProfile: (MessageUi) -> Unit,
    onRowComposed: ((String) -> Unit)?,
) {
    if (onRowComposed != null) SideEffect { onRowComposed(row.message.id) }
    val m = row.message
    Column {
        // The chrome is part of the row, so the list keys and the scroll position stay as they are.
        // The list is reversed, but a row draws top down: the chrome is above its message.
        row.chrome.dayLabel?.let { DateSeparator(it) }
        if (row.chrome.newDivider) NewDivider()
        MessageRow(
            message = m,
            grouped = row.grouped,
            avatar = { JidAvatar(owner = m.senderId, name = m.senderName, hash = m.avatarUrl) },
            modifier = Modifier.testTag("message_row"),
            onLongPress = { onLongPress(m) },
            onReactionClick = { emoji -> onReactionClick(m.id, emoji) },
            onRetryClick = { onRetry(m) },
            onImageClick = { onImageClick(m) },
            onXmppLink = onXmppLink,
            onProfileClick = { onOpenProfile(m) },
        )
    }
}

@Composable
private fun LoadingRow() {
    Box(Modifier.fillMaxWidth().padding(ChordSpace.s4), contentAlignment = Alignment.Center) {
        Text(stringResource(R.string.timeline_loading_older), style = ChordType.bodySmall, color = Chord.colors.inkMuted)
    }
}

@Composable
private fun Toast(text: String, modifier: Modifier) {
    val colors = Chord.colors
    val shape = RoundedCornerShape(ChordRadius.md)
    Box(
        modifier
            .fillMaxWidth()
            .background(colors.surfaceRaised, shape)
            .border(1.dp, colors.lineStrong, shape)
            .padding(horizontal = ChordSpace.s4, vertical = ChordSpace.s3)
            .testTag("timeline_error"),
    ) {
        Text(text, style = ChordType.bodySmall, color = colors.ink)
    }
}

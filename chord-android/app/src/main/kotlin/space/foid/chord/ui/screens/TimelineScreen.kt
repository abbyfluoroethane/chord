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
import space.foid.chord.data.TimelineTarget
import space.foid.chord.data.stableKey
import space.foid.chord.notify.ChordNotifications
import space.foid.chord.ui.components.ConnectionBanner
import space.foid.chord.ui.avatar.JidAvatar
import space.foid.chord.ui.components.ComposerBar
import space.foid.chord.ui.components.MessageRow
import space.foid.chord.ui.sheets.AttachmentSheet
import space.foid.chord.ui.sheets.MessageActionsSheet
import space.foid.chord.ui.sheets.ReactionPickerSheet
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordRadius
import space.foid.chord.ui.theme.ChordSize
import space.foid.chord.ui.theme.ChordSpace
import space.foid.chord.ui.theme.ChordType
import space.foid.chord.ui.timeline.MessageUi
import space.foid.chord.ui.timeline.continuesGroup
import space.foid.chord.ui.timeline.toMessageUi
import space.foid.chord.viewmodel.ChordViewModels
import space.foid.chord.viewmodel.TimelineViewModel
import uniffi.chord_ffi.TimelineItem

/** A message with the decision whether it continues the group above it. Built once per list change. */
@Immutable
data class TimelineRowUi(val message: MessageUi, val grouped: Boolean)

/**
 * Newest first, with the grouping flags. [oldestFirst] is the order of the core. This runs once
 * per list change, never in a row.
 */
fun buildTimelineRows(oldestFirst: List<MessageUi>): List<TimelineRowUi> {
    val out = ArrayList<TimelineRowUi>(oldestFirst.size)
    for (i in oldestFirst.indices.reversed()) {
        val prev = if (i > 0) oldestFirst[i - 1] else null
        out.add(TimelineRowUi(oldestFirst[i], continuesGroup(prev, oldestFirst[i])))
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
) {
    val vm: TimelineViewModel = viewModel(key = target.toString(), factory = ChordViewModels.timeline(target))

    // The core items become rows on a background thread, once per diff.
    val rowsFlow = remember(vm) {
        vm.items
            .mapLatest { list -> buildTimelineRows(list.map(TimelineItem::toMessageUi)) }
            .flowOn(Dispatchers.Default)
    }
    val rows by rowsFlow.collectAsStateWithLifecycle(initialValue = emptyList())
    val loaded by vm.loaded.collectAsStateWithLifecycle()
    val loadingOlder by vm.loadingOlder.collectAsStateWithLifecycle()
    val reachedStart by vm.reachedStart.collectAsStateWithLifecycle()

    var draft by rememberSaveable { mutableStateOf("") }
    var mode by remember { mutableStateOf<Compose?>(null) }
    var actionsFor by remember { mutableStateOf<MessageUi?>(null) }
    var reactionFor by remember { mutableStateOf<MessageUi?>(null) }
    var attachOpen by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }

    LaunchedEffect(vm) {
        vm.errors.collect { error = it }
    }
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
    val replying = (mode as? Compose.Reply)?.message
    val editing = (mode as? Compose.Edit)?.message

    TimelineContent(
        rows = rows,
        title = title,
        isRoom = target is TimelineTarget.Room,
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
        onOpenChannels = onOpenChannels,
        onOpenMembers = onOpenMembers,
        onLongPress = { actionsFor = it },
        onReactionClick = { id, emoji -> vm.toggleReaction(id, emoji) },
        onRetry = { vm.send(it.body) },
        onLoadOlder = vm::loadOlder,
        error = error,
        listState = listState,
        modifier = modifier,
    )

    actionsFor?.let { m ->
        val own = m.outgoing && !m.retracted
        MessageActionsSheet(
            message = m,
            canEdit = own,
            canRetract = own,
            onDismiss = { actionsFor = null },
            onReply = {
                actionsFor = null
                mode = Compose.Reply(m)
            },
            onEdit = {
                actionsFor = null
                mode = Compose.Edit(m)
                draft = m.body
            },
            onRetract = {
                actionsFor = null
                vm.retract(m.id)
            },
            onCopy = {
                actionsFor = null
                copyScope.launch { clipboard.setClipEntry(ClipData.newPlainText("message", m.body).toClipEntry()) }
            },
            onReact = { emoji ->
                actionsFor = null
                vm.toggleReaction(m.id, emoji)
            },
            onMoreReactions = {
                actionsFor = null
                reactionFor = m
            },
        )
    }
    reactionFor?.let { m ->
        ReactionPickerSheet(
            onDismiss = { reactionFor = null },
            onPick = { emoji ->
                reactionFor = null
                vm.toggleReaction(m.id, emoji)
            },
        )
    }
    if (attachOpen) {
        // The core has no upload call yet, so both choices close the sheet.
        AttachmentSheet(
            onDismiss = { attachOpen = false },
            onPickImage = { attachOpen = false },
            onPickFile = { attachOpen = false },
        )
    }
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
    onOpenChannels: () -> Unit = {},
    onOpenMembers: () -> Unit = {},
    onLongPress: (MessageUi) -> Unit = {},
    onReactionClick: (messageId: String, emoji: String) -> Unit = { _, _ -> },
    onRetry: (MessageUi) -> Unit = {},
    onLoadOlder: () -> Unit = {},
    error: String? = null,
    listState: LazyListState = rememberLazyListState(),
    onRowComposed: ((String) -> Unit)? = null,
) {
    val colors = Chord.colors
    Column(modifier.fillMaxSize().background(colors.surface100)) {
        TopBar(title = title, isRoom = isRoom, onOpenChannels = onOpenChannels, onOpenMembers = onOpenMembers)
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
                    TimelineRow(row, onLongPress, onReactionClick, onRetry, onRowComposed)
                }
                // After the rows: the top of the screen.
                if (loadingOlder) {
                    item(key = "loading-older", contentType = "loading") { LoadingRow() }
                }
                if (reachedStart) {
                    item(key = "beginning", contentType = "beginning") { BeginningHeader(title, isRoom) }
                }
            }

            if (loaded && rows.isEmpty()) {
                Column(
                    Modifier.fillMaxSize().padding(ChordSpace.s6),
                    verticalArrangement = Arrangement.Center,
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    Text("No messages yet", style = ChordType.name, color = colors.ink)
                    Text(
                        "Say hello in ${if (isRoom) "#" else ""}$title.",
                        style = ChordType.body,
                        color = colors.inkMuted,
                        textAlign = TextAlign.Center,
                    )
                }
            }

            val away by remember(listState) { derivedStateOf { listState.firstVisibleItemIndex >= 2 } }
            if (away) {
                val scope = androidx.compose.runtime.rememberCoroutineScope()
                JumpPill(
                    onClick = { scope.launch { listState.animateScrollToItem(0) } },
                    modifier = Modifier.align(Alignment.BottomCenter).padding(bottom = ChordSpace.s3),
                )
            }
            if (error != null) {
                Toast(error, Modifier.align(Alignment.BottomCenter).padding(horizontal = ChordSpace.s4, vertical = ChordSpace.s3))
            }
        }
        ComposerBar(
            text = composerText,
            onTextChange = onComposerChange,
            onSend = onSend,
            placeholder = if (isRoom) "Message #$title" else "Message $title",
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
    onRowComposed: ((String) -> Unit)?,
) {
    if (onRowComposed != null) SideEffect { onRowComposed(row.message.id) }
    val m = row.message
    MessageRow(
        message = m,
        grouped = row.grouped,
        avatar = { JidAvatar(owner = m.senderId, name = m.senderName, hash = m.avatarUrl) },
        modifier = Modifier.testTag("message_row"),
        onLongPress = { onLongPress(m) },
        onReactionClick = { emoji -> onReactionClick(m.id, emoji) },
        onRetryClick = { onRetry(m) },
    )
}

@Composable
private fun TopBar(title: String, isRoom: Boolean, onOpenChannels: () -> Unit, onOpenMembers: () -> Unit) {
    val colors = Chord.colors
    Column(Modifier.fillMaxWidth().background(colors.surface100).statusBarsPadding()) {
        Row(Modifier.fillMaxWidth().height(ChordSize.bar), verticalAlignment = Alignment.CenterVertically) {
            BarButton("Channels", onOpenChannels, Modifier.testTag("drawer_open_channels")) { MenuGlyph(colors.ink) }
            if (isRoom) {
                Text("#", style = ChordType.title, color = colors.inkMuted)
                Text(
                    title,
                    style = ChordType.title,
                    color = colors.ink,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f).padding(start = ChordSpace.s1),
                )
            } else {
                JidAvatar(owner = title, name = title, size = 28.dp)
                Text(
                    title,
                    style = ChordType.title,
                    color = colors.ink,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f).padding(start = ChordSpace.s2),
                )
            }
            BarButton("Members", onOpenMembers, Modifier.testTag("drawer_open_members")) { MembersGlyph(colors.ink) }
        }
        Box(Modifier.fillMaxWidth().height(1.dp).background(colors.line))
    }
}

@Composable
private fun BarButton(label: String, onClick: () -> Unit, modifier: Modifier, glyph: @Composable () -> Unit) {
    Box(
        modifier
            .size(ChordSize.bar)
            .semantics { contentDescription = label; role = Role.Button }
            .clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
    ) { glyph() }
}

@Composable
private fun MenuGlyph(color: Color) {
    Box(
        Modifier.size(20.dp).drawBehind {
            val w = 2.dp.toPx()
            for (f in listOf(0.2f, 0.5f, 0.8f)) {
                drawLine(color, Offset(0f, size.height * f), Offset(size.width, size.height * f), w, StrokeCap.Round)
            }
        },
    )
}

@Composable
private fun MembersGlyph(color: Color) {
    // A head and shoulders.
    Box(
        Modifier.size(22.dp).drawBehind {
            val w = 2.dp.toPx()
            drawCircle(color, radius = size.width * 0.19f, center = Offset(size.width / 2, size.height * 0.32f), style = Stroke(w))
            drawArc(
                color, startAngle = 180f, sweepAngle = 180f, useCenter = false,
                topLeft = Offset(size.width * 0.12f, size.height * 0.58f),
                size = Size(size.width * 0.76f, size.height * 0.7f),
                style = Stroke(w, cap = StrokeCap.Round),
            )
        },
    )
}

@Composable
private fun LoadingRow() {
    Box(Modifier.fillMaxWidth().padding(ChordSpace.s4), contentAlignment = Alignment.Center) {
        Text("Loading older messages…", style = ChordType.bodySmall, color = Chord.colors.inkMuted)
    }
}

@Composable
private fun BeginningHeader(title: String, isRoom: Boolean) {
    val colors = Chord.colors
    Column(Modifier.fillMaxWidth().padding(horizontal = ChordSpace.s4, vertical = ChordSpace.s6)) {
        Text(
            "Beginning of ${if (isRoom) "#" else ""}$title",
            style = ChordType.title,
            color = colors.ink,
        )
        Text(
            "This is the start of the conversation.",
            style = ChordType.bodySmall,
            color = colors.inkMuted,
            modifier = Modifier.padding(top = ChordSpace.s1),
        )
    }
}

@Composable
private fun JumpPill(onClick: () -> Unit, modifier: Modifier) {
    val colors = Chord.colors
    Box(
        modifier
            .background(colors.brand, CircleShape)
            .clickable(onClick = onClick)
            .padding(horizontal = ChordSpace.s4, vertical = ChordSpace.s2)
            .testTag("jump_to_latest"),
    ) {
        Text("Jump to latest", style = ChordType.label, color = colors.onBrand)
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

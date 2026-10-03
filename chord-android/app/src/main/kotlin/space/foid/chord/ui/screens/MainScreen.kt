package space.foid.chord.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.material3.SnackbarHostState
import android.content.ClipData
import androidx.compose.ui.platform.LocalClipboard
import androidx.compose.ui.platform.toClipEntry
import androidx.lifecycle.viewmodel.compose.viewModel
import space.foid.chord.ui.inbox.InboxCallbacks
import space.foid.chord.ui.inbox.InboxSheet
import space.foid.chord.ui.inbox.NoticeSnackbarHost
import space.foid.chord.ui.join.ChannelActionsSheet
import space.foid.chord.ui.join.JoinCallbacks
import space.foid.chord.ui.join.LocalOpenXmppUri
import space.foid.chord.ui.join.NewConversationSheet
import space.foid.chord.ui.join.XmppLinkInbox
import space.foid.chord.ui.join.actionTarget
import space.foid.chord.viewmodel.InboxViewModel
import space.foid.chord.viewmodel.JoinEvent
import space.foid.chord.viewmodel.JoinViewModel
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import space.foid.chord.ChordApp
import space.foid.chord.data.TimelineTarget
import space.foid.chord.ui.layout.DrawerPane
import space.foid.chord.ui.profile.DmProfilePane
import space.foid.chord.ui.profile.ProfileSheet
import space.foid.chord.ui.layout.DualDrawer
import space.foid.chord.ui.layout.rememberDualDrawerState
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordSpace
import space.foid.chord.ui.theme.ChordType
import uniffi.chord_ffi.ChannelKind
import uniffi.chord_ffi.ChannelScope

/** The timeline target of a channel jid. A private chat with an occupant has the jid `room/nick`. */
fun timelineTargetFor(jid: String): TimelineTarget =
    if ('/' in jid) TimelineTarget.Private(jid.substringBefore('/'), jid.substringAfter('/')) else TimelineTarget.Room(jid)

/** The jid of the room whose members the right drawer shows. */
fun memberRoomFor(jid: String): String = bareJid(jid)

private fun scopeToString(s: ChannelScope): String = when (s) {
    is ChannelScope.Space -> "${s.service}|${s.node}"
    else -> ""
}

private fun scopeFromString(s: String): ChannelScope =
    if ('|' in s) ChannelScope.Space(s.substringBefore('|'), s.substringAfter('|')) else ChannelScope.Home

/**
 * The main screen, laid out like Discord on a phone: the timeline of the current room in the middle,
 * the spaces and channels in a drawer on the left, the members in a drawer on the right. Swipe the
 * middle pane sideways to open a drawer. The selected channel survives process death.
 *
 * @param openPeer the jid that a notification asks for. The screen selects that channel.
 */
@Composable
fun MainScreen(
    onSignedOut: () -> Unit,
    openPeer: String?,
    modifier: Modifier = Modifier,
    onOpenSettings: () -> Unit = {},
) {
    val session = (LocalContext.current.applicationContext as ChordApp).session
    val scope = rememberCoroutineScope()

    var selectedJid by rememberSaveable { mutableStateOf(openPeer.orEmpty()) }
    var selectedName by rememberSaveable { mutableStateOf(openPeer?.let(::bareJid)?.substringBefore('@').orEmpty()) }
    // Null when the kind is not known, for example after a notification tap.
    var selectedDirect by rememberSaveable { mutableStateOf<Boolean?>(null) }
    // The person whose profile sheet is open: address and name.
    var profileOf by remember { mutableStateOf<Pair<String, String>?>(null) }
    // The unread count of the chat when it was picked. The timeline puts its NEW line there.
    var selectedUnread by remember { mutableStateOf(0) }
    var scopeKey by rememberSaveable { mutableStateOf("") }
    val drawer = rememberDualDrawerState(if (selectedJid.isEmpty()) DrawerPane.Left else DrawerPane.Center)
    val channelScope = remember(scopeKey) { scopeFromString(scopeKey) }

    LaunchedEffect(openPeer) {
        if (openPeer != null && openPeer != selectedJid) {
            selectedJid = openPeer
            selectedName = bareJid(openPeer).substringBefore('@')
            selectedDirect = null
            selectedUnread = 0
            drawer.close()
        }
    }

    val account = remember(session) { session.client.value?.let { AccountUi(it.account()) } }

    // Start, join and leave conversations; the inbox; notices.
    val joinVm: JoinViewModel = viewModel(factory = JoinViewModel.factory)
    val inboxVm: InboxViewModel = viewModel(factory = InboxViewModel.factory)
    val join by joinVm.state.collectAsState()
    val actions by joinVm.actions.collectAsState()
    val inbox by inboxVm.state.collectAsState()
    var inboxOpen by rememberSaveable { mutableStateOf(false) }
    val snackbar = remember { SnackbarHostState() }
    val clipboard = LocalClipboard.current

    val onJoinEvent: (JoinEvent) -> Unit = { e ->
        when (e) {
            is JoinEvent.OpenChannel -> {
                selectedJid = e.jid
                selectedName = e.name
                selectedDirect = e.direct
                selectedUnread = 0
                scope.launch { drawer.close() }
            }
            is JoinEvent.OpenSpace -> scopeKey = scopeToString(ChannelScope.Space(e.service, e.node))
            is JoinEvent.Left -> {
                if (bareJid(selectedJid) == e.jid) {
                    selectedJid = ""
                    selectedName = ""
                }
                // With no channel open, the channel list stays in view.
                if (selectedJid.isEmpty()) scope.launch { drawer.openLeft() }
            }
            JoinEvent.SpaceRequested -> inboxVm.refreshPending()
            is JoinEvent.Message -> scope.launch { snackbar.showSnackbar(e.text) }
        }
    }
    val currentOnJoinEvent by rememberUpdatedState(onJoinEvent)
    LaunchedEffect(joinVm) { joinVm.events.collect { currentOnJoinEvent(it) } }
    LaunchedEffect(inboxVm) { inboxVm.events.collect { currentOnJoinEvent(it) } }

    // A link from outside the app waits in XmppLinkInbox until this screen shows.
    val pendingUri by XmppLinkInbox.pending.collectAsState()
    LaunchedEffect(pendingUri) {
        pendingUri?.let { uri ->
            XmppLinkInbox.consume(uri)
            joinVm.openXmppUri(uri)
        }
    }

    CompositionLocalProvider(LocalOpenXmppUri provides joinVm::openXmppUri) {
        Box(modifier) {
            DualDrawer(
                state = drawer,
                modifier = Modifier.fillMaxSize(),
                left = {
                    ChannelDrawer(
                        scope = channelScope,
                        onScope = { scopeKey = scopeToString(it) },
                        selectedJid = selectedJid,
                        onSelect = { ch ->
                            selectedJid = ch.jid
                            selectedName = ch.name.ifBlank { bareJid(ch.jid) }
                            selectedDirect = ch.kind is ChannelKind.Direct
                            selectedUnread = ch.unread.toInt()
                            scope.launch { drawer.close() }
                        },
                        account = account,
                        onSignOut = { scope.launch { session.signOut(); onSignedOut() } },
                        inboxCount = inbox.count,
                        onInbox = { inboxVm.refreshPending(); inboxOpen = true },
                        onNew = { joinVm.show() },
                        onLongPress = { ch -> joinVm.openActions(ch.actionTarget()) },
                        onOpenSettings = onOpenSettings,
                    )
                },
                right = {
                    if (selectedJid.isNotEmpty()) {
                        if (selectedDirect == true && '/' !in selectedJid) {
                            DmProfilePane(
                                address = selectedJid, name = selectedName,
                                onOpenSettings = onOpenSettings,
                                onViewFull = { profileOf = selectedJid to selectedName },
                            )
                        } else {
                            MemberDrawer(
                                memberRoomFor(selectedJid), selectedName,
                                onOpenProfile = { m -> profileOf = (m.jid ?: m.id) to m.name },
                            )
                        }
                    }
                },
            ) {
                if (selectedJid.isEmpty()) {
                    NoChannelSelected(onOpenChannels = { scope.launch { drawer.openLeft() } })
                } else {
                    // A new key per channel: the screen starts fresh, with its own ViewModel.
                    androidx.compose.runtime.key(selectedJid) {
                        TimelineScreen(
                            target = timelineTargetFor(selectedJid),
                            title = selectedName,
                            onOpenChannels = { scope.launch { drawer.openLeft() } },
                            onOpenMembers = { scope.launch { drawer.openRight() } },
                            onXmppLink = joinVm::openXmppUri,
                            direct = selectedDirect,
                            onOpenProfile = { a, n -> profileOf = a to n },
                            unreadOnOpen = selectedUnread,
                        )
                    }
                }
            }
            NoticeSnackbarHost(snackbar, Modifier.align(Alignment.BottomCenter))
        }
    }

    profileOf?.let { (address, name) ->
        ProfileSheet(
            address = address,
            name = name,
            room = selectedJid.takeIf { it.isNotEmpty() && selectedDirect != true }?.let(::memberRoomFor),
            onDismiss = { profileOf = null },
            onMessage = { a, n ->
                selectedJid = a
                selectedName = n
                selectedDirect = true
                scope.launch { drawer.close() }
            },
            onOpenSettings = onOpenSettings,
        )
    }
    if (join.visible) {
        NewConversationSheet(join, JoinCallbacks.of(joinVm), onDismiss = joinVm::dismiss)
    }
    if (inboxOpen) {
        InboxSheet(inbox, InboxCallbacks.of(inboxVm), onDismiss = { inboxOpen = false; inboxVm.clearError() })
    }
    actions?.let { state ->
        ChannelActionsSheet(
            state = state,
            onDismiss = joinVm::closeActions,
            onMarkRead = joinVm::markRead,
            onLevel = joinVm::setLevel,
            onCopy = {
                scope.launch { clipboard.setClipEntry(ClipData.newPlainText("address", state.target.address).toClipEntry()) }
            },
            onLeave = joinVm::leave,
        )
    }
}

/** The center pane while no channel is selected. */
@Composable
fun NoChannelSelected(onOpenChannels: () -> Unit, modifier: Modifier = Modifier) {
    val c = Chord.colors
    Column(
        modifier.fillMaxSize().background(c.surface100).statusBarsPadding().padding(ChordSpace.s6),
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text("Pick a channel", style = ChordType.title, color = c.ink)
        Box(Modifier.padding(top = 16.dp)) {
            Button(onClick = onOpenChannels) { Text("Open channels") }
        }
    }
}

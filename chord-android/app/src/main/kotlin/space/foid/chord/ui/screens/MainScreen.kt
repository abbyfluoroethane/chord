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
import androidx.compose.runtime.rememberCoroutineScope
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
import space.foid.chord.ui.layout.DualDrawer
import space.foid.chord.ui.layout.rememberDualDrawerState
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordSpace
import space.foid.chord.ui.theme.ChordType
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
fun MainScreen(onSignedOut: () -> Unit, openPeer: String?, modifier: Modifier = Modifier) {
    val session = (LocalContext.current.applicationContext as ChordApp).session
    val scope = rememberCoroutineScope()

    var selectedJid by rememberSaveable { mutableStateOf(openPeer.orEmpty()) }
    var selectedName by rememberSaveable { mutableStateOf(openPeer?.let(::bareJid)?.substringBefore('@').orEmpty()) }
    var scopeKey by rememberSaveable { mutableStateOf("") }
    val drawer = rememberDualDrawerState(if (selectedJid.isEmpty()) DrawerPane.Left else DrawerPane.Center)
    val channelScope = remember(scopeKey) { scopeFromString(scopeKey) }

    LaunchedEffect(openPeer) {
        if (openPeer != null && openPeer != selectedJid) {
            selectedJid = openPeer
            selectedName = bareJid(openPeer).substringBefore('@')
            drawer.close()
        }
    }

    val account = remember(session) { session.client.value?.let { AccountUi(it.account()) } }

    DualDrawer(
        state = drawer,
        modifier = modifier,
        left = {
            ChannelDrawer(
                scope = channelScope,
                onScope = { scopeKey = scopeToString(it) },
                selectedJid = selectedJid,
                onSelect = { ch ->
                    selectedJid = ch.jid
                    selectedName = ch.name.ifBlank { bareJid(ch.jid) }
                    scope.launch { drawer.close() }
                },
                account = account,
                onSignOut = { scope.launch { session.signOut(); onSignedOut() } },
            )
        },
        right = {
            if (selectedJid.isNotEmpty()) MemberDrawer(memberRoomFor(selectedJid), selectedName)
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
                )
            }
        }
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

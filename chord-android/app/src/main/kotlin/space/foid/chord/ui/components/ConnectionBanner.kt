package space.foid.chord.ui.components

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.tween
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.compositeOver
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import space.foid.chord.ChordApp
import space.foid.chord.R
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordEase
import space.foid.chord.ui.theme.ChordMotion
import space.foid.chord.ui.theme.ChordSpace
import space.foid.chord.ui.theme.ChordType
import uniffi.chord_ffi.ConnectionState

/** What the banner says. There is no state for "connected": then no banner shows. */
enum class ConnectionNotice {
    /** The session is connecting. */
    Connecting,

    /** The connection is lost and the session reconnects by itself. */
    Offline,

    /** The server rejected the saved credentials. The user must sign in again. */
    SignedOut,
}

/** The notice for a connection state, or null when there is nothing to say. */
fun ConnectionState.notice(): ConnectionNotice? = when (this) {
    is ConnectionState.Connecting -> ConnectionNotice.Connecting
    is ConnectionState.Suspended -> ConnectionNotice.Offline
    is ConnectionState.AuthFailed -> ConnectionNotice.SignedOut
    // Connected: all is well. LoginFailed and Disconnected belong to the sign-in screen.
    else -> null
}

/** What a tap on the banner does. Set by the nav host: sign out and show the sign-in screen. */
val LocalSignInAgain = compositionLocalOf<() -> Unit> { {} }

private val NoConnection: StateFlow<ConnectionState> = MutableStateFlow(ConnectionState.Disconnected)

/** The connection state of the session of the app. A plain Application (JVM tests) has none. */
@Composable
fun rememberConnectionState(): ConnectionState {
    val app = LocalContext.current.applicationContext as? ChordApp
    val flow = remember(app) { app?.session?.connection ?: NoConnection }
    val state by flow.collectAsState()
    return state
}

/**
 * The thin banner under the top bar. It reads the connection of the session and slides in and out.
 * Nothing shows while connected. A tap with [ConnectionNotice.SignedOut] goes to sign-in.
 */
@Composable
fun ConnectionBanner(modifier: Modifier = Modifier) {
    val notice = rememberConnectionState().notice()
    ConnectionBannerContent(notice, onSignIn = LocalSignInAgain.current, modifier = modifier)
}

/** The stateless banner. [notice] null hides it. */
@Composable
fun ConnectionBannerContent(
    notice: ConnectionNotice?,
    onSignIn: () -> Unit,
    modifier: Modifier = Modifier,
) {
    // The last notice stays while the banner slides out, so the text does not vanish first.
    var last by remember { mutableStateOf(notice) }
    if (notice != null) last = notice
    val shown = notice ?: last
    AnimatedVisibility(
        visible = notice != null,
        modifier = modifier,
        enter = expandVertically(tween(ChordMotion.ARRIVE, easing = ChordEase)) + fadeIn(tween(ChordMotion.ARRIVE, easing = ChordEase)),
        exit = shrinkVertically(tween(ChordMotion.FAST, easing = ChordEase)) + fadeOut(tween(ChordMotion.FAST, easing = ChordEase)),
    ) {
        if (shown != null) BannerRow(shown, onSignIn)
    }
}

@Composable
private fun BannerRow(notice: ConnectionNotice, onSignIn: () -> Unit) {
    val c = Chord.colors
    val (bg, fg, dot) = when (notice) {
        ConnectionNotice.Connecting -> Triple(c.surface300, c.ink, c.inkMuted)
        ConnectionNotice.Offline -> Triple(c.away.copy(alpha = 0.18f).over(c.surface100), c.ink, c.away)
        ConnectionNotice.SignedOut -> Triple(c.danger.copy(alpha = 0.18f).over(c.surface100), c.ink, c.danger)
    }
    val text = stringResource(
        when (notice) {
            ConnectionNotice.Connecting -> R.string.connection_connecting
            ConnectionNotice.Offline -> R.string.connection_offline
            ConnectionNotice.SignedOut -> R.string.connection_signed_out
        },
    )
    val tap = if (notice == ConnectionNotice.SignedOut) Modifier.clickable(role = Role.Button, onClick = onSignIn) else Modifier
    Row(
        Modifier
            .fillMaxWidth()
            .background(bg)
            .then(tap)
            .heightIn(min = 28.dp)
            .padding(horizontal = ChordSpace.s4, vertical = ChordSpace.s1)
            .testTag("connection_banner"),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(ChordSpace.s2),
    ) {
        Box(Modifier.size(8.dp).clip(CircleShape).background(dot))
        Text(text, style = ChordType.caption, color = fg, maxLines = 2, overflow = TextOverflow.Ellipsis)
    }
}

private fun Color.over(back: Color): Color = compositeOver(back)

/** The colour of the small dot on the account avatar. */
@Composable
fun connectionDotColor(notice: ConnectionNotice?): Color {
    val c = Chord.colors
    return when (notice) {
        null -> c.online
        ConnectionNotice.Connecting -> c.inkMuted
        ConnectionNotice.Offline -> c.away
        ConnectionNotice.SignedOut -> c.danger
    }
}

/** The dot at the bottom right of the account avatar. [cut] is the colour behind it. */
@Composable
fun ConnectionDot(notice: ConnectionNotice?, cut: Color, modifier: Modifier = Modifier) {
    val label = when (notice) {
        null -> stringResource(R.string.connection_dot_connected)
        ConnectionNotice.Connecting -> stringResource(R.string.connection_connecting)
        ConnectionNotice.Offline -> stringResource(R.string.connection_offline)
        ConnectionNotice.SignedOut -> stringResource(R.string.connection_signed_out)
    }
    Box(
        modifier
            .size(14.dp)
            .clip(CircleShape)
            .background(cut)
            .padding(2.dp)
            .clip(CircleShape)
            .background(connectionDotColor(notice))
            .semantics { contentDescription = label }
            .testTag("connection_dot"),
    )
}

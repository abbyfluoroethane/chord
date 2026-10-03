package space.foid.chord.ui.components

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import space.foid.chord.R
import uniffi.chord_ffi.Availability

/**
 * The presence shape of our own availability. Away and extended away look the same, and
 * invisible is the grey ring, as on the desktop (Presence.svelte).
 */
fun Availability.toPresence(): Presence = when (this) {
    Availability.AVAILABLE -> Presence.Online
    Availability.AWAY, Availability.EXTENDED_AWAY -> Presence.Away
    Availability.DND -> Presence.Dnd
    Availability.INVISIBLE -> Presence.Offline
}

/**
 * The shape to show next to our own avatar. While the connection is not up the ring shows,
 * so a dead connection does not look like "Available".
 */
fun shownOwnPresence(availability: Availability, connected: Boolean): Presence =
    if (connected) availability.toPresence() else Presence.Offline

/** The words for an availability. */
@Composable
fun availabilityLabel(a: Availability): String = stringResource(
    when (a) {
        Availability.AVAILABLE -> R.string.status_available
        Availability.AWAY, Availability.EXTENDED_AWAY -> R.string.status_away
        Availability.DND -> R.string.status_dnd
        Availability.INVISIBLE -> R.string.status_invisible
    },
)

/**
 * A presence shape on its own, for rows of members, direct chats and the status sheet.
 * It has no cut-out; pass [cut] to ring it with the colour behind it.
 */
@Composable
fun PresenceBadge(
    presence: Presence,
    modifier: Modifier = Modifier,
    size: Dp = 10.dp,
    cut: Color? = null,
    pad: Dp = 0.dp,
) = PresenceMark(presence, modifier, size, cut, pad)

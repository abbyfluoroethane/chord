package space.foid.chord.ui.inbox

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordRadius
import space.foid.chord.ui.theme.ChordSpace
import space.foid.chord.ui.theme.ChordType

/**
 * The host of the notices at the bottom of the main screen. Call
 * `state.showSnackbar(text)` to show one. It uses the Chord tokens, not the Material look.
 */
@Composable
fun NoticeSnackbarHost(state: SnackbarHostState, modifier: Modifier = Modifier) {
    SnackbarHost(state, modifier.navigationBarsPadding()) { data ->
        NoticeSnackbar(data.visuals.message)
    }
}

/** One notice: a raised card with a brand bar on the left. */
@Composable
fun NoticeSnackbar(text: String, modifier: Modifier = Modifier) {
    val c = Chord.colors
    val shape = RoundedCornerShape(ChordRadius.md)
    Row(
        modifier
            .padding(horizontal = ChordSpace.s4, vertical = ChordSpace.s2)
            .fillMaxWidth()
            .clip(shape)
            .background(c.surfaceRaised)
            .border(1.dp, c.lineStrong, shape)
            .height(IntrinsicSize.Min)
            .semantics { liveRegion = LiveRegionMode.Polite }
            .testTag("notice_snackbar"),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.width(4.dp).fillMaxHeight().background(c.brand))
        Text(
            text, style = ChordType.body, color = c.ink, maxLines = 4, overflow = TextOverflow.Ellipsis,
            modifier = Modifier.padding(horizontal = ChordSpace.s3, vertical = ChordSpace.s3),
        )
    }
}

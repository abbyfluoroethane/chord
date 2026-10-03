package space.foid.chord.ui.screens

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import space.foid.chord.data.TimelineTarget

/** Stub. The real timeline screen replaces this file. */
@Composable
fun TimelineScreen(
    target: TimelineTarget,
    title: String,
    onOpenChannels: () -> Unit,
    onOpenMembers: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Box(modifier.fillMaxSize(), contentAlignment = Alignment.Center) { Text(title) }
}

package space.foid.chord.ui.screens

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier

/** Stub. The real MainScreen comes from another branch. */
@Composable
fun MainScreen(onSignedOut: () -> Unit, openPeer: String?) {
    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
        Text("Main (stub)" + (openPeer?.let { ": $it" } ?: ""))
    }
}

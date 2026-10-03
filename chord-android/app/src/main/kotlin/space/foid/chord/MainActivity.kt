package space.foid.chord

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.Text
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import uniffi.chord_ffi.ChordClient

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        // Placeholder: open a client on the private files directory to prove the core loads.
        val db = filesDir.resolve("chord.db").absolutePath
        val account = ChordClient(db, "alice@chord.localhost").use { it.account() }
        setContent {
            Box(Modifier.fillMaxSize().background(Color(0xFF111316)), contentAlignment = Alignment.Center) {
                Text("Chord core loaded: $account", color = Color(0xFFECE8E1))
            }
        }
    }
}

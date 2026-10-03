package space.foid.chord

import android.content.Intent
import android.graphics.Color
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.SystemBarStyle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.core.splashscreen.SplashScreen.Companion.installSplashScreen
import androidx.lifecycle.lifecycleScope
import kotlinx.coroutines.launch
import space.foid.chord.data.logWarn
import space.foid.chord.notify.ChordNotifications
import space.foid.chord.ui.ChordNavHost
import space.foid.chord.ui.settings.PrefsSettingsStore
import space.foid.chord.ui.join.XmppLinkInbox
import space.foid.chord.ui.theme.ChordTheme

/** The one activity. It restores the session behind the system splash, then shows the nav host. */
class MainActivity : ComponentActivity() {
    /** null while the saved session is restored. */
    private var startSignedIn by mutableStateOf<Boolean?>(null)

    /** The peer JID of the notification that opened the app, or null. */
    private var openPeer by mutableStateOf<String?>(null)

    override fun onCreate(savedInstanceState: Bundle?) {
        Startup.mark("activity.onCreate")
        val splash = installSplashScreen()
        super.onCreate(savedInstanceState)
        splash.setKeepOnScreenCondition { startSignedIn == null }
        // Transparent bars. The auto style picks light or dark icons from the system theme.
        enableEdgeToEdge(
            statusBarStyle = SystemBarStyle.auto(Color.TRANSPARENT, Color.TRANSPARENT),
            navigationBarStyle = SystemBarStyle.auto(Color.TRANSPARENT, Color.TRANSPARENT),
        )
        openPeer = peerOf(intent)
        // A link from outside the app. After a rotation the intent is the same one: skip it.
        if (savedInstanceState == null) xmppUriOf(intent)?.let(XmppLinkInbox::offer)

        // The saved account opens offline (ChordApp.startup). The login runs in the background.
        val app = application as ChordApp
        lifecycleScope.launch {
            startSignedIn = try {
                // The start-up result is from the process start. After a sign-in in this process,
                // a new activity must still open the main screen, so ask the session too.
                app.startup.await() || app.session.client.value != null
            } catch (e: Exception) {
                logWarn("MainActivity", "restore failed", e)
                false
            }
            Startup.mark("activity.startSignedIn=$startSignedIn")
        }

        val settings = PrefsSettingsStore.get(this)
        setContent {
            val mode by settings.themeMode.collectAsState()
            val dark = mode.isDark(isSystemInDarkTheme())
            // The bar icons follow the chosen theme, not only the system one.
            DisposableEffect(dark) {
                enableEdgeToEdge(
                    statusBarStyle = SystemBarStyle.auto(Color.TRANSPARENT, Color.TRANSPARENT) { dark },
                    navigationBarStyle = SystemBarStyle.auto(Color.TRANSPARENT, Color.TRANSPARENT) { dark },
                )
                onDispose {}
            }
            ChordTheme(dark = dark) {
                Box(Modifier.fillMaxSize().semantics { testTagsAsResourceId = true }) {
                    startSignedIn?.let { signedIn ->
                        ChordNavHost(startSignedIn = signedIn, openPeer = openPeer)
                        // The splash ends with the first frame of real content.
                        LaunchedEffect(Unit) {
                            Startup.mark("content.firstFrame")
                            reportFullyDrawn()
                        }
                    }
                }
            }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        peerOf(intent)?.let { openPeer = it }
        xmppUriOf(intent)?.let(XmppLinkInbox::offer)
    }

    /** The `xmpp:` URI of a VIEW intent, or null. The main screen decides what it means. */
    private fun xmppUriOf(intent: Intent?): String? =
        intent?.takeIf { it.action == Intent.ACTION_VIEW }?.data?.takeIf { it.scheme.equals("xmpp", ignoreCase = true) }?.toString()

    private fun peerOf(intent: Intent?): String? = intent?.getStringExtra(ChordNotifications.EXTRA_PEER)
}

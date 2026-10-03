package space.foid.chord.ui.join

import androidx.compose.runtime.compositionLocalOf
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow

/**
 * Opens an `xmpp:` URI: `?join` fills in the join form, a plain address the "Message someone"
 * form. The main screen provides it. A link in a message calls it. Outside the main screen it
 * does nothing.
 *
 * Usage in any composable below the main screen: `LocalOpenXmppUri.current(uri)`.
 */
val LocalOpenXmppUri = compositionLocalOf<(String) -> Unit> { {} }

/**
 * Where a link from outside the app waits until the main screen is up (the app may still restore
 * its session). MainActivity calls [offer]. The main screen reads [pending] and calls [consume].
 */
object XmppLinkInbox {
    private val _pending = MutableStateFlow<String?>(null)
    val pending: StateFlow<String?> = _pending

    /** A link arrived. A newer link replaces an older one that nobody read. */
    fun offer(uri: String) {
        _pending.value = uri
    }

    /** The main screen took [uri]. */
    fun consume(uri: String) {
        _pending.compareAndSet(uri, null)
    }
}

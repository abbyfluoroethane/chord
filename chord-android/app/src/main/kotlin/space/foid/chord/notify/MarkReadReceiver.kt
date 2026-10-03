package space.foid.chord.notify

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import space.foid.chord.ChordApp

/** The "Mark as read" action of a message notification. */
class MarkReadReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val peer = intent.getStringExtra(ChordNotifications.EXTRA_PEER) ?: return
        ChordNotifications.cancel(context, peer)
        val client = (context.applicationContext as ChordApp).session.client.value ?: return
        val pending = goAsync()
        CoroutineScope(Dispatchers.IO).launch {
            try {
                client.markRead(peer)
            } catch (_: Exception) {
                // The chat stays unread on the server. The UI marks it when opened.
            } finally {
                pending.finish()
            }
        }
    }
}

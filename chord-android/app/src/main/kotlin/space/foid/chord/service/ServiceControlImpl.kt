package space.foid.chord.service

import android.content.Context
import android.content.Intent
import androidx.core.content.ContextCompat
import space.foid.chord.data.ServiceControl

class ServiceControlImpl(context: Context) : ServiceControl {
    private val context = context.applicationContext

    override fun start() {
        ContextCompat.startForegroundService(context, Intent(context, ChordConnectionService::class.java))
    }

    override fun stop() {
        context.stopService(Intent(context, ChordConnectionService::class.java))
    }
}

package space.foid.chord.update

import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageInstaller
import android.net.Uri
import android.os.Build
import android.provider.Settings
import androidx.core.content.IntentCompat
import space.foid.chord.data.logWarn
import java.io.File

/** Installs a downloaded APK through a [PackageInstaller] session. */
object ApkInstaller {
    const val ACTION_INSTALL_STATUS = "space.foid.chord.action.INSTALL_STATUS"

    /** Whether the user lets this app install apps. Without it, send them to [unknownSourcesIntent]. */
    fun canInstall(context: Context): Boolean = context.packageManager.canRequestPackageInstalls()

    /** The system page where the user lets this app install apps. */
    fun unknownSourcesIntent(context: Context): Intent =
        Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:${context.packageName}"))

    /** Writes [apk] to a new session and commits it. The result comes to [InstallReceiver]. */
    fun install(context: Context, apk: File) {
        val installer = context.packageManager.packageInstaller
        val params = PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL).apply {
            setAppPackageName(context.packageName)
            setSize(apk.length())
        }
        val id = installer.createSession(params)
        try {
            installer.openSession(id).use { session ->
                session.openWrite("chord.apk", 0, apk.length()).use { out ->
                    apk.inputStream().use { it.copyTo(out) }
                    session.fsync(out)
                }
                // The installer fills in the status extras, so the intent must be mutable.
                val flags = PendingIntent.FLAG_UPDATE_CURRENT or
                    (if (Build.VERSION.SDK_INT >= 31) PendingIntent.FLAG_MUTABLE else 0)
                val callback = PendingIntent.getBroadcast(
                    context,
                    id,
                    Intent(context, InstallReceiver::class.java).setAction(ACTION_INSTALL_STATUS).setPackage(context.packageName),
                    flags,
                )
                session.commit(callback.intentSender)
            }
        } catch (e: Exception) {
            installer.abandonSession(id)
            throw e
        }
    }
}

/** Gets the result of an install session. A confirm step opens the system dialog. */
class InstallReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action != ApkInstaller.ACTION_INSTALL_STATUS) return
        val status = intent.getIntExtra(PackageInstaller.EXTRA_STATUS, PackageInstaller.STATUS_FAILURE)
        val message = intent.getStringExtra(PackageInstaller.EXTRA_STATUS_MESSAGE)
        val updater = Updater.get(context)
        when (status) {
            PackageInstaller.STATUS_PENDING_USER_ACTION -> {
                val confirm = IntentCompat.getParcelableExtra(intent, Intent.EXTRA_INTENT, Intent::class.java)
                if (confirm == null) {
                    updater.onInstallResult(PackageInstaller.STATUS_FAILURE, "no confirm intent")
                    return
                }
                try {
                    context.startActivity(confirm.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
                } catch (e: Exception) {
                    logWarn("InstallReceiver", "could not open the install dialog", e)
                    updater.onInstallResult(PackageInstaller.STATUS_FAILURE, e.message)
                }
            }
            else -> updater.onInstallResult(status, message)
        }
    }
}

package space.foid.chord.update

import android.content.Context
import android.content.pm.PackageInstaller
import android.os.Build
import androidx.core.content.pm.PackageInfoCompat
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import space.foid.chord.BuildConfig
import space.foid.chord.data.logWarn
import space.foid.chord.ui.settings.PrefsSettingsStore
import java.io.IOException

/** What the updater is doing. The About page shows it. */
sealed interface UpdateStatus {
    data object Idle : UpdateStatus
    data object Checking : UpdateStatus

    /** The channel has nothing newer. [version] is its newest build. */
    data class UpToDate(val version: String) : UpdateStatus

    /** The channel has no build yet. */
    data object NoBuild : UpdateStatus

    data class Available(val manifest: UpdateManifest, val abi: String, val apk: ApkFile) : UpdateStatus

    /** [total] is -1 when unknown. */
    data class Downloading(val available: Available, val done: Long, val total: Long) : UpdateStatus {
        val fraction: Float? get() = if (total > 0) (done.toFloat() / total).coerceIn(0f, 1f) else null
    }

    data class Installing(val available: Available) : UpdateStatus

    /** [available] is set when the step that failed was the download or the install: Retry installs again. */
    data class Failed(val reason: UpdateFailure, val available: Available? = null) : UpdateStatus
}

fun CheckResult.toStatus(): UpdateStatus = when (this) {
    CheckResult.NoBuild -> UpdateStatus.NoBuild
    is CheckResult.UpToDate -> UpdateStatus.UpToDate(manifest.version)
    is CheckResult.Available -> UpdateStatus.Available(manifest, abi, apk)
    is CheckResult.Failed -> UpdateStatus.Failed(reason)
}

/**
 * The in-app updater: checks the chosen channel, downloads and installs. One per process: use
 * [get]. It does nothing when the build does not update itself ([enabled] is false).
 */
class Updater private constructor(private val app: Context) {
    val enabled: Boolean = updaterEnabled(BuildConfig.UPDATER, BuildConfig.CHANNEL)

    private val store = PrefsSettingsStore.get(app)
    private val memory = app.getSharedPreferences("chord_updates", Context.MODE_PRIVATE)
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val downloader = ApkDownloader(app.cacheDir)

    private val _status = MutableStateFlow<UpdateStatus>(UpdateStatus.Idle)
    val status: StateFlow<UpdateStatus> = _status.asStateFlow()

    private val _lastChecked = MutableStateFlow(memory.getLong(LAST_CHECKED, 0L))

    /** When the last check that reached the server ended, in ms. 0: never. */
    val lastChecked: StateFlow<Long> = _lastChecked.asStateFlow()

    /** The newest build that a notification announced. */
    val lastNotified: Long get() = memory.getLong(LAST_NOTIFIED, 0L)

    private var job: Job? = null

    /** A change of channel or a new step makes older results stale. */
    @Volatile private var generation = 0

    /** The channel to follow. */
    val channel: UpdateChannel get() = effectiveChannel(store.prefs.value.updateChannel, BuildConfig.CHANNEL)

    private fun installedBuild(): Long = try {
        PackageInfoCompat.getLongVersionCode(app.packageManager.getPackageInfo(app.packageName, 0))
    } catch (e: Exception) {
        BuildConfig.VERSION_CODE.toLong()
    }

    /**
     * Checks the channel now, on this thread, and shows the result unless a download or an
     * install runs. The daily worker calls it.
     */
    fun checkBlocking(): CheckResult {
        val gen = generation
        val result = UpdateChecker(HttpManifestFetcher, installedBuild(), Build.SUPPORTED_ABIS.toList()).check(channel)
        if (result !is CheckResult.Failed) {
            val now = System.currentTimeMillis()
            memory.edit().putLong(LAST_CHECKED, now).apply()
            _lastChecked.value = now
        }
        if (gen == generation) {
            _status.update { cur ->
                if (cur is UpdateStatus.Downloading || cur is UpdateStatus.Installing) cur else result.toStatus()
            }
        }
        return result
    }

    /** Checks in the background. Does nothing while a check, download or install runs. */
    fun check() {
        if (!enabled || job?.isActive == true) return
        _status.value = UpdateStatus.Checking
        job = scope.launch { checkBlocking() }
    }

    /** The user picked another channel: forget the old result and check the new channel. */
    fun onChannelChanged() {
        if (!enabled) return
        generation++
        job?.cancel()
        job = null
        _status.value = UpdateStatus.Idle
        check()
    }

    /** Downloads and installs the update that the status shows. The caller checked [ApkInstaller.canInstall]. */
    fun install() {
        val a = when (val s = _status.value) {
            is UpdateStatus.Available -> s
            is UpdateStatus.Failed -> s.available
            else -> null
        } ?: return
        if (job?.isActive == true) return
        val gen = ++generation
        _status.value = UpdateStatus.Downloading(a, 0, a.apk.size)
        job = scope.launch {
            var lastPercent = -1
            val file = try {
                downloader.download(a.manifest.build, a.abi, a.apk) { done, total ->
                    val percent = if (total > 0) (done * 100 / total).toInt() else (done shr 20).toInt()
                    if (percent != lastPercent && gen == generation) {
                        lastPercent = percent
                        _status.value = UpdateStatus.Downloading(a, done, total)
                    }
                }
            } catch (e: ChecksumException) {
                if (gen == generation) _status.value = UpdateStatus.Failed(UpdateFailure.Checksum, a)
                return@launch
            } catch (e: IOException) {
                logWarn("Updater", "download failed", e)
                if (gen == generation) _status.value = UpdateStatus.Failed(UpdateFailure.Network, a)
                return@launch
            }
            if (gen != generation) return@launch
            _status.value = UpdateStatus.Installing(a)
            try {
                ApkInstaller.install(app, file)
            } catch (e: Exception) {
                logWarn("Updater", "install session failed", e)
                if (gen == generation) _status.value = UpdateStatus.Failed(UpdateFailure.Install, a)
            }
        }
    }

    /** Retry the step that failed: the install when there was an update, else the check. */
    fun retry() {
        val s = _status.value as? UpdateStatus.Failed ?: return
        if (s.available != null) install() else check()
    }

    /** The result of the install session, from [InstallReceiver]. */
    fun onInstallResult(status: Int, message: String?) {
        val a = (_status.value as? UpdateStatus.Installing)?.available
        when (status) {
            PackageInstaller.STATUS_SUCCESS -> {
                downloader.clear()
                _status.value = UpdateStatus.UpToDate(a?.manifest?.version.orEmpty())
            }
            // The user said no in the system dialog: offer the update again.
            PackageInstaller.STATUS_FAILURE_ABORTED -> _status.value = a ?: UpdateStatus.Idle
            else -> {
                logWarn("Updater", "install failed: $status $message")
                _status.value = UpdateStatus.Failed(UpdateFailure.Install, a)
            }
        }
    }

    fun markNotified(build: Long) {
        memory.edit().putLong(LAST_NOTIFIED, build).apply()
    }

    companion object {
        private const val LAST_CHECKED = "last_checked"
        private const val LAST_NOTIFIED = "last_notified"

        @Volatile private var instance: Updater? = null

        fun get(context: Context): Updater = instance ?: synchronized(this) {
            instance ?: Updater(context.applicationContext).also { instance = it }
        }
    }
}

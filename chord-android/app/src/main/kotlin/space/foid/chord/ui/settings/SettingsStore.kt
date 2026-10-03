package space.foid.chord.ui.settings

import android.content.Context
import android.content.SharedPreferences
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/** The theme the user picked. [System] follows the system setting. */
enum class ThemeMode {
    System, Dark, Light;

    /** Whether the dark theme shows, given the system setting. */
    fun isDark(systemDark: Boolean): Boolean = when (this) {
        System -> systemDark
        Dark -> true
        Light -> false
    }

    companion object {
        fun fromName(name: String?): ThemeMode = entries.firstOrNull { it.name == name } ?: System
    }
}

/** The availability to set at sign-in. [Last] keeps the one from the last session. */
enum class SignInShow {
    Last, Chat, Away, Dnd;

    companion object {
        fun fromName(name: String?): SignInShow = entries.firstOrNull { it.name == name } ?: Last
    }
}

/** The longest status text, as on the desktop. */
const val MAX_STATUS = 128

/** The waits that the idle setting accepts, as on the desktop. */
val IDLE_MINUTES = listOf(5, 10, 30)

/**
 * Every setting that lives on the phone only, not on the account. The defaults are the ones of
 * the desktop. Times are minutes after midnight.
 */
data class AppPrefs(
    val theme: ThemeMode = ThemeMode.System,
    /** Answer version and time queries. */
    val shareInfo: Boolean = true,
    val signInShow: SignInShow = SignInShow.Last,
    val signInStatus: String = "",
    /** Show the presence mark on avatars. */
    val showPresence: Boolean = true,
    /** Tell the contacts when the app has been in the background for [idleMinutes]. */
    val shareIdle: Boolean = true,
    val idleMinutes: Int = 5,
    /** Show the text of a message in its notice. */
    val noticePreview: Boolean = true,
    val quietHours: Boolean = false,
    val quietFrom: Int = 22 * 60,
    val quietTo: Int = 8 * 60,
)

/**
 * The settings that live on the phone. [prefs] holds all of them. [themeMode], [shareInfo] and
 * [showPresence] are flows of one value each, so that other screens can read them: for example
 * `PrefsSettingsStore.get(context).showPresence`.
 */
interface SettingsStore {
    val prefs: StateFlow<AppPrefs>
    val themeMode: StateFlow<ThemeMode>
    val shareInfo: StateFlow<Boolean>

    /** True: show the presence mark on avatars. */
    val showPresence: StateFlow<Boolean>

    /** Change the settings. [change] gets the current ones and returns the new ones. */
    fun update(change: (AppPrefs) -> AppPrefs)

    /** Put every setting back to its default. */
    fun reset() = update { AppPrefs() }
}

/** [SettingsStore] that keeps the values in memory. A subclass saves them in [persist]. */
abstract class MemorySettingsStore(initial: AppPrefs = AppPrefs()) : SettingsStore {
    private val _prefs = MutableStateFlow(initial)
    private val _theme = MutableStateFlow(initial.theme)
    private val _share = MutableStateFlow(initial.shareInfo)
    private val _presence = MutableStateFlow(initial.showPresence)

    override val prefs: StateFlow<AppPrefs> = _prefs.asStateFlow()
    override val themeMode: StateFlow<ThemeMode> = _theme.asStateFlow()
    override val shareInfo: StateFlow<Boolean> = _share.asStateFlow()
    override val showPresence: StateFlow<Boolean> = _presence.asStateFlow()

    @Synchronized
    override fun update(change: (AppPrefs) -> AppPrefs) {
        val next = change(_prefs.value).sanitised()
        if (next == _prefs.value) return
        persist(next)
        _prefs.value = next
        _theme.value = next.theme
        _share.value = next.shareInfo
        _presence.value = next.showPresence
    }

    protected abstract fun persist(prefs: AppPrefs)
}

/** Keep each value in its range. */
internal fun AppPrefs.sanitised(): AppPrefs = copy(
    signInStatus = signInStatus.take(MAX_STATUS),
    idleMinutes = if (idleMinutes in IDLE_MINUTES) idleMinutes else 5,
    quietFrom = quietFrom.coerceIn(0, 24 * 60 - 1),
    quietTo = quietTo.coerceIn(0, 24 * 60 - 1),
)

/** [SettingsStore] in SharedPreferences. One instance per process: use [get]. */
class PrefsSettingsStore private constructor(private val sp: SharedPreferences) :
    MemorySettingsStore(read(sp)) {

    override fun persist(prefs: AppPrefs) {
        sp.edit()
            .putString(THEME, prefs.theme.name)
            .putBoolean(SHARE_INFO, prefs.shareInfo)
            .putString(SIGN_IN_SHOW, prefs.signInShow.name)
            .putString(SIGN_IN_STATUS, prefs.signInStatus)
            .putBoolean(SHOW_PRESENCE, prefs.showPresence)
            .putBoolean(SHARE_IDLE, prefs.shareIdle)
            .putInt(IDLE_MINUTES_KEY, prefs.idleMinutes)
            .putBoolean(NOTICE_PREVIEW, prefs.noticePreview)
            .putBoolean(QUIET, prefs.quietHours)
            .putInt(QUIET_FROM, prefs.quietFrom)
            .putInt(QUIET_TO, prefs.quietTo)
            .apply()
    }

    companion object {
        private const val THEME = "theme"
        private const val SHARE_INFO = "share_info"
        private const val SIGN_IN_SHOW = "sign_in_show"
        private const val SIGN_IN_STATUS = "sign_in_status"
        private const val SHOW_PRESENCE = "show_presence"
        private const val SHARE_IDLE = "share_idle"
        private const val IDLE_MINUTES_KEY = "idle_minutes"
        private const val NOTICE_PREVIEW = "notice_preview"
        private const val QUIET = "quiet_hours"
        private const val QUIET_FROM = "quiet_from"
        private const val QUIET_TO = "quiet_to"

        private fun read(sp: SharedPreferences): AppPrefs {
            val d = AppPrefs()
            return AppPrefs(
                theme = ThemeMode.fromName(sp.getString(THEME, null)),
                shareInfo = sp.getBoolean(SHARE_INFO, d.shareInfo),
                signInShow = SignInShow.fromName(sp.getString(SIGN_IN_SHOW, null)),
                signInStatus = sp.getString(SIGN_IN_STATUS, "").orEmpty(),
                showPresence = sp.getBoolean(SHOW_PRESENCE, d.showPresence),
                shareIdle = sp.getBoolean(SHARE_IDLE, d.shareIdle),
                idleMinutes = sp.getInt(IDLE_MINUTES_KEY, d.idleMinutes),
                noticePreview = sp.getBoolean(NOTICE_PREVIEW, d.noticePreview),
                quietHours = sp.getBoolean(QUIET, d.quietHours),
                quietFrom = sp.getInt(QUIET_FROM, d.quietFrom),
                quietTo = sp.getInt(QUIET_TO, d.quietTo),
            ).sanitised()
        }

        @Volatile private var instance: PrefsSettingsStore? = null

        fun get(context: Context): PrefsSettingsStore = instance ?: synchronized(this) {
            instance ?: PrefsSettingsStore(
                context.applicationContext.getSharedPreferences("chord_settings", Context.MODE_PRIVATE),
            ).also { instance = it }
        }
    }
}

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

/** The settings that live on the phone only, not on the account. */
interface SettingsStore {
    val themeMode: StateFlow<ThemeMode>
    fun setThemeMode(mode: ThemeMode)

    /** Answer version and time queries. On by default, as on the desktop. */
    val shareInfo: StateFlow<Boolean>
    fun setShareInfo(share: Boolean)
}

/** [SettingsStore] in SharedPreferences. One instance per process: use [get]. */
class PrefsSettingsStore private constructor(private val prefs: SharedPreferences) : SettingsStore {
    private val _theme = MutableStateFlow(ThemeMode.fromName(prefs.getString(THEME, null)))
    private val _share = MutableStateFlow(prefs.getBoolean(SHARE_INFO, true))

    override val themeMode: StateFlow<ThemeMode> = _theme.asStateFlow()
    override val shareInfo: StateFlow<Boolean> = _share.asStateFlow()

    override fun setThemeMode(mode: ThemeMode) {
        prefs.edit().putString(THEME, mode.name).apply()
        _theme.value = mode
    }

    override fun setShareInfo(share: Boolean) {
        prefs.edit().putBoolean(SHARE_INFO, share).apply()
        _share.value = share
    }

    companion object {
        private const val THEME = "theme"
        private const val SHARE_INFO = "share_info"

        @Volatile private var instance: PrefsSettingsStore? = null

        fun get(context: Context): PrefsSettingsStore = instance ?: synchronized(this) {
            instance ?: PrefsSettingsStore(
                context.applicationContext.getSharedPreferences("chord_settings", Context.MODE_PRIVATE),
            ).also { instance = it }
        }
    }
}

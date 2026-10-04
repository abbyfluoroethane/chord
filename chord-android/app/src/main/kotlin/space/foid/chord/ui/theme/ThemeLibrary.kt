package space.foid.chord.ui.theme

import android.content.Context
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject
import space.foid.chord.R
import java.net.HttpURLConnection
import java.net.URI
import java.net.URL

/** A theme in the library: a bundled one or one the user imported. */
class ThemeEntry(
    val id: String,
    val css: String,
    val builtIn: Boolean,
    /** The link of a theme that came from a link. */
    val url: String? = null,
) {
    val info: ThemeInfo = parseTheme(css)
}

/** An imported theme as stored. */
data class CustomTheme(val id: String, val css: String, val url: String? = null)

const val DEFAULT_DARK_THEME = "chord-dark"
const val DEFAULT_LIGHT_THEME = "chord-light"

/** The theme to show for a mode. A missing theme, or one of the other mode, falls back to Chord. */
fun resolveTheme(library: List<ThemeEntry>, id: String, dark: Boolean): ThemeEntry =
    library.firstOrNull { it.id == id && it.info.dark == dark }
        ?: library.first { it.id == if (dark) DEFAULT_DARK_THEME else DEFAULT_LIGHT_THEME }

/** The result of an import. */
sealed interface ImportResult {
    data class Added(val entry: ThemeEntry) : ImportResult
    data class Failed(val error: ImportError) : ImportResult
}

enum class ImportError { Empty, TooBig, NoColours, BadLink, NotHttps, Fetch }

/** Checks pasted or fetched CSS. Returns the trimmed CSS, or the problem. */
fun checkThemeCss(css: String): Pair<String?, ImportError?> {
    val text = css.trim()
    if (text.isEmpty()) return null to ImportError.Empty
    if (text.length > MAX_THEME_CHARS) return null to ImportError.TooBig
    val info = parseTheme(text)
    val known = listOf("--surface-100", "--ink", "--brand").any { it in info.base }
    if (!known) return null to ImportError.NoColours
    return text to null
}

/** Checks the link of a theme: https only. A GitHub page link becomes the raw link of the file. */
fun normalizeThemeUrl(input: String): Pair<String?, ImportError?> {
    val text = input.trim()
    if (text.isEmpty()) return null to ImportError.BadLink
    val uri = try { URI(text) } catch (_: Exception) { return null to ImportError.BadLink }
    if (uri.scheme == null || uri.host == null) return null to ImportError.BadLink
    if (!uri.scheme.equals("https", ignoreCase = true)) return null to ImportError.NotHttps
    if (uri.userInfo != null) return null to ImportError.BadLink
    val host = uri.host.lowercase()
    if (host == "github.com" || host == "www.github.com") {
        val m = Regex("^/([^/]+)/([^/]+)/(?:blob|raw)/(.+)$").find(uri.path.orEmpty())
        if (m != null) {
            return "https://raw.githubusercontent.com/${m.groupValues[1]}/${m.groupValues[2]}/${m.groupValues[3]}" to null
        }
    }
    return URI(uri.scheme, null, uri.host, uri.port, uri.path, uri.query, null).toString() to null
}

/** The host of a link, for a theme card. */
fun hostOf(url: String): String = try { URI(url).host.removePrefix("www.") } catch (_: Exception) { url }

/** Bundled and imported themes. [persist] saves the imported ones. */
class ThemeLibrary(
    private val bundled: List<ThemeEntry>,
    initial: List<CustomTheme> = emptyList(),
    private val persist: (List<CustomTheme>) -> Unit = {},
    private val newId: () -> String = { "custom-" + java.lang.Long.toString(System.currentTimeMillis(), 36) },
) {
    private val custom = MutableStateFlow(initial)
    private val _entries = MutableStateFlow(bundled + initial.map(::entryOf))
    val entries: StateFlow<List<ThemeEntry>> = _entries.asStateFlow()

    private fun entryOf(t: CustomTheme) = ThemeEntry(t.id, t.css, builtIn = false, url = t.url)

    private fun publish(list: List<CustomTheme>) {
        custom.value = list
        _entries.value = bundled + list.map(::entryOf)
        persist(list)
    }

    /** Adds pasted CSS. */
    @Synchronized
    fun import(css: String, url: String? = null): ImportResult {
        val (text, err) = checkThemeCss(css)
        if (text == null) return ImportResult.Failed(err!!)
        val known = url?.let { u -> custom.value.firstOrNull { it.url == u } }
        val id = known?.id ?: newId()
        val item = CustomTheme(id, text, url)
        publish(if (known != null) custom.value.map { if (it.id == id) item else it } else custom.value + item)
        return ImportResult.Added(_entries.value.first { it.id == id })
    }

    /** Removes an imported theme. */
    @Synchronized
    fun remove(id: String) {
        publish(custom.value.filterNot { it.id == id })
    }

    companion object {
        private const val PREFS = "chord_themes"
        private const val KEY = "custom"

        /** The bundled themes, in the order of the desktop. */
        private val BUNDLED = listOf(
            "chord-dark" to R.raw.theme_chord_dark,
            "chord-light" to R.raw.theme_chord_light,
            "catppuccin-mocha" to R.raw.theme_catppuccin_mocha,
            "catppuccin-latte" to R.raw.theme_catppuccin_latte,
        )

        @Volatile private var instance: ThemeLibrary? = null

        fun get(context: Context): ThemeLibrary = instance ?: synchronized(this) {
            instance ?: create(context.applicationContext).also { instance = it }
        }

        private fun create(context: Context): ThemeLibrary {
            val bundled = BUNDLED.map { (id, res) ->
                val css = context.resources.openRawResource(res).bufferedReader().use { it.readText() }
                ThemeEntry(id, css, builtIn = true)
            }
            val sp = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            return ThemeLibrary(
                bundled = bundled,
                initial = decode(sp.getString(KEY, null)),
                persist = { sp.edit().putString(KEY, encode(it)).apply() },
            )
        }

        internal fun encode(list: List<CustomTheme>): String = JSONArray().also { arr ->
            list.forEach {
                arr.put(JSONObject().put("id", it.id).put("css", it.css).apply { it.url?.let { u -> put("url", u) } })
            }
        }.toString()

        internal fun decode(json: String?): List<CustomTheme> {
            if (json.isNullOrEmpty()) return emptyList()
            return try {
                val arr = JSONArray(json)
                (0 until arr.length()).mapNotNull { i ->
                    val o = arr.optJSONObject(i) ?: return@mapNotNull null
                    val id = o.optString("id")
                    val css = o.optString("css")
                    if (id.isEmpty() || css.isEmpty()) null else CustomTheme(id, css, o.optString("url").ifEmpty { null })
                }
            } catch (_: Exception) {
                emptyList()
            }
        }
    }
}

/** Fetches the CSS of a theme link. Runs on the IO dispatcher. Returns the CSS or null. */
suspend fun fetchThemeCss(url: String): String? = withContext(Dispatchers.IO) {
    try {
        val conn = URL(url).openConnection() as HttpURLConnection
        conn.connectTimeout = 10_000
        conn.readTimeout = 15_000
        conn.instanceFollowRedirects = true
        conn.inputStream.use { input ->
            val buf = java.io.ByteArrayOutputStream()
            val chunk = ByteArray(8192)
            while (true) {
                val n = input.read(chunk)
                if (n < 0) break
                buf.write(chunk, 0, n)
                if (buf.size() > MAX_THEME_CHARS * 4) return@withContext null
            }
            buf.toString(Charsets.UTF_8.name())
        }
    } catch (_: Exception) {
        null
    }
}

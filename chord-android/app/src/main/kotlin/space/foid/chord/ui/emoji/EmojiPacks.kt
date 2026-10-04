package space.foid.chord.ui.emoji

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.util.LruCache
import androidx.compose.runtime.Immutable
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import com.caverock.androidsvg.SVG
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.ByteArrayOutputStream
import java.io.File
import java.net.HttpURLConnection
import java.net.URL

/** The emoji images of one installed pack. Drawing an emoji is a lookup, a parse and one bitmap. */
class EmojiImages(val pack: EmojiPack, private val index: PackIndex, private val px: Int = 96) {
    private val cache = object : LruCache<String, ImageBitmap>(400) {}
    private val missing = HashSet<String>()

    /** True if the pack has an image for [emoji]. */
    fun has(emoji: String): Boolean = index.has(emoji)

    /** The bitmap if it is in memory already. */
    fun cached(emoji: String): ImageBitmap? = synchronized(cache) { cache.get(emoji) }

    /** The bitmap of [emoji], or null if the pack has none or the SVG does not draw. */
    suspend fun load(emoji: String): ImageBitmap? {
        cached(emoji)?.let { return it }
        if (synchronized(missing) { emoji in missing }) return null
        val bmp = withContext(Dispatchers.IO) { render(emoji) }
        if (bmp == null) synchronized(missing) { missing += emoji } else synchronized(cache) { cache.put(emoji, bmp) }
        return bmp
    }

    /** The bitmap now, on the calling thread. For tests and previews. */
    fun loadNow(emoji: String): ImageBitmap? = cached(emoji) ?: render(emoji)?.also { synchronized(cache) { cache.put(emoji, it) } }

    private fun render(emoji: String): ImageBitmap? = try {
        val text = index.svg(emoji)
        if (text == null) null else {
            val svg = SVG.getFromString(text)
            svg.setDocumentWidth(px.toFloat())
            svg.setDocumentHeight(px.toFloat())
            val bmp = Bitmap.createBitmap(px, px, Bitmap.Config.ARGB_8888)
            svg.renderToCanvas(Canvas(bmp))
            bmp.asImageBitmap()
        }
    } catch (_: Exception) {
        null
    }
}

/** What the settings page shows for each pack. */
@Immutable
data class EmojiPackStatus(
    val installed: Set<EmojiPack> = setOf(EmojiPack.System),
    /** The pack that downloads now, with the progress from 0 to 1, or null if it is not known. */
    val installing: EmojiPack? = null,
    val progress: Float? = null,
    /** The pack whose last download failed. */
    val failed: EmojiPack? = null,
)

/**
 * Downloads, keeps and loads the packs. One per process: use [get]. The packs live in
 * `filesDir/emoji/<pack>`.
 */
class EmojiPacks private constructor(private val root: File) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    private val _status = MutableStateFlow(EmojiPackStatus())
    val status: StateFlow<EmojiPackStatus> = _status.asStateFlow()

    private val _active = MutableStateFlow<EmojiImages?>(null)

    /** The images that draw now: the chosen pack when it is on the phone, else null (system font). */
    val active: StateFlow<EmojiImages?> = _active.asStateFlow()

    init {
        refreshInstalled()
    }

    private fun dirOf(pack: EmojiPack) = File(root, pack.dirName)

    private fun refreshInstalled() {
        val on = EmojiPack.entries.filter { it == EmojiPack.System || PackIndex.isInstalled(dirOf(it)) }.toSet()
        _status.update { it.copy(installed = on) }
    }

    /** Draw with [pack]. A pack that is not installed gives null, so the system font draws. */
    fun activate(pack: EmojiPack) {
        if (pack == EmojiPack.System || pack !in _status.value.installed) {
            _active.value = null
            return
        }
        if (_active.value?.pack == pack) return
        scope.launch { _active.value = imagesFor(pack) }
    }

    private val loaded = HashMap<EmojiPack, EmojiImages>()

    /** The images of an installed pack, or null. The index loads once. */
    suspend fun imagesFor(pack: EmojiPack): EmojiImages? {
        if (pack == EmojiPack.System || pack !in _status.value.installed) return null
        synchronized(loaded) { loaded[pack] }?.let { return it }
        return withContext(Dispatchers.IO) {
            try { EmojiImages(pack, PackIndex(dirOf(pack))).also { synchronized(loaded) { loaded[pack] = it } } } catch (_: Exception) { null }
        }
    }

    /**
     * Makes [pack] ready, then calls [onReady] on a background thread. Does nothing if a download
     * runs. A failure sets [EmojiPackStatus.failed].
     */
    fun install(pack: EmojiPack, onReady: () -> Unit) {
        if (pack == EmojiPack.System) { onReady(); return }
        if (pack in _status.value.installed) { onReady(); return }
        if (_status.value.installing != null) return
        _status.update { it.copy(installing = pack, progress = null, failed = null) }
        scope.launch {
            val ok = try {
                download(pack)
                true
            } catch (e: Exception) {
                android.util.Log.w("EmojiPacks", "install ${pack.name} failed", e)
                false
            }
            refreshInstalled()
            _status.update { it.copy(installing = null, progress = null, failed = if (ok) null else pack) }
            if (ok) onReady()
        }
    }

    private suspend fun download(pack: EmojiPack) {
        // Fluent has no code point map. It takes the names of Twemoji.
        val twemoji = if (pack == EmojiPack.Fluent) {
            if (!PackIndex.isInstalled(dirOf(EmojiPack.Twemoji))) fetchAndInstall(EmojiPack.Twemoji, null, report = false)
            PackIndex.readChars(dirOf(EmojiPack.Twemoji))
        } else null
        fetchAndInstall(pack, twemoji, report = true)
    }

    private suspend fun fetchAndInstall(pack: EmojiPack, twemoji: Map<String, String>?, report: Boolean) {
        val bytes = withContext(Dispatchers.IO) { fetch(pack.url!!, report) }
        if (!verifyIntegrity(bytes, pack.integrity!!)) throw PackException("the pack does not match its checksum")
        withContext(Dispatchers.IO) {
            root.mkdirs()
            installPack(bytes.inputStream(), dirOf(pack), twemoji)
        }
    }

    private fun fetch(url: String, report: Boolean): ByteArray {
        val conn = URL(url).openConnection() as HttpURLConnection
        conn.connectTimeout = 15_000
        conn.readTimeout = 30_000
        conn.instanceFollowRedirects = true
        if (conn.responseCode != 200) throw PackException("the server answered ${conn.responseCode}")
        val total = conn.contentLengthLong
        val out = ByteArrayOutputStream()
        conn.inputStream.use { input ->
            val chunk = ByteArray(16 * 1024)
            while (true) {
                val n = input.read(chunk)
                if (n < 0) break
                out.write(chunk, 0, n)
                if (out.size() > MAX_PACK_BYTES) throw PackException("the pack is too big")
                if (report && total > 0) _status.update { it.copy(progress = out.size().toFloat() / total) }
            }
        }
        return out.toByteArray()
    }

    companion object {
        @Volatile private var instance: EmojiPacks? = null

        fun get(context: Context): EmojiPacks = instance ?: synchronized(this) {
            instance ?: EmojiPacks(File(context.applicationContext.filesDir, "emoji")).also { instance = it }
        }
    }
}

package space.foid.chord.ui.avatar

import android.graphics.BitmapFactory
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import space.foid.chord.data.ChordSession
import space.foid.chord.data.logWarn

/** Decodes image bytes to a bitmap of about [maxPx] pixels, or null if the bytes are no image. */
fun interface AvatarDecoder {
    fun decode(bytes: ByteArray, maxPx: Int): ImageBitmap?
}

/** The real decoder. It reads the size first and decodes a smaller bitmap (`inSampleSize`). */
object BitmapAvatarDecoder : AvatarDecoder {
    override fun decode(bytes: ByteArray, maxPx: Int): ImageBitmap? = try {
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
        val side = minOf(bounds.outWidth, bounds.outHeight)
        var sample = 1
        while (side / (sample * 2) >= maxPx) sample *= 2
        val opts = BitmapFactory.Options().apply { inSampleSize = sample }
        BitmapFactory.decodeByteArray(bytes, 0, bytes.size, opts)?.asImageBitmap()
    } catch (e: Exception) {
        null
    }
}

/**
 * Loads, decodes and caches avatar bitmaps.
 *
 *  - The cache is an LRU in memory, keyed by owner, hash and size class.
 *  - Equal loads that run at the same time share one job.
 *  - If the core has a hash but no data yet, the repository calls `refresh` and asks again after
 *    each of [retryDelaysMs]. The core sends no event when an avatar arrives, so it is a backoff,
 *    as on the desktop.
 *  - An owner with no usable avatar is not asked again for [missTtlMs].
 *  - A bad image gives null: the caller keeps the initials.
 *
 * It clears itself when the signed-in client goes away or changes.
 */
class AvatarRepository(
    private val source: StateFlow<AvatarSource?>,
    private val scope: CoroutineScope,
    private val decoder: AvatarDecoder = BitmapAvatarDecoder,
    private val decodeDispatcher: CoroutineDispatcher = Dispatchers.Default,
    private val retryDelaysMs: List<Long> = listOf(1_500, 5_000, 15_000),
    private val missTtlMs: Long = 60_000,
    private val maxCacheBytes: Int = 16 * 1024 * 1024,
    private val now: () -> Long = System::currentTimeMillis,
) {
    private data class Key(val owner: String, val hash: String, val px: Int)

    private val lock = Any()
    private val cache = LinkedHashMap<Key, ImageBitmap>(64, 0.75f, true)
    private var cacheBytes = 0
    private val misses = HashMap<Key, Long>()
    private val inflight = HashMap<Key, Deferred<ImageBitmap?>>()
    private var generation = 0

    init {
        var last: AvatarSource? = source.value
        scope.launch {
            source.collect { s ->
                if (s !== last) clear()
                last = s
            }
        }
    }

    /** The bitmap if it is in memory now. No work. Use it for the first frame. */
    fun peek(owner: String, hash: String?, sizePx: Int): ImageBitmap? = synchronized(lock) {
        cache[Key(owner, hash.orEmpty(), sizeClass(sizePx))]
    }

    /** Drop everything. Loads that run now do not fill the cache. Call it on sign-out. */
    fun clear() = synchronized(lock) {
        generation++
        cache.clear()
        cacheBytes = 0
        misses.clear()
        inflight.values.forEach { it.cancel() }
        inflight.clear()
    }

    /**
     * The avatar of [owner]. [owner] is a bare JID, or an address with a slash, which the source
     * resolves. [hash] is the hash that a view item carries, or null. Returns null if there is no
     * usable image. Safe to call from any thread.
     */
    suspend fun load(owner: String, hash: String?, sizePx: Int): ImageBitmap? {
        val key = Key(owner, hash.orEmpty(), sizeClass(sizePx))
        val job = synchronized(lock) {
            cache[key]?.let { return it }
            val missed = misses[key]
            if (missed != null) {
                if (now() - missed < missTtlMs) return null
                misses.remove(key)
            }
            inflight.getOrPut(key) {
                val gen = generation
                scope.async(start = CoroutineStart.LAZY) { fetch(key, hash != null, gen) }
            }
        }
        return try {
            job.await()
        } catch (e: CancellationException) {
            // Our own job was cancelled by clear(): the caller gets no image. The caller itself
            // was cancelled: pass it on.
            if (!job.isCancelled) throw e
            null
        }
    }

    private suspend fun fetch(key: Key, hashKnown: Boolean, gen: Int): ImageBitmap? {
        val result = try {
            fetchUncached(key, hashKnown)
        } finally {
            synchronized(lock) { if (generation == gen) inflight.remove(key) }
        }
        synchronized(lock) {
            if (generation != gen) return result
            if (result == null) {
                misses[key] = now()
            } else {
                cache[key] = result
                cacheBytes += bytesOf(result)
                trim()
            }
        }
        return result
    }

    private suspend fun fetchUncached(key: Key, hashKnown: Boolean): ImageBitmap? {
        val src = source.value ?: return null
        val owner = when {
            '/' in key.owner -> src.realJid(key.owner)
            '@' in key.owner -> key.owner
            else -> null
        } ?: return null
        var refreshed = false
        var attempt = 0
        while (true) {
            val stored = guarded { src.avatar(owner) }
            val bytes = stored?.bytes
            if (bytes != null) {
                return withContext(decodeDispatcher) { guarded { decoder.decode(bytes, key.px) } }
            }
            // No avatar known and no hash in the view: nothing to wait for.
            if (stored == null && !hashKnown) return null
            if (!refreshed) {
                refreshed = true
                guarded { src.refresh(owner) }
            }
            if (attempt >= retryDelaysMs.size) return null
            delay(retryDelaysMs[attempt++])
        }
    }

    private suspend fun <T> guarded(block: suspend () -> T): T? = try {
        block()
    } catch (e: CancellationException) {
        throw e
    } catch (e: Exception) {
        logWarn("AvatarRepository", "avatar call failed", e)
        null
    }

    private fun trim() {
        val it = cache.entries.iterator()
        while (cacheBytes > maxCacheBytes && it.hasNext()) {
            val e = it.next()
            cacheBytes -= bytesOf(e.value)
            it.remove()
        }
    }

    private fun bytesOf(b: ImageBitmap) = b.width * b.height * 4

    private fun sizeClass(px: Int): Int {
        var c = 32
        while (c < px && c < 512) c *= 2
        return c
    }

    companion object {
        /** The repository of the app: it follows the client of [session]. */
        fun forSession(session: ChordSession, scope: CoroutineScope) = AvatarRepository(
            source = session.client.map { it?.let(::ClientAvatarSource) }
                .stateIn(scope, SharingStarted.Eagerly, null),
            scope = scope,
        )
    }
}

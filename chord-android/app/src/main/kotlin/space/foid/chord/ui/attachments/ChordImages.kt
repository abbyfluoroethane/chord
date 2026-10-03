package space.foid.chord.ui.attachments

import android.content.Context
import androidx.compose.runtime.Composable
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.ui.graphics.painter.Painter
import coil3.ImageLoader
import coil3.disk.DiskCache
import coil3.request.crossfade
import okio.Path.Companion.toOkioPath

/** Where an inline image is. */
enum class ImageStatus { LOADING, LOADED, FAILED }

/** A painter with its status. [painter] is null until there is something to draw. */
class ImageState(val painter: Painter?, val status: ImageStatus)

/**
 * Where the pictures come from. The app uses Coil. A test gives a fake source, so that no
 * network or decoder runs.
 */
fun interface ImageSource {
    @Composable
    fun stateFor(url: String): ImageState
}

/** Null means the Coil source of the app. */
val LocalImageSource = compositionLocalOf<ImageSource?> { null }

/** The Coil image loader of the app. One loader serves all screens. */
object ChordImages {
    private const val DISK_CACHE_BYTES = 128L * 1024 * 1024

    @Volatile private var loader: ImageLoader? = null

    fun loader(context: Context): ImageLoader {
        loader?.let { return it }
        synchronized(this) {
            loader?.let { return it }
            val app = context.applicationContext
            return ImageLoader.Builder(app)
                .crossfade(true)
                .diskCache {
                    DiskCache.Builder()
                        .directory(app.cacheDir.resolve("image_cache").toOkioPath())
                        .maxSizeBytes(DISK_CACHE_BYTES)
                        .build()
                }
                .build()
                .also { loader = it }
        }
    }
}

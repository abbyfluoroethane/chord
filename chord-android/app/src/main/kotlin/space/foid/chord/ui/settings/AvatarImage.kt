package space.foid.chord.ui.settings

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.net.Uri
import java.io.ByteArrayOutputStream
import kotlin.math.max
import kotlin.math.min

/** An avatar that is ready to send: a square PNG. XEP-0084 requires clients to support PNG. */
class PreparedAvatar(val mime: String, val data: ByteArray, val width: Int, val height: Int)

/** The side of the avatar that Chord sends, in pixels. */
const val AVATAR_SIDE = 256

/**
 * Read the picture at [uri], cut the largest centred square out of it and scale it to
 * [AVATAR_SIDE]. Returns null when the picture cannot be read. Call it off the main thread.
 */
fun prepareAvatar(context: Context, uri: Uri): PreparedAvatar? = try {
    val resolver = context.contentResolver
    val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    resolver.openInputStream(uri)?.use { BitmapFactory.decodeStream(it, null, bounds) }
    // Decode at most twice the target, then scale exactly.
    var sample = 1
    while (min(bounds.outWidth, bounds.outHeight) / (sample * 2) >= AVATAR_SIDE * 2) sample *= 2
    val opts = BitmapFactory.Options().apply { inSampleSize = max(1, sample) }
    val source = resolver.openInputStream(uri)?.use { BitmapFactory.decodeStream(it, null, opts) }
    source?.let { src ->
        val side = min(src.width, src.height)
        val square = Bitmap.createBitmap(src, (src.width - side) / 2, (src.height - side) / 2, side, side)
        val out = Bitmap.createScaledBitmap(square, AVATAR_SIDE, AVATAR_SIDE, true)
        val bytes = ByteArrayOutputStream().also { out.compress(Bitmap.CompressFormat.PNG, 100, it) }.toByteArray()
        PreparedAvatar("image/png", bytes, AVATAR_SIDE, AVATAR_SIDE)
    }
} catch (e: Exception) {
    null
}

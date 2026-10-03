package space.foid.chord.ui.attachments

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Matrix
import android.net.Uri
import android.provider.OpenableColumns
import androidx.exifinterface.media.ExifInterface
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import space.foid.chord.viewmodel.UploadException
import space.foid.chord.viewmodel.UploadFile
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.io.IOException

/** The core refuses a file over this size (`MAX_UPLOAD_BYTES` in chord-core). The server may have a lower limit. */
const val MAX_UPLOAD_BYTES = 100L * 1024 * 1024

/** The name and size that the system gives for a picked file. */
data class PickedInfo(val name: String, val size: Long?, val mime: String?)

/**
 * Reads a picked file for the upload. All work runs on the IO dispatcher.
 *
 * A JPEG or HEIC photo is decoded, turned upright, scaled to [MAX_PHOTO_SIDE] and saved again as a
 * JPEG with quality [PHOTO_JPEG_QUALITY]. This drops all EXIF data, including the location. A PNG
 * or WebP image is only scaled when it is bigger than that. Everything else goes as it is.
 */
object AttachmentReader {
    /** Name, size and type of [uri]. */
    fun info(context: Context, uri: Uri): PickedInfo {
        val resolver = context.contentResolver
        var name: String? = null
        var size: Long? = null
        try {
            resolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE), null, null, null)?.use { c ->
                if (c.moveToFirst()) {
                    val n = c.getColumnIndex(OpenableColumns.DISPLAY_NAME)
                    val s = c.getColumnIndex(OpenableColumns.SIZE)
                    if (n >= 0 && !c.isNull(n)) name = c.getString(n)
                    if (s >= 0 && !c.isNull(s)) size = c.getLong(s)
                }
            }
        } catch (_: Exception) {
            // The picker gave a URI with no columns. The name falls back below.
        }
        val mime = resolver.getType(uri)
        val fallback = uri.lastPathSegment?.substringAfterLast('/')?.takeIf { it.isNotBlank() } ?: "file"
        return PickedInfo(name?.takeIf { it.isNotBlank() } ?: fallback, size, mime)
    }

    /** Read [uri] and prepare it. Throws [UploadException] with plain English text. */
    suspend fun read(context: Context, uri: Uri): UploadFile = withContext(Dispatchers.IO) {
        val info = info(context, uri)
        if (info.size != null && info.size > MAX_UPLOAD_BYTES) throw tooLarge(info.size)
        val bytes = try {
            readLimited(context, uri)
        } catch (e: UploadException) {
            throw e
        } catch (e: IOException) {
            throw UploadException("Chord could not read this file.", e)
        } catch (e: SecurityException) {
            throw UploadException("Chord is not allowed to read this file.", e)
        }
        if (bytes.isEmpty()) throw UploadException("This file is empty.")
        val mime = (info.mime?.takeIf { it != "application/octet-stream" } ?: mimeForName(info.name)).lowercase()
        prepare(info.name, mime, bytes)
    }

    private fun tooLarge(size: Long) =
        UploadException("This file is ${formatBytes(size)}. The limit is ${formatBytes(MAX_UPLOAD_BYTES)}.")

    private fun readLimited(context: Context, uri: Uri): ByteArray {
        val input = context.contentResolver.openInputStream(uri) ?: throw UploadException("Chord could not open this file.")
        input.use { stream ->
            val out = ByteArrayOutputStream()
            val buffer = ByteArray(64 * 1024)
            var total = 0L
            while (true) {
                val n = stream.read(buffer)
                if (n < 0) break
                total += n
                if (total > MAX_UPLOAD_BYTES) throw tooLarge(total)
                out.write(buffer, 0, n)
            }
            return out.toByteArray()
        }
    }

    /** Shrinks a photo and strips its EXIF data. Other files pass through. */
    internal fun prepare(name: String, mime: String, bytes: ByteArray): UploadFile {
        val photo = mime == "image/jpeg" || mime == "image/heic" || mime == "image/heif"
        val big = mime == "image/png" || mime == "image/webp"
        if (!photo && !big) return UploadFile(name, mime, bytes)
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
        val w = bounds.outWidth
        val h = bounds.outHeight
        if (w <= 0 || h <= 0) {
            if (photo) throw UploadException("Chord could not read this photo.")
            return UploadFile(name, mime, bytes)
        }
        if (big && maxOf(w, h) <= MAX_PHOTO_SIDE) return UploadFile(name, mime, bytes)

        val target = downscaledSize(w, h)
        val opts = BitmapFactory.Options().apply { inSampleSize = sampleSizeFor(w, h, target) }
        val decoded = BitmapFactory.decodeByteArray(bytes, 0, bytes.size, opts)
            ?: if (photo) throw UploadException("Chord could not read this photo.") else return UploadFile(name, mime, bytes)
        val orientation = if (photo) {
            runCatching {
                ExifInterface(ByteArrayInputStream(bytes)).getAttributeInt(ExifInterface.TAG_ORIENTATION, ExifInterface.ORIENTATION_NORMAL)
            }.getOrDefault(ExifInterface.ORIENTATION_NORMAL)
        } else {
            ExifInterface.ORIENTATION_NORMAL
        }
        val matrix = Matrix()
        // The decoder sampled the bitmap down by a power of two. This scale does the rest.
        matrix.postScale(target.width.toFloat() / decoded.width, target.height.toFloat() / decoded.height)
        applyOrientation(matrix, orientation)
        val result = Bitmap.createBitmap(decoded, 0, 0, decoded.width, decoded.height, matrix, true)
        if (result !== decoded) decoded.recycle()
        val out = ByteArrayOutputStream()
        val png = !photo
        result.compress(if (png) Bitmap.CompressFormat.PNG else Bitmap.CompressFormat.JPEG, PHOTO_JPEG_QUALITY, out)
        result.recycle()
        return if (png) {
            UploadFile(name, "image/png", out.toByteArray()).let { if (mime == "image/png") it else it.renamed(name.withExtension("png")) }
        } else {
            UploadFile(name.withExtension("jpg"), "image/jpeg", out.toByteArray())
        }
    }

    private fun UploadFile.renamed(newName: String) = UploadFile(newName, contentType, bytes)

    private fun applyOrientation(m: Matrix, orientation: Int) {
        when (orientation) {
            ExifInterface.ORIENTATION_ROTATE_90 -> m.postRotate(90f)
            ExifInterface.ORIENTATION_ROTATE_180 -> m.postRotate(180f)
            ExifInterface.ORIENTATION_ROTATE_270 -> m.postRotate(270f)
            ExifInterface.ORIENTATION_FLIP_HORIZONTAL -> m.postScale(-1f, 1f)
            ExifInterface.ORIENTATION_FLIP_VERTICAL -> m.postScale(1f, -1f)
            ExifInterface.ORIENTATION_TRANSPOSE -> {
                m.postRotate(90f)
                m.postScale(-1f, 1f)
            }
            ExifInterface.ORIENTATION_TRANSVERSE -> {
                m.postRotate(270f)
                m.postScale(-1f, 1f)
            }
        }
    }
}

/** [this] with its extension replaced by [ext]: `IMG_1.HEIC` becomes `IMG_1.jpg`. */
fun String.withExtension(ext: String): String {
    val dot = lastIndexOf('.')
    val base = if (dot > 0) substring(0, dot) else this
    return "$base.$ext"
}

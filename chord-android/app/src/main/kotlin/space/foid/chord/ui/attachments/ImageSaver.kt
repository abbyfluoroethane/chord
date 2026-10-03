package space.foid.chord.ui.attachments

import android.content.ContentValues
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.Environment
import android.provider.MediaStore
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL

/** The biggest image that Save downloads, as on the desktop. */
const val MAX_SAVE_BYTES = 50L * 1024 * 1024

/**
 * Saves an image to the Pictures folder of the phone with MediaStore. This needs no permission on
 * Android 10 and later. Older versions would need the storage permission, so [canSave] is false
 * there and the viewer has no Save button.
 */
object ImageSaver {
    val canSave: Boolean get() = Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q

    /** Download [url] (https, or http for the user's own image) and save it. Throws [IOException]. */
    suspend fun save(context: Context, url: String, allowHttp: Boolean = false) = withContext(Dispatchers.IO) {
        if (!canSave) throw IOException("saving needs Android 10 or later")
        if (!isHttps(url) && !(allowHttp && isWebUrl(url))) throw IOException("the link is not secure")
        val conn = URL(url).openConnection() as HttpURLConnection
        conn.connectTimeout = 15_000
        conn.readTimeout = 30_000
        try {
            if (conn.responseCode !in 200..299) throw IOException("the server answered ${conn.responseCode}")
            val length = conn.contentLengthLong
            if (length > MAX_SAVE_BYTES) throw IOException("the image is too big")
            val mime = conn.contentType?.substringBefore(';')?.takeIf { it.startsWith("image/") }
                ?: mimeForName(fileNameOfUrl(url)).takeIf { it.startsWith("image/") }
                ?: "image/jpeg"
            val name = fileNameOfUrl(url).ifEmpty { "chord-image" }.let {
                if (extensionOf(it).isEmpty()) "$it.${if (mime == "image/png") "png" else "jpg"}" else it
            }
            val resolver = context.contentResolver
            val values = ContentValues().apply {
                put(MediaStore.Images.Media.DISPLAY_NAME, name)
                put(MediaStore.Images.Media.MIME_TYPE, mime)
                put(MediaStore.Images.Media.RELATIVE_PATH, Environment.DIRECTORY_PICTURES + "/Chord")
                put(MediaStore.Images.Media.IS_PENDING, 1)
            }
            val uri = resolver.insert(MediaStore.Images.Media.EXTERNAL_CONTENT_URI, values)
                ?: throw IOException("MediaStore refused the image")
            try {
                resolver.openOutputStream(uri)?.use { out ->
                    conn.inputStream.use { input ->
                        val buffer = ByteArray(64 * 1024)
                        var total = 0L
                        while (true) {
                            val n = input.read(buffer)
                            if (n < 0) break
                            total += n
                            if (total > MAX_SAVE_BYTES) throw IOException("the image is too big")
                            out.write(buffer, 0, n)
                        }
                    }
                } ?: throw IOException("cannot write the image")
                resolver.update(uri, ContentValues().apply { put(MediaStore.Images.Media.IS_PENDING, 0) }, null, null)
            } catch (e: Exception) {
                resolver.delete(uri, null, null)
                throw e
            }
        } finally {
            conn.disconnect()
        }
    }

    /** Share the link of an image with another app. */
    fun share(context: Context, url: String) {
        val send = Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, url)
        context.startActivity(Intent.createChooser(send, null).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
    }
}

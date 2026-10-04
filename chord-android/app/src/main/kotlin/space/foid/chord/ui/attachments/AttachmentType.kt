package space.foid.chord.ui.attachments

import java.net.URI
import java.net.URLDecoder
import java.util.Locale

/** What an attachment is. The type decides how a message shows it. */
enum class AttachmentKind { IMAGE, VIDEO, AUDIO, FILE }

private val IMAGE_EXT = setOf("jpg", "jpeg", "png", "gif", "webp", "bmp", "avif", "heic", "heif")
private val VIDEO_EXT = setOf("mp4", "m4v", "mov", "webm", "mkv", "3gp")
private val AUDIO_EXT = setOf("mp3", "m4a", "aac", "ogg", "oga", "opus", "wav", "flac")

private val MIME_BY_EXT = mapOf(
    "jpg" to "image/jpeg", "jpeg" to "image/jpeg", "png" to "image/png", "gif" to "image/gif",
    "webp" to "image/webp", "bmp" to "image/bmp", "avif" to "image/avif", "heic" to "image/heic",
    "heif" to "image/heif", "mp4" to "video/mp4", "m4v" to "video/mp4", "mov" to "video/quicktime",
    "webm" to "video/webm", "mkv" to "video/x-matroska", "3gp" to "video/3gpp",
    "mp3" to "audio/mpeg", "m4a" to "audio/mp4", "aac" to "audio/aac", "ogg" to "audio/ogg",
    "oga" to "audio/ogg", "opus" to "audio/ogg", "wav" to "audio/wav", "flac" to "audio/flac",
    "pdf" to "application/pdf", "txt" to "text/plain", "zip" to "application/zip",
)

/** The lower-case extension of a file name or URL (query and fragment ignored), or "". */
fun extensionOf(nameOrUrl: String): String {
    val path = nameOrUrl.substringBefore('#').substringBefore('?')
    val last = path.substringAfterLast('/')
    val dot = last.lastIndexOf('.')
    return if (dot < 0 || dot == last.length - 1) "" else last.substring(dot + 1).lowercase(Locale.ROOT)
}

/** The kind from a MIME type, or null when the type says nothing useful. */
fun kindOfMime(mime: String?): AttachmentKind? {
    val m = mime?.substringBefore(';')?.trim()?.lowercase(Locale.ROOT) ?: return null
    return when {
        m.startsWith("image/") -> AttachmentKind.IMAGE
        m.startsWith("video/") -> AttachmentKind.VIDEO
        m.startsWith("audio/") -> AttachmentKind.AUDIO
        m.isEmpty() -> null
        m == "application/octet-stream" -> null
        else -> AttachmentKind.FILE
    }
}

/** The kind from a file name or URL extension. */
fun kindOfName(nameOrUrl: String): AttachmentKind = when (extensionOf(nameOrUrl)) {
    in IMAGE_EXT -> AttachmentKind.IMAGE
    in VIDEO_EXT -> AttachmentKind.VIDEO
    in AUDIO_EXT -> AttachmentKind.AUDIO
    else -> AttachmentKind.FILE
}

/** The kind of an attachment. A known MIME type wins over the extension. */
fun attachmentKind(nameOrUrl: String, mime: String? = null): AttachmentKind =
    kindOfMime(mime) ?: kindOfName(nameOrUrl)

/** A MIME type for a file name. Falls back to `application/octet-stream`. */
fun mimeForName(name: String): String = MIME_BY_EXT[extensionOf(name)] ?: "application/octet-stream"

/** The file name in a URL: the last path segment, percent-decoded. Empty text if there is none. */
fun fileNameOfUrl(url: String): String {
    val path = url.substringBefore('#').substringBefore('?')
    val seg = path.trimEnd('/').substringAfterLast('/')
    if (seg.isEmpty() || seg.endsWith(":")) return ""
    return runCatching { URLDecoder.decode(seg.replace("+", "%2B"), "UTF-8") }.getOrDefault(seg)
}

/** The host of a URL for a short label, or the whole text when it does not parse. */
fun hostOfUrl(url: String): String = runCatching { URI(url).host }.getOrNull() ?: url

/** True for an `https:` URL. */
fun isHttps(url: String): Boolean = url.startsWith("https://", ignoreCase = true)

/** True for an `http:` or `https:` URL. */
fun isWebUrl(url: String): Boolean = url.startsWith("https://", ignoreCase = true) || url.startsWith("http://", ignoreCase = true)

/**
 * Whether the app may load the image at [url] to show it inline. Plain `http` leaks the reader's
 * address to a third party and anyone on the path can swap the image, so only an image that the
 * user sent (and so knows) may use it. Other schemes never load.
 */
fun mayLoadImage(url: String, outgoing: Boolean): Boolean = isHttps(url) || (outgoing && isWebUrl(url))

/** "1.5 MB" style size text. */
fun formatBytes(bytes: Long): String {
    if (bytes < 1024) return "$bytes B"
    val units = arrayOf("KB", "MB", "GB")
    var v = bytes / 1024.0
    var i = 0
    while (v >= 1024 && i < units.lastIndex) {
        v /= 1024
        i++
    }
    return if (v >= 10 || v == Math.floor(v)) "${v.toLong()} ${units[i]}" else String.format(Locale.ROOT, "%.1f %s", v, units[i])
}

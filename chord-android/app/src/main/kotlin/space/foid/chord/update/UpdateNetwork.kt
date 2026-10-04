package space.foid.chord.update

import space.foid.chord.BuildConfig
import java.io.File
import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL

private const val CONNECT_TIMEOUT_MS = 15_000
private const val READ_TIMEOUT_MS = 30_000
private const val MAX_MANIFEST_BYTES = 256 * 1024

private fun open(url: String): HttpURLConnection =
    (URL(url).openConnection() as HttpURLConnection).apply {
        connectTimeout = CONNECT_TIMEOUT_MS
        readTimeout = READ_TIMEOUT_MS
        useCaches = false
        instanceFollowRedirects = true
        setRequestProperty("User-Agent", "Chord-Android/${BuildConfig.VERSION_NAME}")
    }

/** Gets a manifest over HTTPS. 404 gives null. */
object HttpManifestFetcher : ManifestFetcher {
    override fun fetch(url: String): String? {
        val conn = open(url)
        try {
            val code = conn.responseCode
            if (code == HttpURLConnection.HTTP_NOT_FOUND) return null
            if (code != HttpURLConnection.HTTP_OK) throw IOException("HTTP $code")
            val out = java.io.ByteArrayOutputStream()
            conn.inputStream.use { input ->
                val buf = ByteArray(8 * 1024)
                while (true) {
                    val n = input.read(buf)
                    if (n < 0) break
                    out.write(buf, 0, n)
                    if (out.size() > MAX_MANIFEST_BYTES) throw IOException("manifest too large")
                }
            }
            return out.toString("UTF-8")
        } finally {
            conn.disconnect()
        }
    }
}

/**
 * Downloads APKs into `<cache>/updates`. Only the newest download stays there.
 */
class ApkDownloader(cacheDir: File) {
    private val dir = File(cacheDir, "updates")

    /**
     * Downloads [apk] of [build] and checks its SHA-256. A file from an earlier download that
     * matches is used again. [onProgress] gets (bytes so far, total or -1).
     * Throws [ChecksumException] or IOException.
     */
    fun download(build: Long, abi: String, apk: ApkFile, onProgress: (Long, Long) -> Unit): File {
        if (!dir.isDirectory && !dir.mkdirs()) throw IOException("no cache directory")
        val target = File(dir, "chord-$build-$abi.apk")
        dir.listFiles()?.forEach { if (it != target) it.delete() }
        if (target.isFile && target.inputStream().use(::sha256Hex) == apk.sha256) {
            onProgress(target.length(), target.length())
            return target
        }
        val part = File(dir, target.name + ".part")
        val conn = open(apk.url)
        try {
            val code = conn.responseCode
            if (code != HttpURLConnection.HTTP_OK) throw IOException("HTTP $code")
            val total = conn.contentLengthLong.takeIf { it > 0 } ?: apk.size
            conn.inputStream.use { input ->
                part.outputStream().use { out -> copyVerified(input, out, apk.sha256) { onProgress(it, total) } }
            }
        } catch (e: IOException) {
            part.delete()
            throw e
        } finally {
            conn.disconnect()
        }
        if (!part.renameTo(target)) {
            part.delete()
            throw IOException("rename failed")
        }
        return target
    }

    fun clear() {
        dir.listFiles()?.forEach { it.delete() }
    }
}

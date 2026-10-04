package space.foid.chord.ui.emoji

import android.util.JsonReader
import android.util.JsonToken
import org.json.JSONObject
import java.io.BufferedWriter
import java.io.File
import java.io.InputStream
import java.io.InputStreamReader
import java.security.MessageDigest
import java.util.Base64
import java.util.zip.GZIPInputStream

/**
 * The emoji packs. Chord Desktop draws emoji from the npm icon sets of Iconify
 * (@iconify-json/twemoji, noto and fluent-emoji), which are SVG. Android uses the same three
 * tarballs, with the same versions and checksums as chord-desktop/src-tauri/src/emoji.rs.
 *
 * - Twemoji: Twitter and contributors, CC BY 4.0. 1.5 MB.
 * - Noto Emoji: Google, Apache License 2.0. 3 MB.
 * - Fluent Emoji: Microsoft, MIT License. 14 MB.
 *
 * No pack is in the APK. A pack downloads when the user picks it, and until it is ready the
 * phone font draws emoji. [System] is the font of the phone and needs no download.
 */
enum class EmojiPack(
    val title: String,
    /** Megabytes to download, or null for no download. */
    val downloadMb: Int?,
    val url: String?,
    /** The npm `dist.integrity` of the tarball. */
    val integrity: String?,
) {
    Twemoji(
        "Twemoji", 2,
        "https://registry.npmjs.org/@iconify-json/twemoji/-/twemoji-1.2.5.tgz",
        "sha512-uKpuIEV0v6K5BW3Mjdyl+XKFVAbbcPxAgifKvEMtZoUZB5+YiY5zaMm2uNNCxyXzAWU9yNLlj41WU6/mvgALsw==",
    ),
    Noto(
        "Noto Emoji", 3,
        "https://registry.npmjs.org/@iconify-json/noto/-/noto-1.2.9.tgz",
        "sha512-DQiuXENbun41ch+XPSV55H9FaWKpHTyz46YIKtSaKYD4fIDDzL1tR6H4np/T38jfrTAeI3rcKF0mqHYeZVAuaw==",
    ),
    Fluent(
        "Fluent Emoji", 14,
        "https://registry.npmjs.org/@iconify-json/fluent-emoji/-/fluent-emoji-1.2.7.tgz",
        "sha512-D8G6bKAyIsyxP1rzZtIWr/gg/fO4Rg1+DyCWlfqKK4h8W+4Em/Y0P6QYgJUQTlUcagzJvSyg+9OBv4hz4Hc/QA==",
    ),
    System("System", null, null, null);

    /** The folder name of the pack. */
    val dirName: String get() = name.lowercase()

    companion object {
        fun fromName(name: String?): EmojiPack = entries.firstOrNull { it.name == name } ?: System
    }
}

/** The most bytes of a downloaded pack: 32 MB. Fluent is about 14 MB. */
const val MAX_PACK_BYTES = 32 * 1024 * 1024

/**
 * The file name of an emoji in a pack: the code points in lower-case hex, at least 4 digits,
 * joined with "-", without U+FE0F (the emoji presentation selector).
 */
fun emojiHex(emoji: String): String =
    emoji.codePoints().toArray().filter { it != 0xFE0F }.joinToString("-") { it.toString(16).padStart(4, '0') }

/** Checks a tarball against the npm integrity string (`sha512-<base64>`). */
fun verifyIntegrity(bytes: ByteArray, integrity: String): Boolean {
    val want = integrity.removePrefix("sha512-")
    val expected = try { Base64.getDecoder().decode(want) } catch (_: Exception) { return false }
    return MessageDigest.isEqual(MessageDigest.getInstance("SHA-512").digest(bytes), expected)
}

private val TONES = listOf("medium-light", "medium-dark", "light", "medium", "dark")

/**
 * Code points by icon name, from the Twemoji names (hex to name). The Fluent set has no code
 * point map, and its names put the skin tone last without "skin-tone": "waving-hand-light".
 * Twemoji says "waving-hand-light-skin-tone". Most Fluent names match. The rest draw with the
 * phone font. Same rule as codes_by_name in emoji.rs.
 */
fun codesByName(twemojiChars: Map<String, String>): Map<String, String> {
    val byName = HashMap<String, String>()
    for ((hex, name) in twemojiChars) {
        byName.putIfAbsent(name.replace("-skin-tone", ""), hex)
        for (tone in TONES) {
            val mark = "-$tone-skin-tone"
            if (name.contains(mark) && !name.endsWith(mark)) {
                byName.putIfAbsent(name.replaceFirst(mark, "") + "-$tone", hex)
                break
            }
        }
        byName.putIfAbsent(name, hex)
    }
    return byName
}

private val HEX_KEY = Regex("^[0-9a-f]{1,6}(-[0-9a-f]{1,6}){0,15}$")
private val SAFE_NAME = Regex("^[A-Za-z0-9_.-]{1,160}$")

/** Reads the entries of a tar stream. [onEntry] must read what it wants and may leave the rest. */
internal fun readTar(input: InputStream, onEntry: (name: String, data: InputStream) -> Unit) {
    val header = ByteArray(512)
    while (true) {
        if (!input.readFully(header)) return
        if (header.all { it == 0.toByte() }) return
        val name = String(header, 0, 100, Charsets.UTF_8).trimEnd('\u0000')
        val size = String(header, 124, 12, Charsets.US_ASCII).trim { it == '\u0000' || it == ' ' }.toLongOrNull(8) ?: return
        val type = header[156].toInt().toChar()
        val bounded = BoundedInput(input, size)
        if (type == '0' || type == '\u0000') onEntry(name, bounded)
        bounded.drain()
        val pad = (512 - size % 512) % 512
        input.skipFully(pad)
    }
}

private fun InputStream.readFully(buf: ByteArray): Boolean {
    var n = 0
    while (n < buf.size) {
        val r = read(buf, n, buf.size - n)
        if (r < 0) return false
        n += r
    }
    return true
}

private fun InputStream.skipFully(count: Long) {
    var left = count
    val sink = ByteArray(8192)
    while (left > 0) {
        val r = read(sink, 0, minOf(left, sink.size.toLong()).toInt())
        if (r < 0) return
        left -= r
    }
}

private class BoundedInput(private val src: InputStream, size: Long) : InputStream() {
    private var left = size
    override fun read(): Int {
        if (left <= 0) return -1
        val b = src.read()
        if (b >= 0) left--
        return b
    }
    override fun read(b: ByteArray, off: Int, len: Int): Int {
        if (left <= 0) return -1
        val r = src.read(b, off, minOf(len.toLong(), left).toInt())
        if (r > 0) left -= r
        return r
    }
    fun drain() = src.skipFully(left).also { left = 0 }
    override fun close() {}
}

/** What a pack folder holds. */
internal object PackLayout {
    const val NAMES = "names"
    const val CHARS = "chars.json"
    const val ALIASES = "aliases.json"
    const val META = "meta"
    const val DONE = ".installed"
}

/** Thrown when a pack cannot be read. The text is for the log, not for the user. */
class PackException(message: String) : Exception(message)

/**
 * Writes the pack in [tgz] to [target]. The files go to a new folder first and then replace
 * the old one, so a failed install leaves the old pack as it was.
 *
 * Layout: `names/<icon name>` holds `left top width height` (0 means the set default) and the
 * SVG body; `chars.json` maps a hex code to an icon name; `aliases.json` maps an alias to its
 * parent; `meta` holds the default width and height.
 *
 * @param twemojiChars hex to name of Twemoji, for a pack with no code point map (Fluent).
 */
fun installPack(tgz: InputStream, target: File, twemojiChars: Map<String, String>?) {
    val temp = File(target.parentFile, ".${target.name}-new")
    temp.deleteRecursively()
    val names = File(temp, PackLayout.NAMES).also { it.mkdirs() }
    var chars: Map<String, String>? = null
    val aliases = LinkedHashMap<String, String>()
    val iconNames = HashSet<String>()
    var width = 32.0
    var height = 32.0
    GZIPInputStream(tgz.buffered(), 64 * 1024).use { gz ->
        readTar(gz) { entry, data ->
            when (entry) {
                "package/chars.json" -> {
                    val o = JSONObject(data.readBytes().toString(Charsets.UTF_8))
                    chars = o.keys().asSequence().associateWith { o.getString(it) }
                }
                "package/icons.json" -> JsonReader(InputStreamReader(data, Charsets.UTF_8)).use { r ->
                    r.beginObject()
                    while (r.hasNext()) {
                        when (r.nextName()) {
                            "icons" -> readIcons(r, names, iconNames)
                            "aliases" -> readAliases(r, aliases)
                            "width" -> width = r.nextDouble()
                            "height" -> height = r.nextDouble()
                            else -> r.skipValue()
                        }
                    }
                    r.endObject()
                }
            }
        }
    }
    if (iconNames.isEmpty()) throw PackException("the pack has no icons")
    val map = chars ?: run {
        val codes = codesByName(twemojiChars ?: throw PackException("no code point map for this pack"))
        val out = LinkedHashMap<String, String>()
        for (name in iconNames + aliases.keys) codes[name]?.let { out.putIfAbsent(it, name) }
        out
    }
    val clean = map.filterKeys { HEX_KEY.matches(it) }
    File(temp, PackLayout.CHARS).writeText(JSONObject(clean as Map<*, *>).toString())
    File(temp, PackLayout.ALIASES).writeText(JSONObject(aliases as Map<*, *>).toString())
    File(temp, PackLayout.META).writeText("${num(width)} ${num(height)}")
    File(temp, PackLayout.DONE).writeText("ok")
    swapIn(temp, target)
}

private fun readIcons(r: JsonReader, names: File, seen: MutableSet<String>) {
    r.beginObject()
    while (r.hasNext()) {
        val name = r.nextName()
        var body = ""
        var left = 0.0
        var top = 0.0
        var w = 0.0
        var h = 0.0
        r.beginObject()
        while (r.hasNext()) {
            when (r.nextName()) {
                "body" -> body = r.nextString()
                "left" -> left = r.nextDouble()
                "top" -> top = r.nextDouble()
                "width" -> w = r.nextDouble()
                "height" -> h = r.nextDouble()
                else -> r.skipValue()
            }
        }
        r.endObject()
        if (SAFE_NAME.matches(name) && body.isNotEmpty()) {
            File(names, name).bufferedWriter().use { it.writeIcon(left, top, w, h, body) }
            seen += name
        }
    }
    r.endObject()
}

/** A number for the files: 36 and not 36.0. */
private fun num(d: Double): String = if (d == Math.floor(d) && Math.abs(d) < 1e9) d.toLong().toString() else d.toString()

private fun BufferedWriter.writeIcon(left: Double, top: Double, w: Double, h: Double, body: String) {
    write("${num(left)} ${num(top)} ${num(w)} ${num(h)}\n")
    write(body)
}

private fun readAliases(r: JsonReader, out: MutableMap<String, String>) {
    r.beginObject()
    while (r.hasNext()) {
        val name = r.nextName()
        if (r.peek() != JsonToken.BEGIN_OBJECT) { r.skipValue(); continue }
        r.beginObject()
        while (r.hasNext()) {
            if (r.nextName() == "parent") out[name] = r.nextString() else r.skipValue()
        }
        r.endObject()
    }
    r.endObject()
}

/** Puts [temp] in place of [target]. The old folder goes away only after the rename works. */
private fun swapIn(temp: File, target: File) {
    val backup = File(target.parentFile, ".${target.name}-old")
    backup.deleteRecursively()
    if (target.exists() && !target.renameTo(backup)) throw PackException("cannot move the old pack")
    if (!temp.renameTo(target)) {
        if (backup.exists()) backup.renameTo(target)
        throw PackException("cannot move the new pack")
    }
    backup.deleteRecursively()
}

/** An installed pack on disk, ready to give SVG text. */
class PackIndex(private val dir: File) {
    private val chars: Map<String, String>
    private val aliases: Map<String, String>
    private val defaultW: String
    private val defaultH: String

    init {
        fun map(f: String): Map<String, String> {
            val o = JSONObject(File(dir, f).readText())
            return o.keys().asSequence().associateWith { o.getString(it) }
        }
        chars = map(PackLayout.CHARS)
        aliases = map(PackLayout.ALIASES)
        val (w, h) = File(dir, PackLayout.META).readText().trim().split(' ')
        defaultW = w
        defaultH = h
    }

    /** The icon name of an emoji, or null if the pack lacks it. */
    private fun nameOf(emoji: String): String? {
        val name = chars[emojiHex(emoji)] ?: return null
        val own = File(dir, "${PackLayout.NAMES}/$name")
        if (own.isFile) return name
        return aliases[name]?.takeIf { File(dir, "${PackLayout.NAMES}/$it").isFile }
    }

    fun has(emoji: String): Boolean = nameOf(emoji) != null

    /** The SVG document of an emoji, or null. */
    fun svg(emoji: String): String? {
        val name = nameOf(emoji) ?: return null
        val text = File(dir, "${PackLayout.NAMES}/$name").readText()
        val nl = text.indexOf('\n')
        if (nl < 0) return null
        val (left, top, w0, h0) = text.substring(0, nl).split(' ')
        val w = if ((w0.toDoubleOrNull() ?: 0.0) > 0) w0 else defaultW
        val h = if ((h0.toDoubleOrNull() ?: 0.0) > 0) h0 else defaultH
        return """<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="$left $top $w $h" width="$w" height="$h">${text.substring(nl + 1)}</svg>"""
    }

    companion object {
        /** True if [dir] holds a complete pack. */
        fun isInstalled(dir: File): Boolean = File(dir, PackLayout.DONE).isFile

        /** The hex to name map of an installed pack, for the packs that need Twemoji's names. */
        fun readChars(dir: File): Map<String, String>? = try {
            val o = JSONObject(File(dir, PackLayout.CHARS).readText())
            o.keys().asSequence().associateWith { o.getString(it) }
        } catch (_: Exception) {
            null
        }
    }
}

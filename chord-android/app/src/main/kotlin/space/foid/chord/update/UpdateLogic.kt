package space.foid.chord.update

import org.json.JSONException
import org.json.JSONObject
import java.io.IOException
import java.io.InputStream
import java.io.OutputStream
import java.security.MessageDigest

// The rules of the updater, without Android. See docs/updates.md.

/** An update channel. [speed] orders them: a faster channel gets more builds. */
enum class UpdateChannel(val id: String, val speed: Int) {
    Stable("stable", 0),
    Beta("beta", 1),
    Nightly("nightly", 2);

    companion object {
        fun fromId(id: String?): UpdateChannel? = entries.firstOrNull { it.id == id }
    }
}

/** The channel to follow when the user did not pick one: the channel of the build, Stable for a dev build. */
fun defaultChannel(buildChannel: String): UpdateChannel = UpdateChannel.fromId(buildChannel) ?: UpdateChannel.Stable

/** The channel to follow: the one the user picked, else the default. */
fun effectiveChannel(picked: UpdateChannel?, buildChannel: String): UpdateChannel = picked ?: defaultChannel(buildChannel)

/** Whether the build updates itself. A dev build and a store build (-Pchord.updater=false) do not. */
fun updaterEnabled(updater: Boolean, buildChannel: String): Boolean = updater && buildChannel != "dev"

/**
 * True when [picked] is slower than the channel of the installed build. The user then keeps the
 * build until the slower channel has a newer one (no downgrades).
 */
fun isSlowerThanBuild(picked: UpdateChannel, buildChannel: String): Boolean {
    val build = UpdateChannel.fromId(buildChannel) ?: return false
    return picked.speed < build.speed
}

const val MANIFEST_BASE = "https://raw.githubusercontent.com/abbyfluoroethane/chord-android/main/channels/"

fun manifestUrl(channel: UpdateChannel): String = "$MANIFEST_BASE${channel.id}.json"

/** One APK of a build. [size] is -1 when the manifest does not give it. */
data class ApkFile(val url: String, val sha256: String, val size: Long)

/** A channel manifest, `channels/<channel>.json`. */
data class UpdateManifest(
    val version: String,
    val build: Long,
    val channel: String,
    val commit: String,
    val pubDate: String,
    val releaseUrl: String,
    val notes: String,
    /** By ABI name, plus "universal". */
    val apks: Map<String, ApkFile>,
)

class ManifestException(message: String) : Exception(message)

/**
 * Reads a manifest. `version`, `build` and at least one usable APK are required. An APK entry
 * without a `url` or a 64 hex digit `sha256` is left out. Other fields may be missing.
 */
fun parseManifest(json: String): UpdateManifest {
    val o = try {
        JSONObject(json)
    } catch (e: JSONException) {
        throw ManifestException("not JSON")
    }
    val version = o.optString("version").takeIf { it.isNotBlank() } ?: throw ManifestException("no version")
    val build = o.optLong("build", -1L).takeIf { it > 0 } ?: throw ManifestException("no build")
    val apksJson = o.optJSONObject("apks") ?: throw ManifestException("no apks")
    val apks = buildMap {
        for (abi in apksJson.keys()) {
            val a = apksJson.optJSONObject(abi) ?: continue
            val url = a.optString("url").takeIf { it.startsWith("https://") } ?: continue
            val sha = a.optString("sha256").lowercase().takeIf { SHA256_HEX.matches(it) } ?: continue
            put(abi, ApkFile(url, sha, a.optLong("size", -1L)))
        }
    }
    if (apks.isEmpty()) throw ManifestException("no usable apk")
    return UpdateManifest(
        version = version,
        build = build,
        channel = o.optString("channel"),
        commit = o.optString("commit"),
        pubDate = o.optString("pub_date"),
        releaseUrl = o.optString("release_url"),
        notes = o.optString("notes"),
        apks = apks,
    )
}

private val SHA256_HEX = Regex("^[0-9a-f]{64}$")

/** The APK for this phone: the first of [supportedAbis] that the manifest has, else "universal". */
fun pickApk(manifest: UpdateManifest, supportedAbis: List<String>): Pair<String, ApkFile>? {
    for (abi in supportedAbis) manifest.apks[abi]?.let { return abi to it }
    return manifest.apks["universal"]?.let { "universal" to it }
}

/** No downgrades: only a higher build number is an update. */
fun isUpdate(manifest: UpdateManifest, installedBuild: Long): Boolean = manifest.build > installedBuild

/** One notification per new build: only a build newer than the last one notified. */
fun shouldNotify(build: Long, lastNotified: Long): Boolean = build > lastNotified

/** Gets a manifest. Returns null when the server says 404 (no build on that channel). Throws IOException otherwise. */
fun interface ManifestFetcher {
    @Throws(IOException::class)
    fun fetch(url: String): String?
}

/** What a check found. */
sealed interface CheckResult {
    /** The channel has no build yet (404). */
    data object NoBuild : CheckResult

    /** The newest build of the channel is not newer than the installed one. */
    data class UpToDate(val manifest: UpdateManifest) : CheckResult

    data class Available(val manifest: UpdateManifest, val abi: String, val apk: ApkFile) : CheckResult

    data class Failed(val reason: UpdateFailure) : CheckResult
}

/** Why a step failed. The UI shows a short line for each. */
enum class UpdateFailure { Network, BadManifest, NoApk, Checksum, Install }

/** Checks one channel. Blocking: call it off the main thread. */
class UpdateChecker(
    private val fetcher: ManifestFetcher,
    private val installedBuild: Long,
    private val supportedAbis: List<String>,
) {
    fun check(channel: UpdateChannel): CheckResult {
        val text = try {
            fetcher.fetch(manifestUrl(channel)) ?: return CheckResult.NoBuild
        } catch (e: IOException) {
            return CheckResult.Failed(UpdateFailure.Network)
        }
        val manifest = try {
            parseManifest(text)
        } catch (e: ManifestException) {
            return CheckResult.Failed(UpdateFailure.BadManifest)
        }
        if (!isUpdate(manifest, installedBuild)) return CheckResult.UpToDate(manifest)
        val (abi, apk) = pickApk(manifest, supportedAbis) ?: return CheckResult.Failed(UpdateFailure.NoApk)
        return CheckResult.Available(manifest, abi, apk)
    }
}

class ChecksumException : IOException("SHA-256 does not match")

/**
 * Copies [input] to [output] and checks its SHA-256 against [expectedSha256] (lower-case hex).
 * [onProgress] gets the bytes copied so far. Throws [ChecksumException] on a mismatch.
 */
fun copyVerified(input: InputStream, output: OutputStream, expectedSha256: String, onProgress: (Long) -> Unit = {}) {
    val digest = MessageDigest.getInstance("SHA-256")
    val buf = ByteArray(64 * 1024)
    var done = 0L
    while (true) {
        val n = input.read(buf)
        if (n < 0) break
        digest.update(buf, 0, n)
        output.write(buf, 0, n)
        done += n
        onProgress(done)
    }
    if (digest.digest().toHex() != expectedSha256.lowercase()) throw ChecksumException()
}

/** The SHA-256 of [input], lower-case hex. */
fun sha256Hex(input: InputStream): String {
    val digest = MessageDigest.getInstance("SHA-256")
    val buf = ByteArray(64 * 1024)
    while (true) {
        val n = input.read(buf)
        if (n < 0) break
        digest.update(buf, 0, n)
    }
    return digest.digest().toHex()
}

private fun ByteArray.toHex(): String = joinToString("") { "%02x".format(it) }

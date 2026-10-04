package space.foid.chord.update

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import space.foid.chord.ui.settings.AppPrefs
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.io.IOException

// Robolectric for the real org.json. No network: the fetcher is a fake.
@RunWith(RobolectricTestRunner::class)
class UpdateLogicTest {
    private val shaA = "a".repeat(64)
    private val shaB = "B".repeat(64)

    private fun manifest(
        build: Long = 1_612_345,
        apks: String = """
            "arm64-v8a": { "url": "https://example.org/a.apk", "sha256": "$shaA", "size": 17756806 },
            "x86_64": { "url": "https://example.org/x.apk", "sha256": "$shaB", "size": 19919549 },
            "universal": { "url": "https://example.org/u.apk", "sha256": "$shaA" }
        """,
    ) = """
        {
          "version": "0.3.0-beta.2",
          "build": $build,
          "channel": "beta",
          "commit": "09c83fb",
          "pub_date": "2026-10-04T06:00:00Z",
          "release_url": "https://github.com/abbyfluoroethane/chord-android/releases/tag/v0.3.0-beta.2%2B09c83fb",
          "notes": "Fixes.",
          "apks": { $apks }
        }
    """.trimIndent()

    @Test fun parsesTheFullManifest() {
        val m = parseManifest(manifest())
        assertEquals("0.3.0-beta.2", m.version)
        assertEquals(1_612_345L, m.build)
        assertEquals("beta", m.channel)
        assertEquals("09c83fb", m.commit)
        assertEquals("2026-10-04T06:00:00Z", m.pubDate)
        assertEquals("Fixes.", m.notes)
        assertEquals(setOf("arm64-v8a", "x86_64", "universal"), m.apks.keys)
        assertEquals(17_756_806L, m.apks.getValue("arm64-v8a").size)
        // The hash is kept in lower case. A missing size is -1.
        assertEquals("b".repeat(64), m.apks.getValue("x86_64").sha256)
        assertEquals(-1L, m.apks.getValue("universal").size)
    }

    @Test fun optionalFieldsMayBeMissing() {
        val m = parseManifest("""{"version":"0.3.0","build":5,"apks":{"universal":{"url":"https://e.org/u.apk","sha256":"$shaA"}}}""")
        assertEquals("", m.notes)
        assertEquals("", m.commit)
        assertEquals("", m.releaseUrl)
    }

    @Test fun requiredFieldsMustBeThere() {
        val bad = listOf(
            "not json",
            """{"build":5,"apks":{"universal":{"url":"https://e.org/u.apk","sha256":"$shaA"}}}""",
            """{"version":"0.3.0","apks":{"universal":{"url":"https://e.org/u.apk","sha256":"$shaA"}}}""",
            """{"version":"0.3.0","build":5}""",
            // Each APK lacks something: no usable APK at all.
            """{"version":"0.3.0","build":5,"apks":{"universal":{"url":"https://e.org/u.apk"},"x86_64":{"sha256":"$shaA"},"arm64-v8a":{"url":"http://e.org/a.apk","sha256":"$shaA"}}}""",
            """{"version":"0.3.0","build":5,"apks":{"universal":{"url":"https://e.org/u.apk","sha256":"abc"}}}""",
        )
        for (json in bad) {
            try {
                parseManifest(json)
                fail("parsed: $json")
            } catch (_: ManifestException) {
            }
        }
    }

    @Test fun anEntryWithoutAHashIsLeftOut() {
        val m = parseManifest(
            manifest(
                apks = """
                    "arm64-v8a": { "url": "https://example.org/a.apk" },
                    "universal": { "url": "https://example.org/u.apk", "sha256": "$shaA" }
                """,
            ),
        )
        assertEquals(setOf("universal"), m.apks.keys)
    }

    @Test fun picksTheFirstSupportedAbi() {
        val m = parseManifest(manifest())
        assertEquals("arm64-v8a", pickApk(m, listOf("arm64-v8a", "armeabi-v7a"))?.first)
        assertEquals("x86_64", pickApk(m, listOf("x86_64", "x86", "arm64-v8a"))?.first)
    }

    @Test fun anUnknownAbiTakesTheUniversalApk() {
        val m = parseManifest(manifest())
        assertEquals("universal", pickApk(m, listOf("riscv64"))?.first)
        assertEquals("https://example.org/u.apk", pickApk(m, listOf("riscv64"))?.second?.url)
    }

    @Test fun noMatchAndNoUniversalGivesNull() {
        val m = parseManifest(manifest(apks = """"x86_64": { "url": "https://e.org/x.apk", "sha256": "$shaA" }"""))
        assertNull(pickApk(m, listOf("arm64-v8a")))
    }

    @Test fun onlyAHigherBuildIsAnUpdate() {
        val m = parseManifest(manifest(build = 100))
        assertTrue(isUpdate(m, 99))
        assertFalse(isUpdate(m, 100))
        assertFalse(isUpdate(m, 101))
    }

    @Test fun channelDefaults() {
        assertEquals(UpdateChannel.Stable, defaultChannel("stable"))
        assertEquals(UpdateChannel.Beta, defaultChannel("beta"))
        assertEquals(UpdateChannel.Nightly, defaultChannel("nightly"))
        assertEquals(UpdateChannel.Stable, defaultChannel("dev"))
        assertEquals(UpdateChannel.Beta, effectiveChannel(null, "beta"))
        assertEquals(UpdateChannel.Nightly, effectiveChannel(UpdateChannel.Nightly, "beta"))
        // A fresh install has no channel picked: it follows its build.
        assertNull(AppPrefs().updateChannel)
        assertTrue(AppPrefs().autoUpdateCheck)
    }

    @Test fun theUpdaterIsOffForDevAndStoreBuilds() {
        assertFalse(updaterEnabled(true, "dev"))
        assertFalse(updaterEnabled(false, "stable"))
        assertTrue(updaterEnabled(true, "nightly"))
    }

    @Test fun slowerChannelThanTheBuild() {
        assertTrue(isSlowerThanBuild(UpdateChannel.Stable, "nightly"))
        assertTrue(isSlowerThanBuild(UpdateChannel.Beta, "nightly"))
        assertFalse(isSlowerThanBuild(UpdateChannel.Nightly, "beta"))
        assertFalse(isSlowerThanBuild(UpdateChannel.Stable, "stable"))
        assertFalse(isSlowerThanBuild(UpdateChannel.Stable, "dev"))
    }

    @Test fun notifiesOncePerNewBuild() {
        assertTrue(shouldNotify(200, 0))
        assertFalse(shouldNotify(200, 200))
        assertFalse(shouldNotify(150, 200))
        assertTrue(shouldNotify(201, 200))
    }

    @Test fun manifestUrlPerChannel() {
        assertEquals(
            "https://raw.githubusercontent.com/abbyfluoroethane/chord-android/main/channels/nightly.json",
            manifestUrl(UpdateChannel.Nightly),
        )
    }

    private class FakeFetcher(val body: String?, val fail: Boolean = false) : ManifestFetcher {
        val urls = mutableListOf<String>()
        override fun fetch(url: String): String? {
            urls += url
            if (fail) throw IOException("offline")
            return body
        }
    }

    @Test fun checkerFindsAnUpdate() {
        val f = FakeFetcher(manifest(build = 500))
        val r = UpdateChecker(f, installedBuild = 400, supportedAbis = listOf("x86_64")).check(UpdateChannel.Beta)
        r as CheckResult.Available
        assertEquals("x86_64", r.abi)
        assertEquals(listOf(manifestUrl(UpdateChannel.Beta)), f.urls)
    }

    @Test fun checkerResults() {
        assertEquals(CheckResult.NoBuild, UpdateChecker(FakeFetcher(null), 1, listOf("x86_64")).check(UpdateChannel.Stable))
        assertTrue(UpdateChecker(FakeFetcher(manifest(build = 400)), 400, listOf("x86_64")).check(UpdateChannel.Stable) is CheckResult.UpToDate)
        assertEquals(
            CheckResult.Failed(UpdateFailure.Network),
            UpdateChecker(FakeFetcher(null, fail = true), 1, listOf("x86_64")).check(UpdateChannel.Stable),
        )
        assertEquals(
            CheckResult.Failed(UpdateFailure.BadManifest),
            UpdateChecker(FakeFetcher("{}"), 1, listOf("x86_64")).check(UpdateChannel.Stable),
        )
        val noApk = manifest(build = 500, apks = """"arm64-v8a": { "url": "https://e.org/a.apk", "sha256": "$shaA" }""")
        assertEquals(
            CheckResult.Failed(UpdateFailure.NoApk),
            UpdateChecker(FakeFetcher(noApk), 1, listOf("x86_64")).check(UpdateChannel.Stable),
        )
    }

    @Test fun copyChecksTheHash() {
        val data = "hello".toByteArray()
        // sha256("hello")
        val sha = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        assertEquals(sha, sha256Hex(ByteArrayInputStream(data)))
        val out = ByteArrayOutputStream()
        var progress = 0L
        copyVerified(ByteArrayInputStream(data), out, sha.uppercase()) { progress = it }
        assertEquals("hello", out.toString("UTF-8"))
        assertEquals(5L, progress)
        try {
            copyVerified(ByteArrayInputStream(data), ByteArrayOutputStream(), shaA)
            fail("no checksum error")
        } catch (_: ChecksumException) {
        }
    }
}

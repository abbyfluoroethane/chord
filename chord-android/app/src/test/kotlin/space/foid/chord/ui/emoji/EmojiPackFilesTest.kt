package space.foid.chord.ui.emoji

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import androidx.compose.ui.graphics.asAndroidBitmap
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import space.foid.chord.ui.text.emojiRuns
import space.foid.chord.ui.theme.CustomTheme
import space.foid.chord.ui.theme.ThemeLibrary
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.io.File
import java.util.zip.GZIPOutputStream

@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [36])
class EmojiPackFilesTest {
    @get:Rule val tmp = TemporaryFolder()

    /** The Twemoji tarball that the desktop bundles. It is the same file the app downloads. */
    private val twemojiTgz = File("../../chord-desktop/src-tauri/resources/twemoji-1.2.5.tgz")

    @Test fun hexNamesDropTheVariationSelector() {
        assertEquals("1f600", emojiHex("😀"))
        assertEquals("2764", emojiHex("❤️"))
        assertEquals("1f44b-1f3fd", emojiHex("👋🏽"))
        assertEquals("0023-20e3", emojiHex("#️⃣"))
    }

    @Test fun twemojiTarballMatchesItsChecksum() {
        assertTrue(twemojiTgz.isFile)
        assertTrue(verifyIntegrity(twemojiTgz.readBytes(), EmojiPack.Twemoji.integrity!!))
        assertFalse(verifyIntegrity(byteArrayOf(1, 2, 3), EmojiPack.Twemoji.integrity!!))
    }

    @Test fun installsTheRealTwemojiPackAndDrawsAnEmoji() {
        val target = File(tmp.newFolder(), "twemoji")
        installPack(twemojiTgz.inputStream(), target, null)
        assertTrue(PackIndex.isInstalled(target))
        val index = PackIndex(target)
        val grin = "😀"
        assertTrue(index.has(grin))
        assertTrue(index.has("👋🏽"))
        val svg = index.svg(grin)!!
        assertTrue(svg, svg.startsWith("<svg") && svg.contains("viewBox=\"0 0 36 36\""))
        // An alias name draws too: "womans-flat-shoe" is an alias of "flat-shoe".
        assertFalse(index.has("not an emoji"))
        // And it renders to a bitmap with some colour in it.
        val bmp = EmojiImages(EmojiPack.Twemoji, index, px = 48).loadNow(grin)
        assertNotNull(bmp)
        val px = bmp!!.asAndroidBitmap().getPixel(24, 24)
        assertTrue("centre pixel is drawn", (px ushr 24) > 0)
        // A second install replaces the first one.
        installPack(twemojiTgz.inputStream(), target, null)
        assertTrue(PackIndex(target).has(grin))
        assertTrue(PackIndex.readChars(target)!!.isNotEmpty())
    }

    @Test fun fluentStylePackTakesItsCodesFromTwemojiNames() {
        // Twemoji says "waving-hand-light-skin-tone". Fluent says "waving-hand-light".
        val twemoji = mapOf(
            "1f600" to "grinning-face",
            "1f44b-1f3fb" to "waving-hand-light-skin-tone",
            "1f468-1f3fb-200d-1f9b2" to "man-light-skin-tone-bald",
        )
        val codes = codesByName(twemoji)
        assertEquals("1f600", codes["grinning-face"])
        assertEquals("1f44b-1f3fb", codes["waving-hand-light"])
        assertEquals("1f468-1f3fb-200d-1f9b2", codes["man-light-bald"])

        // Like the real Fluent tarball: chars.json is there but empty.
        val tgz = tarGz(
            "package/chars.json" to "{}",
            "package/icons.json" to
                """{"prefix":"fluent-emoji","icons":{"grinning-face":{"body":"<circle cx='16' cy='16' r='14' fill='#fc0'/>"},""" +
                """"waving-hand-light":{"body":"<rect width='20' height='20' fill='#c96'/>","width":24,"height":24}},""" +
                """"aliases":{"waving-hand-pale":{"parent":"waving-hand-light"}},"width":32,"height":32}""",
        )
        val target = File(tmp.newFolder(), "fluent")
        installPack(ByteArrayInputStream(tgz), target, twemoji)
        assertTrue(PackIndex.isInstalled(target))
        val index = PackIndex(target)
        assertTrue(index.has("😀"))
        assertTrue(index.has("👋🏻"))
        assertFalse(index.has("👨🏻‍🦲"))
        assertTrue(index.svg("👋🏻")!!.contains("viewBox=\"0 0 24 24\""))
        assertTrue(index.svg("😀")!!.contains("viewBox=\"0 0 32 32\""))
    }

    @Test fun aPackThatMapsNoEmojiDoesNotCountAsInstalled() {
        val dir = File(tmp.newFolder(), "fluent").also { it.mkdirs() }
        File(dir, "chars.json").writeText("{}")
        File(dir, ".installed").writeText("ok")
        assertFalse(PackIndex.isInstalled(dir))
    }

    @Test fun aPackWithoutAMapOrIconsFailsAndKeepsTheOldOne() {
        val dir = tmp.newFolder()
        val target = File(dir, "p")
        installPack(twemojiTgz.inputStream(), target, null)
        val bad = tarGz("package/icons.json" to """{"icons":{},"width":8,"height":8}""")
        val failed = runCatching { installPack(ByteArrayInputStream(bad), target, null) }
        assertTrue(failed.exceptionOrNull() is PackException)
        assertTrue(PackIndex(target).has("😀"))
    }

    @Test fun emojiRunsFindWholeEmoji() {
        val text = "hi 👋🏽 and ❤️, 🇩🇪 1 © 👨‍💻"
        val found = emojiRuns(text).map { text.substring(it.first, it.last + 1) }
        assertEquals(
            listOf("👋🏽", "❤️", "🇩🇪", "👨‍💻"),
            found,
        )
        assertTrue(emojiRuns("no emoji here").isEmpty())
    }

    @Test fun storedLibraryRoundTrips() {
        val list = listOf(CustomTheme("a", ":root{--ink:#fff}", "https://e.org/a.css"), CustomTheme("b", "x"))
        assertEquals(list, ThemeLibrary.decode(ThemeLibrary.encode(list)))
        assertEquals(emptyList<CustomTheme>(), ThemeLibrary.decode("not json"))
    }

    /** A gzip tar with the given text files, as `npm pack` makes it. */
    private fun tarGz(vararg files: Pair<String, String>): ByteArray {
        val tar = ByteArrayOutputStream()
        for ((name, text) in files) {
            val data = text.toByteArray()
            val header = ByteArray(512)
            name.toByteArray().copyInto(header)
            "0000644".toByteArray().copyInto(header, 100)
            String.format("%011o", data.size).toByteArray().copyInto(header, 124)
            header[156] = '0'.code.toByte()
            tar.write(header)
            tar.write(data)
            tar.write(ByteArray((512 - data.size % 512) % 512))
        }
        tar.write(ByteArray(1024))
        return ByteArrayOutputStream().also { out -> GZIPOutputStream(out).use { it.write(tar.toByteArray()) } }.toByteArray()
    }
}

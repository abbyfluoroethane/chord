package space.foid.chord.ui.theme

import androidx.compose.ui.graphics.Color
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.math.abs

class ThemeCssTest {
    /** Equal within one step of 8 bit colour: mixes are floats. */
    private fun near(name: String, want: Color, got: Color) {
        val d = 1.01f / 255f
        assertTrue(
            "$name: want $want got $got",
            abs(want.red - got.red) < d && abs(want.green - got.green) < d &&
                abs(want.blue - got.blue) < d && abs(want.alpha - got.alpha) < d,
        )
    }

    private fun assertColors(want: ChordColors, got: ChordColors) {
        near("surface100", want.surface100, got.surface100)
        near("surface200", want.surface200, got.surface200)
        near("surface300", want.surface300, got.surface300)
        near("surfaceRail", want.surfaceRail, got.surfaceRail)
        near("surfaceSide", want.surfaceSide, got.surfaceSide)
        near("surfaceRaised", want.surfaceRaised, got.surfaceRaised)
        near("line", want.line, got.line)
        near("lineStrong", want.lineStrong, got.lineStrong)
        near("ink", want.ink, got.ink)
        near("inkMuted", want.inkMuted, got.inkMuted)
        near("brand", want.brand, got.brand)
        near("brandSoft", want.brandSoft, got.brandSoft)
        near("onBrand", want.onBrand, got.onBrand)
        near("brandInk", want.brandInk, got.brandInk)
        near("accent", want.accent, got.accent)
        near("online", want.online, got.online)
        near("away", want.away, got.away)
        near("danger", want.danger, got.danger)
        near("onDanger", want.onDanger, got.onDanger)
        near("scrim", want.scrim, got.scrim)
        assertEquals("isDark", want.isDark, got.isDark)
    }

    @Test fun chordDarkCssGivesTheHandWrittenPalette() =
        assertColors(ChordDark, TestThemes.entry("chord-dark").info.colors())

    @Test fun chordLightCssGivesTheHandWrittenPalette() =
        assertColors(ChordLight, TestThemes.entry("chord-light").info.colors())

    @Test fun headerFieldsAndModes() {
        val dark = TestThemes.entry("chord-dark").info
        assertEquals("Chord Dark", dark.name)
        assertEquals("Chord", dark.author)
        assertTrue(dark.dark)
        assertEquals(listOf("amber"), dark.accents.map { it.id })
        assertEquals("Amber", dark.accents[0].name)
        assertFalse(TestThemes.entry("chord-light").info.dark)
        assertTrue(TestThemes.entry("catppuccin-mocha").info.dark)
        assertFalse(TestThemes.entry("catppuccin-latte").info.dark)
    }

    @Test fun mochaHasFourteenAccentsInHeaderOrder() {
        val info = TestThemes.entry("catppuccin-mocha").info
        assertEquals(14, info.accents.size)
        assertEquals("rosewater", info.accents.first().id)
        assertEquals("lavender", info.accents.last().id)
        assertEquals("mauve", info.defaultAccent)
        assertEquals(Color(0xFFF5E0DC), info.accents.first().color)
    }

    @Test fun mochaColoursFollowTheCatppuccinPalette() {
        val c = TestThemes.entry("catppuccin-mocha").info.colors()
        assertEquals(Color(0xFF1E1E2E), c.surface100)
        assertEquals(Color(0xFFCBA6F7), c.brand)
        assertEquals(Color(0xFFCBA6F7), c.brandInk) // var(--brand)
        assertEquals(Color(0xFF11111B), c.onBrand)
        // --brand-soft: color-mix(in srgb, var(--brand) 20%, #1e1e2e)
        near("brandSoft", Color(0xFF413956), c.brandSoft)
        // --scrim: rgb(17 17 27 / 0.6)
        assertEquals(Color(17, 17, 27, 153), c.scrim)
    }

    @Test fun anAccentChangesBrandBrandInkAndBrandSoft() {
        val info = TestThemes.entry("catppuccin-mocha").info
        val peach = info.colors("peach")
        assertEquals(Color(0xFFFAB387), peach.brand)
        assertEquals(Color(0xFFFAB387), peach.brandInk)
        assertTrue(peach.brandSoft != info.colors().brandSoft)
        assertEquals(info.colors().surface100, peach.surface100)
    }

    @Test fun lattePicksTheLatteAccent() {
        val c = TestThemes.entry("catppuccin-latte").info.colors("blue")
        assertFalse(c.isDark)
        assertEquals(Color(0xFFEFF1F5), c.surface100)
        assertEquals(Color(0xFF1E66F5), c.brand)
    }

    @Test fun accentOrFallsBackToTheDefault() {
        val info = TestThemes.entry("catppuccin-mocha").info
        assertEquals("teal", info.accentOr("teal"))
        assertEquals("mauve", info.accentOr("nope"))
        assertEquals("mauve", info.accentOr(null))
    }

    @Test fun colorMixOfTwoColours() {
        val c = evalColor("color-mix(in srgb, #ffffff 50%, #000000)", emptyMap(), 0)!!
        assertEquals(0.5f, c.red, 0.002f)
        // The second percentage is the rest.
        val d = evalColor("color-mix(in srgb, #ff0000 25%, #0000ff)", emptyMap(), 0)!!
        assertEquals(0.25f, d.red, 0.002f)
        assertEquals(0.75f, d.blue, 0.002f)
    }

    @Test fun rgbFormats() {
        assertEquals(Color(17, 17, 27, 153), evalColor("rgb(17 17 27 / 0.6)", emptyMap(), 0))
        assertEquals(Color(1, 2, 3), evalColor("rgb(1, 2, 3)", emptyMap(), 0))
        assertEquals(Color(1, 2, 3, 128), evalColor("rgba(1, 2, 3, 0.5)", emptyMap(), 0))
        assertEquals(Color(0xFF, 0xAA, 0x00), evalColor("#fa0", emptyMap(), 0))
        assertNull(evalColor("hsl(20 50% 50%)", emptyMap(), 0))
        assertNull(evalColor("#12", emptyMap(), 0))
    }

    @Test fun varWithFallbackAndLoops() {
        val vars = mapOf("--a" to "var(--b)", "--b" to "#102030", "--loop" to "var(--loop)")
        assertEquals(Color(0x10, 0x20, 0x30), evalColor("var(--a)", vars, 0))
        assertEquals(Color(1, 2, 3), evalColor("var(--missing, #010203)", vars, 0))
        assertNull(evalColor("var(--loop)", vars, 0))
    }

    @Test fun themeWithoutModeReadsTheSurfaceLuminance() {
        val light = parseTheme(":root { --surface-100: #f0f0f0; --ink: #111111; }")
        assertFalse(light.dark)
        val dark = parseTheme(":root { --surface-100: #101010; }")
        assertTrue(dark.dark)
        assertEquals("Imported theme", dark.name)
    }

    @Test fun missingTokensFallBackToTheChordPaletteOfTheMode() {
        val c = parseTheme("/** @mode light */ :root { --brand: #123456; }").colors()
        assertEquals(Color(0xFF123456), c.brand)
        assertEquals(ChordLight.surface100, c.surface100)
        assertEquals(ChordLight.danger, c.danger)
    }

    @Test fun badValuesAreIgnored() {
        val c = parseTheme("/** @mode dark */ :root { --brand: banana; --ink: #fff }").colors()
        assertEquals(ChordDark.brand, c.brand)
        assertEquals(Color.White, c.ink)
    }

    @Test fun modeRuleOnlyAppliesToItsMode() {
        val css = "/** @mode light */ :root { --brand: #111111; } :root[data-mode='light'] { --brand: #222222; } :root[data-mode='dark'] { --brand: #333333; }"
        assertEquals(Color(0xFF222222), parseTheme(css).colors().brand)
    }

    @Test fun headerAccentWithoutRuleUsesTheThemeBrand() {
        val info = parseTheme("/**\n * @accent red Red\n * @accent blue\n */ :root { --brand: #aa0000; }")
        assertEquals(listOf("red", "blue"), info.accents.map { it.id })
        assertEquals("Blue", info.accents[1].name)
        assertEquals(Color(0xFFAA0000), info.accents[0].color)
    }

    @Test fun declarationsKeepSemicolonsInsideParentheses() {
        val d = declarations("--a: url(data:image/png;base64,xx); --b: #fff; color: red")
        assertEquals(setOf("--a", "--b"), d.keys)
    }

    // ---- import checks

    @Test fun importChecks() {
        assertEquals(ImportError.Empty, checkThemeCss("  ").second)
        assertEquals(ImportError.TooBig, checkThemeCss("x".repeat(MAX_THEME_CHARS + 1)).second)
        assertEquals(ImportError.NoColours, checkThemeCss("body { color: red }").second)
        assertNotNull(checkThemeCss(":root { --surface-100: #000; }").first)
    }

    @Test fun themeUrls() {
        assertEquals("https://example.com/t.css", normalizeThemeUrl(" https://example.com/t.css#x ").first)
        assertEquals(ImportError.NotHttps, normalizeThemeUrl("http://example.com/t.css").second)
        assertEquals(ImportError.BadLink, normalizeThemeUrl("not a link").second)
        assertEquals(ImportError.BadLink, normalizeThemeUrl("https://user:pw@example.com/t.css").second)
        assertEquals(
            "https://raw.githubusercontent.com/u/r/main/theme.css",
            normalizeThemeUrl("https://github.com/u/r/blob/main/theme.css").first,
        )
        assertEquals("example.com", hostOf("https://www.example.com/a"))
    }

    // ---- library

    @Test fun libraryImportsUsesAndRemoves() {
        var saved = emptyList<CustomTheme>()
        var n = 0
        val lib = ThemeLibrary(TestThemes.bundled(), persist = { saved = it }, newId = { "custom-${++n}" })
        assertEquals(4, lib.entries.value.size)
        val r = lib.import("/** @name Mine\n * @mode light */ :root { --surface-100: #fff; --brand: #f00; }")
        r as ImportResult.Added
        assertEquals("Mine", r.entry.info.name)
        assertEquals(5, lib.entries.value.size)
        assertEquals(1, saved.size)
        // The same link gives the new CSS to the same card.
        lib.import(":root { --surface-100: #000; }", url = "https://e.org/a.css")
        lib.import(":root { --surface-100: #111; }", url = "https://e.org/a.css")
        assertEquals(6, lib.entries.value.size)
        lib.remove("custom-1")
        assertEquals(5, lib.entries.value.size)
        assertTrue(lib.import("nothing here") is ImportResult.Failed)
    }

    @Test fun resolveThemeFallsBackToChord() {
        val all = TestThemes.bundled()
        assertEquals("catppuccin-mocha", resolveTheme(all, "catppuccin-mocha", true).id)
        // A light theme id in the dark slot, and a missing id, give Chord.
        assertEquals("chord-dark", resolveTheme(all, "catppuccin-latte", true).id)
        assertEquals("chord-light", resolveTheme(all, "gone", false).id)
    }
}

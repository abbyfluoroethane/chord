package space.foid.chord.ui.text

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

class MessageFormatterTest {
    private val mentionBg = Color(0xFFFBE7C4)
    private val palette = FormatPalette(
        link = Color.Blue,
        muted = Color.Gray,
        mentionBackground = mentionBg,
        mentionInk = Color.Red,
        codeBackground = Color.LightGray,
    )

    private fun fmt(body: String, names: List<String> = emptyList(), actor: String = "") =
        formatMessage(body, palette, names, actor)

    private fun para(body: String, names: List<String> = emptyList()): AnnotatedString =
        (fmt(body, names).blocks.single() as TextBlock.Paragraph).text

    private fun links(text: AnnotatedString): List<Pair<String, String>> =
        text.getLinkAnnotations(0, text.length).map {
            (it.item as LinkAnnotation.Url).url to text.substring(it.start, it.end)
        }

    private fun AnnotatedString.styled(pick: (androidx.compose.ui.text.SpanStyle) -> Boolean): List<String> =
        spanStyles.filter { pick(it.item) }.map { text.substring(it.start, it.end) }

    private fun AnnotatedString.bold() = styled { it.fontWeight == FontWeight.Bold }
    private fun AnnotatedString.italic() = styled { it.fontStyle == FontStyle.Italic }
    private fun AnnotatedString.strike() = styled { it.textDecoration == TextDecoration.LineThrough }
    private fun AnnotatedString.under() = styled { it.textDecoration == TextDecoration.Underline && it.color == Color.Unspecified }
    private fun AnnotatedString.code() = styled { it.background == Color.LightGray }
    private fun AnnotatedString.mentions() = styled { it.background == mentionBg }

    // ---- plain text

    @Test fun plainTextStaysPlain() {
        val t = para("hello world")
        assertEquals("hello world", t.text)
        assertTrue(t.spanStyles.isEmpty())
        assertTrue(links(t).isEmpty())
    }

    @Test fun lineBreaksStay() = assertEquals("a\nb", para("a\nb").text)

    @Test fun crLfBecomesLf() = assertEquals("a\nb", para("a\r\nb").text)

    @Test fun emptyBodyHasNoBlocks() {
        assertTrue(fmt("").blocks.isEmpty())
        assertTrue(fmt("  \n ").blocks.isEmpty())
    }

    // ---- links

    @Test fun httpsLink() {
        val t = para("see https://example.com/a?b=1 now")
        assertEquals(listOf("https://example.com/a?b=1" to "https://example.com/a?b=1"), links(t))
        assertEquals("see https://example.com/a?b=1 now", t.text)
    }

    @Test fun httpLink() = assertEquals(1, links(para("http://example.com")).size)

    @Test fun schemeIsCaseInsensitive() = assertEquals(1, links(para("HTTPS://EXAMPLE.COM/x")).size)

    @Test fun trailingPunctuationIsNotPartOfTheLink() {
        for (p in listOf(".", ",", "!", "?", ";", ":", ")", "\"", "'")) {
            assertEquals("https://example.com/x", links(para("go to https://example.com/x$p")).single().first)
        }
        assertEquals("https://example.com/x", links(para("(https://example.com/x)")).single().first)
        assertEquals("https://example.com/x", links(para("https://example.com/x...")).single().first)
        assertEquals("https://example.com/x", links(para("https://example.com/x?!")).single().first)
    }

    @Test fun balancedParenthesisStaysInTheLink() {
        assertEquals(
            "https://en.wikipedia.org/wiki/Foo_(bar)",
            links(para("https://en.wikipedia.org/wiki/Foo_(bar)")).single().first,
        )
        assertEquals(
            "https://en.wikipedia.org/wiki/Foo_(bar)",
            links(para("(https://en.wikipedia.org/wiki/Foo_(bar))")).single().first,
        )
    }

    @Test fun linkInsideQuotesAndBrackets() {
        assertEquals("https://a.example", links(para("\"https://a.example\"")).single().first)
        assertEquals("https://a.example", links(para("<https://a.example>")).single().first)
        assertEquals("<https://a.example>".length - 2, links(para("<https://a.example>")).single().second.length)
    }

    @Test fun schemeAloneIsNoLink() {
        assertTrue(links(para("https://")).isEmpty())
        assertTrue(links(para("http:// nothing")).isEmpty())
    }

    @Test fun otherSchemesAreNoLinks() {
        assertTrue(links(para("ftp://example.com")).isEmpty())
        assertTrue(links(para("javascript:alert(1)")).isEmpty())
        assertTrue(links(para("www.example.com")).isEmpty())
    }

    @Test fun linkMustStartAtAWordBoundary() {
        assertTrue(links(para("nothttps://example.com")).isEmpty())
    }

    @Test fun twoLinksOnOneLine() {
        val l = links(para("https://a.example and https://b.example."))
        assertEquals(listOf("https://a.example", "https://b.example"), l.map { it.first })
    }

    @Test fun linkHasTheLinkColourAndUnderline() {
        val t = para("https://example.com")
        val style = (t.getLinkAnnotations(0, t.length).single().item as LinkAnnotation.Url).styles!!.style!!
        assertEquals(Color.Blue, style.color)
        assertEquals(TextDecoration.Underline, style.textDecoration)
    }

    @Test fun xmppUriWithJoin() {
        val l = links(para("join xmpp:room@conference.example.org?join now"))
        assertEquals("xmpp:room@conference.example.org?join", l.single().first)
    }

    @Test fun xmppUriWithTrailingDot() {
        assertEquals("xmpp:room@example.org?join", links(para("Use xmpp:room@example.org?join.")).single().first)
    }

    @Test fun xmppJidOnly() = assertEquals("xmpp:alice@example.org", links(para("xmpp:alice@example.org")).single().first)

    @Test fun xmppWithoutAddressIsNoLink() {
        assertTrue(links(para("xmpp:")).isEmpty())
        assertTrue(links(para("xmpp: hello")).isEmpty())
    }

    @Test fun maskedLinkShowsTheLabel() {
        val t = para("[the docs](https://example.com/docs)")
        assertEquals("the docs", t.text)
        assertEquals("https://example.com/docs" to "the docs", links(t).single())
    }

    @Test fun maskedLinkWithBadSchemeStaysText() {
        val t = para("[x](javascript:alert(1))")
        assertEquals("[x](javascript:alert(1))", t.text)
        assertTrue(links(t).isEmpty())
    }

    @Test fun maskedLinkWithAnotherHostShowsTheRealHost() {
        val t = para("[https://bank.example](https://evil.example/x)")
        assertTrue(t.text.endsWith("(evil.example)"))
    }

    @Test fun maskedLinkWithSameHostShowsNoNote() {
        assertEquals("example.com", para("[example.com](https://www.example.com/x)").text)
    }

    @Test fun maskedMismatchRules() {
        assertTrue(maskedMismatch("https://a.example", "https://b.example"))
        assertTrue(maskedMismatch("a.example", "https://b.example/x"))
        assertFalse(maskedMismatch("click here", "https://b.example"))
        assertFalse(maskedMismatch("a.example", "https://a.example/page"))
        assertFalse(maskedMismatch("https://www.a.example", "https://a.example"))
    }

    @Test fun linkInsideEmphasisKeepsBoth() {
        val t = para("**see https://example.com**")
        assertEquals(1, links(t).size)
        assertEquals(listOf("see https://example.com"), t.bold())
    }

    @Test fun markersInsideALinkAreNotEmphasis() {
        val t = para("https://example.com/a_b_c and https://example.com/*x*")
        assertEquals(2, links(t).size)
        assertTrue(t.italic().isEmpty())
    }

    @Test fun linkInCodeIsNoLink() {
        val t = para("`https://example.com`")
        assertTrue(links(t).isEmpty())
        assertEquals("https://example.com", t.text)
    }

    // ---- emphasis

    @Test fun bold() {
        val t = para("a **b** c")
        assertEquals("a b c", t.text)
        assertEquals(listOf("b"), t.bold())
    }

    @Test fun italicWithStar() = assertEquals(listOf("b"), para("a *b* c").italic())

    @Test fun italicWithUnderscore() = assertEquals(listOf("b"), para("a _b_ c").italic())

    @Test fun underlineWithDoubleUnderscore() = assertEquals(listOf("b"), para("a __b__ c").under())

    @Test fun strikeWithDoubleTilde() {
        val t = para("a ~~b~~ c")
        assertEquals("a b c", t.text)
        assertEquals(listOf("b"), t.strike())
    }

    @Test fun singleTildeIsText() {
        val t = para("a ~b~ c")
        assertEquals("a ~b~ c", t.text)
        assertTrue(t.strike().isEmpty())
    }

    @Test fun boldItalicWithThreeStars() {
        val t = para("***both***")
        assertEquals("both", t.text)
        assertEquals(listOf("both"), t.bold())
        assertEquals(listOf("both"), t.italic())
    }

    @Test fun nestedEmphasis() {
        val t = para("**bold and _italic_ here**")
        assertEquals("bold and italic here", t.text)
        assertEquals(listOf("bold and italic here"), t.bold())
        assertEquals(listOf("italic"), t.italic())
    }

    @Test fun nestedStrikeInBold() {
        val t = para("**a ~~b~~ c**")
        assertEquals("a b c", t.text)
        assertEquals(listOf("b"), t.strike())
    }

    @Test fun unclosedBoldStaysText() {
        val t = para("a **b c")
        assertEquals("a **b c", t.text)
        assertTrue(t.spanStyles.isEmpty())
    }

    @Test fun unclosedItalicStaysText() {
        val t = para("a *b c")
        assertEquals("a *b c", t.text)
        assertTrue(t.italic().isEmpty())
    }

    @Test fun unclosedMarkerDoesNotStopLaterEmphasis() {
        val t = para("**a _x_")
        assertEquals("**a x", t.text)
        assertEquals(listOf("x"), t.italic())
    }

    @Test fun emptyEmphasisIsText() {
        assertEquals("a ** b", para("a ** b").text)
    }

    @Test fun italicDoesNotTakeSpacesAtTheEdges() {
        val t = para("2 * 3 * 4")
        assertEquals("2 * 3 * 4", t.text)
        assertTrue(t.italic().isEmpty())
    }

    @Test fun underscoreInsideAWordIsText() {
        val t = para("snake_case_name and other_thing")
        assertEquals("snake_case_name and other_thing", t.text)
        assertTrue(t.italic().isEmpty())
    }

    @Test fun underscoreCloserBeforeALetterIsText() {
        assertTrue(para("_a_b").italic().isEmpty())
    }

    @Test fun escapedMarkerIsText() {
        val t = para("\\*not italic\\*")
        assertEquals("*not italic*", t.text)
        assertTrue(t.italic().isEmpty())
    }

    @Test fun backslashBeforeALetterStays() = assertEquals("a\\b", para("a\\b").text)

    @Test fun emphasisAcrossLines() {
        val t = para("**a\nb**")
        assertEquals("a\nb", t.text)
        assertEquals(listOf("a\nb"), t.bold())
    }

    @Test fun spoilerBarsStayText() = assertEquals("||x||", para("||x||").text)

    // ---- code

    @Test fun inlineCode() {
        val t = para("run `ls -la` now")
        assertEquals("run ls -la now", t.text)
        assertEquals(listOf("ls -la"), t.code())
    }

    @Test fun inlineCodeUsesTheMonoFont() {
        val t = para("`x`")
        assertEquals(palette.mono, t.spanStyles.single().item.fontFamily)
    }

    @Test fun codeKeepsMarkers() {
        val t = para("`**not bold** _x_ ~~y~~ @me`", listOf("me"))
        assertEquals("**not bold** _x_ ~~y~~ @me", t.text)
        assertTrue(t.bold().isEmpty())
        assertTrue(t.italic().isEmpty())
        assertTrue(t.strike().isEmpty())
        assertTrue(t.mentions().isEmpty())
    }

    @Test fun emphasisCannotCloseInsideCode() {
        val t = para("**a `b**` c")
        assertEquals("**a b** c", t.text)
        assertTrue(t.bold().isEmpty())
    }

    @Test fun doubleBacktickCodeCanHoldABacktick() {
        val t = para("``a`b``")
        assertEquals("a`b", t.text)
        assertEquals(listOf("a`b"), t.code())
    }

    @Test fun unclosedBacktickStaysText() {
        val t = para("a `b c")
        assertEquals("a `b c", t.text)
        assertTrue(t.code().isEmpty())
    }

    @Test fun fencedCodeBlock() {
        val f = fmt("before\n```\nval a = **1**\nval b = 2\n```\nafter")
        assertEquals(3, f.blocks.size)
        assertEquals("before", ((f.blocks[0]) as TextBlock.Paragraph).text.text)
        val code = f.blocks[1] as TextBlock.Code
        assertEquals("val a = **1**\nval b = 2", code.code)
        assertEquals("", code.lang)
        assertEquals("after", (f.blocks[2] as TextBlock.Paragraph).text.text)
    }

    @Test fun fencedCodeBlockWithLanguage() {
        val code = fmt("```Kotlin\nfun x() {}\n```").blocks.single() as TextBlock.Code
        assertEquals("kotlin", code.lang)
        assertEquals("fun x() {}", code.code)
    }

    @Test fun fencedCodeOnOneLine() {
        val code = fmt("```pre```").blocks.single() as TextBlock.Code
        assertEquals("pre", code.code)
    }

    @Test fun unclosedFenceIsText() {
        val f = fmt("```\nnot closed")
        assertTrue(f.blocks.single() is TextBlock.Paragraph)
    }

    @Test fun codeBlockKeepsLinksAsText() {
        val code = fmt("```\nhttps://example.com\n```").blocks.single() as TextBlock.Code
        assertEquals("https://example.com", code.code)
    }

    @Test fun codeBlockKeepsBlankLinesAndIndent() {
        val code = fmt("```\na\n\n    b\n```").blocks.single() as TextBlock.Code
        assertEquals("a\n\n    b", code.code)
    }

    // ---- quotes

    @Test fun singleLineQuote() {
        val q = fmt("> hello").blocks.single() as TextBlock.Quote
        assertEquals("hello", (q.blocks.single() as TextBlock.Paragraph).text.text)
    }

    @Test fun multiLineQuoteThenText() {
        val f = fmt("> a\n> b\nc")
        val q = f.blocks[0] as TextBlock.Quote
        assertEquals("a\nb", (q.blocks.single() as TextBlock.Paragraph).text.text)
        assertEquals("c", (f.blocks[1] as TextBlock.Paragraph).text.text)
    }

    @Test fun quoteHasStyling() {
        val q = fmt("> **bold** https://example.com").blocks.single() as TextBlock.Quote
        val t = (q.blocks.single() as TextBlock.Paragraph).text
        assertEquals(listOf("bold"), t.bold())
        assertEquals(1, links(t).size)
    }

    @Test fun tripleQuoteTakesTheRest() {
        val f = fmt(">>> a\nb\nc")
        val q = f.blocks.single() as TextBlock.Quote
        assertEquals("a\nb\nc", (q.blocks.single() as TextBlock.Paragraph).text.text)
    }

    @Test fun greaterThanWithoutSpaceIsText() {
        assertEquals(">no", para(">no").text)
    }

    @Test fun greaterThanMidLineIsText() {
        assertEquals("a > b", para("a > b").text)
    }

    @Test fun quotedCodeBlock() {
        val q = fmt("> ```\n> x\n> ```").blocks.single() as TextBlock.Quote
        assertEquals("x", (q.blocks.single() as TextBlock.Code).code)
    }

    @Test fun quoteDoesNotNest() {
        val q = fmt("> > x").blocks.single() as TextBlock.Quote
        assertEquals("> x", (q.blocks.single() as TextBlock.Paragraph).text.text)
    }

    // ---- headings and lists

    @Test fun headingIsBigAndBold() {
        val t = para("# Title")
        assertEquals("Title", t.text)
        assertEquals(24f, t.spanStyles.first().item.fontSize.value)
    }

    @Test fun hashWithoutSpaceIsText() = assertEquals("#tag", para("#tag").text)

    @Test fun bulletList() {
        val t = para("- one\n- two")
        assertEquals("• one\n• two", t.text)
    }

    @Test fun numberedListKeepsStart() {
        assertEquals("3. a\n4. b", para("3. a\n4. b").text)
    }

    @Test fun nestedList() {
        assertEquals("• a\n  • b\n• c", para("- a\n  - b\n- c").text)
    }

    @Test fun listItemsHaveStyling() {
        assertEquals(listOf("x"), para("- **x**").bold())
    }

    // ---- mentions

    @Test fun ownMentionIsHighlighted() {
        val t = para("hi @abby, ok?", listOf("abby"))
        assertEquals(listOf("@abby"), t.mentions())
        assertEquals(Color.Red, t.spanStyles.first { it.item.background == mentionBg }.item.color)
    }

    @Test fun mentionIgnoresCase() = assertEquals(listOf("@ABBY"), para("@ABBY hi", listOf("abby")).mentions())

    @Test fun mentionAtTheStartAndEnd() {
        assertEquals(listOf("@abby"), para("@abby", listOf("abby")).mentions())
        assertEquals(listOf("@abby"), para("hello @abby", listOf("abby")).mentions())
    }

    @Test fun mentionOfAnotherNickIsNotHighlighted() {
        assertTrue(para("hi @bob", listOf("abby")).mentions().isEmpty())
    }

    @Test fun mentionNeedsAWordBoundaryAfter() {
        assertTrue(para("@abby2 hi", listOf("abby")).mentions().isEmpty())
        assertTrue(para("@abby_x hi", listOf("abby")).mentions().isEmpty())
        assertTrue(para("@abbyé hi", listOf("abby")).mentions().isEmpty())
    }

    @Test fun mentionNeedsAWordBoundaryBefore() {
        assertTrue(para("mail@abby.example", listOf("abby")).mentions().isEmpty())
        assertTrue(para("x@abby", listOf("abby")).mentions().isEmpty())
        assertTrue(para("a.@abby", listOf("abby")).mentions().isEmpty())
    }

    @Test fun mentionWithoutAtIsNotHighlighted() {
        assertTrue(para("abby is here", listOf("abby")).mentions().isEmpty())
    }

    @Test fun mentionFollowedByPunctuation() {
        for (p in listOf(",", ".", "!", "?", ":", ")")) {
            assertEquals(listOf("@abby"), para("@abby$p", listOf("abby")).mentions())
        }
    }

    @Test fun mentionOfNickWithASpace() {
        assertEquals(listOf("@Abby B"), para("hey @Abby B!", listOf("Abby B")).mentions())
    }

    @Test fun longestOwnNameWins() {
        assertEquals(listOf("@abby.b"), para("@abby.b hi", listOf("abby", "abby.b")).mentions())
    }

    @Test fun mentionByAddress() {
        assertEquals(
            listOf("@abby@example.org"),
            para("@abby@example.org hi", listOf("abby@example.org", "abby")).mentions(),
        )
    }

    @Test fun mentionInsideBoldKeepsBoth() {
        val t = para("**hi @abby**", listOf("abby"))
        assertEquals(listOf("@abby"), t.mentions())
        assertEquals(listOf("hi @abby"), t.bold())
    }

    @Test fun mentionInsideCodeIsNotHighlighted() {
        assertTrue(para("`@abby`", listOf("abby")).mentions().isEmpty())
    }

    @Test fun blankOwnNamesAreIgnored() {
        assertTrue(para("@ hi", listOf("", " ")).mentions().isEmpty())
    }

    @Test fun mentionInsideALinkIsNoMention() {
        val t = para("https://example.com/@abby", listOf("abby"))
        assertTrue(t.mentions().isEmpty())
    }

    // ---- /me

    @Test fun meActionHasAPrefix() {
        val f = fmt("/me waves", actor = "Alice")
        assertTrue(f.action)
        assertEquals("* Alice waves", (f.blocks.single() as TextBlock.Paragraph).text.text)
    }

    @Test fun meActorIsBold() {
        val t = (fmt("/me waves", actor = "Alice").blocks.single() as TextBlock.Paragraph).text
        assertEquals(listOf("Alice"), t.styled { it.fontWeight == FontWeight.SemiBold })
    }

    @Test fun meActionWithStyling() {
        val t = (fmt("/me waves **hard** at https://example.com", actor = "Al").blocks.single() as TextBlock.Paragraph).text
        assertEquals("* Al waves hard at https://example.com", t.text)
        assertEquals(listOf("hard"), t.bold())
        assertEquals(1, links(t).size)
        // The link range moved with the prefix.
        assertEquals("https://example.com", links(t).single().second)
    }

    @Test fun meWithoutTextIsNoAction() {
        assertFalse(fmt("/me").action)
        assertFalse(fmt("/me   ").action)
        assertEquals("/me", para("/me").text)
    }

    @Test fun meNeedsTheSpace() = assertFalse(fmt("/mewaves").action)

    @Test fun meMustStartTheBody() = assertFalse(fmt("hi /me waves").action)

    @Test fun meIsCaseSensitive() = assertFalse(fmt("/ME waves").action)

    @Test fun meWithoutActor() {
        assertEquals("* waves", (fmt("/me waves").blocks.single() as TextBlock.Paragraph).text.text)
    }

    @Test fun actionTextRule() {
        assertEquals("waves", actionText("/me waves"))
        assertEquals("waves", actionText("/me   waves  "))
        assertNull(actionText("/me "))
        assertNull(actionText("hello"))
    }

    // ---- emoji only

    @Test fun oneEmojiIsJumbo() = assertTrue(fmt("😀").jumbo)

    @Test fun severalEmojiAreJumbo() = assertTrue(fmt("😀 😀👍").jumbo)

    @Test fun textWithEmojiIsNotJumbo() = assertFalse(fmt("hi 😀").jumbo)

    @Test fun plainTextIsNotJumbo() = assertFalse(fmt("hello").jumbo)

    @Test fun emptyIsNotJumbo() {
        assertFalse(fmt("").jumbo)
        assertFalse(isEmojiOnly("   "))
    }

    @Test fun digitsAreNotEmoji() {
        assertFalse(isEmojiOnly("123"))
        assertFalse(isEmojiOnly("#"))
        assertFalse(isEmojiOnly("*"))
    }

    @Test fun keycapIsEmoji() {
        assertTrue(isEmojiOnly("1️⃣"))
        assertTrue(isEmojiOnly("#️⃣"))
    }

    @Test fun flagIsEmoji() {
        assertTrue(isEmojiOnly("🇨🇦"))
        // One regional indicator alone is no flag.
        assertFalse(isEmojiOnly("🇨"))
    }

    @Test fun skinToneIsOneEmoji() = assertTrue(isEmojiOnly("👍🏽"))

    @Test fun zwjSequenceIsOneEmoji() {
        // Family: man ZWJ woman ZWJ girl.
        val family = "👨‍👩‍👧"
        assertTrue(isEmojiOnly(family))
        assertTrue(fmt(family).jumbo)
    }

    @Test fun variationSelectorIsPartOfTheEmoji() = assertTrue(isEmojiOnly("❤️"))

    @Test fun thirtyEmojiAreJumboAndThirtyOneAreNot() {
        assertTrue(isEmojiOnly("😀".repeat(30)))
        assertFalse(isEmojiOnly("😀".repeat(31)))
    }

    @Test fun lineBreakBetweenEmojiIsStillEmojiOnly() = assertTrue(isEmojiOnly("😀\n😀"))

    @Test fun emojiWithALinkIsNotJumbo() = assertFalse(fmt("😀 https://example.com").jumbo)

    @Test fun emojiInAQuoteIsNotJumbo() = assertFalse(fmt("> 😀").jumbo)

    @Test fun emojiInCodeIsNotJumbo() = assertFalse(fmt("```\n😀\n```").jumbo)

    @Test fun emojiActionIsJumbo() {
        // The desktop shows the action text with the same rule.
        assertTrue(isEmojiOnly("😀"))
    }

    // ---- cache and misc

    @Test fun sameInputGivesTheSameInstance() {
        val a = fmt("cache me **please**", listOf("x"))
        val b = fmt("cache me **please**", listOf("x"))
        assertSame(a, b)
    }

    @Test fun otherNamesGiveAnotherResult() {
        val a = fmt("@abby", listOf("abby"))
        val b = fmt("@abby", listOf("bob"))
        assertTrue(a !== b)
        assertEquals(1, (a.blocks.single() as TextBlock.Paragraph).text.mentions().size)
        assertEquals(0, (b.blocks.single() as TextBlock.Paragraph).text.mentions().size)
    }

    @Test fun otherPaletteGivesAnotherResult() {
        val a = fmt("x")
        val b = formatMessage("x", palette.copy(link = Color.Green))
        assertTrue(a !== b)
    }

    @Test fun longBodyIsFast() {
        val body = ("word **bold** _it_ https://example.com/x `c` @abby ".repeat(2000))
        val start = System.nanoTime()
        formatMessage(body, palette, listOf("abby"))
        val ms = (System.nanoTime() - start) / 1_000_000
        assertTrue("took $ms ms", ms < 2000)
    }

    @Test fun manyUnclosedMarkersAreFast() {
        val body = "*a ".repeat(3000)
        val start = System.nanoTime()
        formatMessage(body, palette)
        val ms = (System.nanoTime() - start) / 1_000_000
        assertTrue("took $ms ms", ms < 5000)
    }

    @Test fun isLinkableRules() {
        assertTrue(isLinkable("https://a.example"))
        assertTrue(isLinkable("xmpp:a@b.example?join"))
        assertFalse(isLinkable("ftp://a.example"))
        assertFalse(isLinkable("javascript:alert(1)"))
        assertNotNull(para("x"))
    }
}

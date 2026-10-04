package space.foid.chord.ui.text

import androidx.compose.runtime.Immutable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLinkStyles
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.em
import androidx.compose.ui.unit.sp

// The message text rules of the desktop (chord-desktop/src/lib/ui/markdown.ts and action.ts),
// ported to Kotlin. The desktop uses Discord chat markdown, not XEP-0393:
//   *italic*  **bold**  ***bold italic***  _italic_  __underline__  ~~strike~~  `code`
//   ```pre```  "> quote"  ">>> quote to the end"  "# heading"  "-# small"  "- list"  "1. list"
//   [text](url)  <url>  bare http(s) and xmpp: links  @mention  "/me action"  emoji-only = large.
// Not ported: ||spoiler|| (it shows as plain text), <t:...> time stamps and :shortcodes:.
// The function is pure. It uses no FFI and no Android class.

/** The theme colours and the mono font that a formatted text needs. */
@Immutable
data class FormatPalette(
    val link: Color,
    val muted: Color,
    val mentionBackground: Color,
    val mentionInk: Color,
    val codeBackground: Color,
    val mono: FontFamily = FontFamily.Monospace,
)

/** One block of a message. */
sealed interface TextBlock {
    /** A run of text with its styles and links. A heading or a list is a paragraph too. */
    class Paragraph(val text: AnnotatedString) : TextBlock

    /** A quote. It can hold paragraphs and code blocks, but no quote. */
    class Quote(val blocks: List<TextBlock>) : TextBlock

    /** A fenced code block. [lang] is lower case, or empty. */
    class Code(val code: String, val lang: String) : TextBlock
}

/**
 * A message body after formatting. It has no equals on purpose: [formatMessage] returns the same
 * instance for the same input, so equal messages compare equal by reference.
 */
@Immutable
class FormattedMessage(
    val blocks: List<TextBlock>,
    /** Only 1 to 30 emoji. Show them large. */
    val jumbo: Boolean,
    /** A "/me" action. Show it in italic and muted. */
    val action: Boolean,
)

private class Key(val body: String, val names: List<String>, val actor: String, val palette: FormatPalette) {
    override fun equals(other: Any?) =
        other is Key && body == other.body && names == other.names && actor == other.actor && palette == other.palette
    override fun hashCode() = ((body.hashCode() * 31 + names.hashCode()) * 31 + actor.hashCode()) * 31 + palette.hashCode()
}

private const val CACHE_SIZE = 512

private val cache = object : LinkedHashMap<Key, FormattedMessage>(64, 0.75f, true) {
    override fun removeEldestEntry(eldest: MutableMap.MutableEntry<Key, FormattedMessage>) = size > CACHE_SIZE
}

/**
 * Format [body]. The result is cached (an LRU of 512), so the same message is parsed once.
 *
 * @param names what an `@mention` must name to count as the own nick: the room nick, the local
 *   part of the address, the address. The match ignores case.
 * @param actor the sender name, for a "/me" action ("* Alice waves").
 */
fun formatMessage(
    body: String,
    palette: FormatPalette,
    names: List<String> = emptyList(),
    actor: String = "",
): FormattedMessage {
    val key = Key(body, names, actor, palette)
    synchronized(cache) { cache[key]?.let { return it } }
    val made = Formatter(palette, names.filter { it.isNotBlank() }.sortedByDescending { it.length }).format(body, actor)
    synchronized(cache) { cache[key] = made }
    return made
}

/** The text after "/me ", or null if the body is no action. */
fun actionText(body: String): String? {
    if (!body.startsWith("/me ")) return null
    return body.substring(4).trim().ifEmpty { null }
}

// ---------------------------------------------------------------- emoji

/** True if [text] holds 1 to 30 emoji and nothing else but white space. */
fun isEmojiOnly(text: String): Boolean {
    val s = text.filterNot { it.isWhitespace() }
    if (s.isEmpty()) return false
    var i = 0
    var n = 0
    while (i < s.length) {
        val end = emojiEnd(s, i)
        if (end < 0) return false
        i = end
        if (++n > 30) return false
    }
    return n >= 1
}

private fun isPictographic(cp: Int): Boolean = when (cp) {
    0xA9, 0xAE, 0x203C, 0x2049, 0x2122, 0x2139, 0x2328, 0x2388, 0x23CF, 0x24C2, 0x25B6, 0x25C0,
    0x2B50, 0x2B55, 0x3030, 0x303D, 0x3297, 0x3299, 0x1F12F, 0x1F18E, 0x1F21A, 0x1F22F -> true
    else -> cp in 0x2194..0x2199 || cp in 0x21A9..0x21AA || cp in 0x231A..0x231B || cp in 0x23E9..0x23F3 ||
        cp in 0x23F8..0x23FA || cp in 0x25AA..0x25AB || cp in 0x25FB..0x25FE || cp in 0x2600..0x27BF ||
        cp in 0x2934..0x2935 || cp in 0x2B05..0x2B07 || cp in 0x2B1B..0x2B1C || cp in 0x1F000..0x1F0FF ||
        cp in 0x1F10D..0x1F10F || cp in 0x1F16C..0x1F171 || cp in 0x1F17E..0x1F17F || cp in 0x1F191..0x1F19A ||
        cp in 0x1F201..0x1F20F || cp in 0x1F232..0x1F23A || cp in 0x1F23C..0x1F23F || cp in 0x1F249..0x1F3FA ||
        (cp in 0x1F400..0x1FAFF && cp !in 0x1F3FB..0x1F3FF) || cp in 0x1FC00..0x1FFFD
}

private fun isModifier(cp: Int) = cp in 0x1F3FB..0x1F3FF
private fun isRegional(cp: Int) = cp in 0x1F1E6..0x1F1FF

/** The end of the emoji that starts at [i], or -1 if none starts there. */
private fun emojiEnd(s: String, i: Int): Int {
    val cp = s.codePointAt(i)
    var j = i + Character.charCount(cp)
    fun peek(at: Int) = if (at < s.length) s.codePointAt(at) else -1
    if (cp == '#'.code || cp == '*'.code || cp in '0'.code..'9'.code) {
        if (peek(j) == 0xFE0F) j++
        return if (peek(j) == 0x20E3) j + 1 else -1
    }
    if (isRegional(cp)) {
        val next = peek(j)
        return if (isRegional(next)) j + Character.charCount(next) else -1
    }
    if (!isPictographic(cp)) return -1
    fun tail() {
        val p = peek(j)
        if (p == 0xFE0F || isModifier(p)) j += Character.charCount(p)
    }
    tail()
    while (peek(j) == 0x200D) {
        val next = peek(j + 1)
        if (next < 0 || !isPictographic(next)) break
        j += 1 + Character.charCount(next)
        tail()
    }
    while (peek(j) in 0xE0020..0xE007F) j += 2
    return j
}

/**
 * The character ranges (end included) of each emoji in [s]. A symbol that is text by default and
 * has no variation selector (the copyright sign, an arrow) is not an emoji here.
 */
fun emojiRuns(s: String): List<IntRange> {
    val out = ArrayList<IntRange>()
    var i = 0
    while (i < s.length) {
        val end = emojiEnd(s, i)
        if (end < 0) {
            i += Character.charCount(s.codePointAt(i))
            continue
        }
        val single = end - i == Character.charCount(s.codePointAt(i))
        val cp = s.codePointAt(i)
        val textSymbol = single && (cp == 0xA9 || cp == 0xAE || cp == 0x203C || cp == 0x2049 || cp == 0x2122 ||
            cp == 0x2139 || cp in 0x2194..0x21AA)
        if (!textSymbol) out += i until end
        i = end
    }
    return out.map { it.first..it.last }
}

// ---------------------------------------------------------------- links

private val BARE_LINK = Regex("^(?:https?://|xmpp:)[^\\s<>]+", RegexOption.IGNORE_CASE)
private val ANGLE_LINK = Regex("^<((?:https?://|xmpp:)[^\\s<>]+)>", RegexOption.IGNORE_CASE)
private val HTTP_ONLY = Regex("^https?://\\S+$", RegexOption.IGNORE_CASE)
private val HTTP_HEAD = Regex("^https?://[^\\s]", RegexOption.IGNORE_CASE)
private val XMPP_URI = Regex("^xmpp:[^\\s@/?#]+(?:@[^\\s?#]+|\\.[^\\s?#]+)(?:\\?\\S*)?$", RegexOption.IGNORE_CASE)
private const val ESCAPABLE = "\\`*_~|[]()<>#@:!.>+-"
private const val TRAIL = ".,;:!?)\"'*_~|"

private fun isWord(c: Char) = c.isLetterOrDigit()
private fun isWordish(c: Char) = c.isLetterOrDigit() || c == '_'

/** http, https, or an xmpp: URI with an address. */
fun isLinkable(href: String): Boolean = HTTP_ONLY.matches(href) || XMPP_URI.matches(href)

/** True if [href] is an xmpp: URI. */
fun isXmppUri(href: String) = href.startsWith("xmpp:", ignoreCase = true)

/** The URL at the start of [s] from [i], without the punctuation at its end, or null. */
internal fun bareLinkAt(s: String, i: Int): String? {
    val m = BARE_LINK.find(s.substring(i, minOf(s.length, i + 2100))) ?: return null
    var url = m.value
    while (true) {
        val last = url.last()
        val balanced = last == ')' && url.count { it == '(' } >= url.count { it == ')' }
        if (balanced || last !in TRAIL) break
        url = url.dropLast(1)
    }
    return if (isXmppUri(url)) url.takeIf { XMPP_URI.matches(it) }
    else url.takeIf { it.length > 8 && HTTP_HEAD.containsMatchIn(it) }
}

private val HOST = Regex("^https?://(?:[^/?#@]*@)?([^/?#:]+)", RegexOption.IGNORE_CASE)
private val SCHEME = Regex("^https?://", RegexOption.IGNORE_CASE)
private val LOOKS_LIKE_HOST = Regex("^[\\p{L}\\p{N}-]+(\\.[\\p{L}\\p{N}-]+)+(/.*)?$")

private fun hostOf(href: String): String? = HOST.find(href)?.groupValues?.get(1)?.lowercase()

/**
 * A masked link `[https://bank.example](https://evil.example)` shows one address and opens
 * another. True when the label looks like an address of another host than [href].
 */
fun maskedMismatch(label: String, href: String): Boolean {
    val t = label.trim()
    if (t.contains(' ')) return false
    val shown = when {
        SCHEME.containsMatchIn(t) -> hostOf(t)
        LOOKS_LIKE_HOST.matches(t) -> hostOf("https://$t")
        else -> return false
    } ?: return false
    val real = hostOf(href) ?: return true
    return shown.removePrefix("www.") != real.removePrefix("www.")
}

// ---------------------------------------------------------------- parser

private class Formatter(private val palette: FormatPalette, private val names: List<String>) {
    private val mentionStyle = SpanStyle(
        color = palette.mentionInk,
        background = palette.mentionBackground,
        fontWeight = FontWeight.Medium,
    )
    private val linkStyles = TextLinkStyles(SpanStyle(color = palette.link, textDecoration = TextDecoration.Underline))

    fun format(body: String, actor: String): FormattedMessage {
        val text = body.replace("\r\n", "\n").replace('\r', '\n')
        val action = actionText(text)
        val shown = action ?: text
        var blocks = parseBlocks(shown, quotes = true)
        if (action != null) {
            val prefix = AnnotatedString.Builder().apply {
                append("* ")
                if (actor.isNotEmpty()) {
                    withStyle(SpanStyle(fontWeight = FontWeight.SemiBold)) { append(actor) }
                    append(" ")
                }
            }.toAnnotatedString()
            val first = blocks.firstOrNull()
            blocks = if (first is TextBlock.Paragraph) {
                val joined = AnnotatedString.Builder().apply { append(prefix); append(first.text) }.toAnnotatedString()
                listOf<TextBlock>(TextBlock.Paragraph(joined)) + blocks.drop(1)
            } else {
                listOf<TextBlock>(TextBlock.Paragraph(prefix)) + blocks
            }
        }
        val jumbo = isEmojiOnly(shown) && blocks.size == 1 && blocks[0] is TextBlock.Paragraph
        return FormattedMessage(blocks, jumbo, action != null)
    }

    // ---- blocks

    private val listRe = Regex("^(\\s*)([-*]|\\d{1,9}\\.) (.*)$")
    private val headingRe = Regex("^(#{1,3}) (.+)$")
    private val subtextRe = Regex("^-# (.+)$")
    private val fenceHead = Regex("^([A-Za-z0-9_+#.-]*)\\n([\\s\\S]*)$")

    private fun parseBlocks(text: String, quotes: Boolean): List<TextBlock> {
        val blocks = ArrayList<TextBlock>()
        val para = ArrayList<String>()
        fun flush() {
            val joined = para.joinToString("\n").trim('\n')
            para.clear()
            if (joined.isNotBlank()) blocks.add(TextBlock.Paragraph(inlineText(joined)))
        }
        var pos = 0
        while (pos < text.length) {
            val eol = text.indexOf('\n', pos)
            val end = if (eol < 0) text.length else eol
            val line = text.substring(pos, end)

            if (line.startsWith("```")) {
                val close = text.indexOf("```", pos + 3)
                if (close > pos + 3) {
                    var code = text.substring(pos + 3, close)
                    var lang = ""
                    fenceHead.matchEntire(code)?.let {
                        lang = it.groupValues[1]
                        code = it.groupValues[2]
                    }
                    code = code.removeSuffix("\n")
                    if (code.isNotEmpty() || lang.isNotEmpty()) {
                        flush()
                        blocks.add(TextBlock.Code(code, lang.lowercase()))
                        pos = close + 3
                        if (pos < text.length && text[pos] == '\n') pos++
                        continue
                    }
                }
            }
            if (quotes && line.startsWith(">>> ")) {
                flush()
                val rest = line.substring(4) + (if (eol < 0) "" else "\n" + text.substring(eol + 1))
                blocks.add(TextBlock.Quote(parseBlocks(rest, quotes = false)))
                break
            }
            if (quotes && line.startsWith("> ")) {
                flush()
                val inner = ArrayList<String>()
                var p = pos
                while (p < text.length) {
                    val e = text.indexOf('\n', p)
                    val l = text.substring(p, if (e < 0) text.length else e)
                    if (l.startsWith("> ")) inner.add(l.substring(2))
                    else if (l == ">") inner.add("")
                    else break
                    p = if (e < 0) text.length else e + 1
                }
                blocks.add(TextBlock.Quote(parseBlocks(inner.joinToString("\n"), quotes = false)))
                pos = p
                continue
            }
            val heading = headingRe.matchEntire(line)
            val sub = subtextRe.matchEntire(line)
            if (heading != null) {
                flush()
                val size = when (heading.groupValues[1].length) { 1 -> 24.sp; 2 -> 20.sp; else -> 16.sp }
                val b = AnnotatedString.Builder()
                b.withStyle(SpanStyle(fontSize = size, fontWeight = FontWeight.SemiBold)) { inline(heading.groupValues[2], b, false) }
                blocks.add(TextBlock.Paragraph(b.toAnnotatedString()))
            } else if (sub != null) {
                flush()
                val b = AnnotatedString.Builder()
                b.withStyle(SpanStyle(color = palette.muted, fontSize = 12.sp, fontWeight = FontWeight.Medium)) { inline(sub.groupValues[1], b, false) }
                blocks.add(TextBlock.Paragraph(b.toAnnotatedString()))
            } else if (listRe.matches(line)) {
                flush()
                val lines = ArrayList<String>()
                var p = pos
                while (p < text.length) {
                    val e = text.indexOf('\n', p)
                    val l = text.substring(p, if (e < 0) text.length else e)
                    if (!listRe.matches(l)) break
                    lines.add(l)
                    p = if (e < 0) text.length else e + 1
                }
                blocks.add(TextBlock.Paragraph(listText(lines)))
                pos = p
                continue
            } else {
                para.add(line)
            }
            pos = end + 1
        }
        flush()
        return blocks
    }

    /** A list as text: a bullet or the number, and two spaces for each level of depth. */
    private fun listText(lines: List<String>): AnnotatedString {
        val b = AnnotatedString.Builder()
        val levels = ArrayList<Int>()
        val counters = HashMap<Int, Int>()
        lines.forEachIndexed { n, l ->
            val m = listRe.matchEntire(l)!!
            val indent = m.groupValues[1].length
            while (levels.isNotEmpty() && levels.last() > indent) {
                counters.remove(levels.removeAt(levels.lastIndex))
            }
            if (levels.isEmpty() || levels.last() < indent) levels.add(indent)
            val depth = levels.size - 1
            if (n > 0) b.append("\n")
            b.append("  ".repeat(depth))
            val marker = m.groupValues[2]
            if (marker[0].isDigit()) {
                val num = counters[indent]?.plus(1) ?: marker.dropLast(1).toInt()
                counters[indent] = num
                b.append("$num. ")
            } else {
                b.append("\u2022 ")
            }
            inline(m.groupValues[3], b, false)
        }
        return b.toAnnotatedString()
    }

    private fun inlineText(s: String): AnnotatedString {
        val b = AnnotatedString.Builder()
        inline(s, b, false)
        return b.toAnnotatedString()
    }

    // ---- inline

    private fun runLength(s: String, i: Int, c: Char): Int {
        var n = 0
        while (i + n < s.length && s[i + n] == c) n++
        return n
    }

    /** A code span that opens at [i]. The closing run has the same length. Gives text and end. */
    private fun codeSpan(s: String, i: Int): Pair<String, Int>? {
        val n = runLength(s, i, '`')
        var j = i + n
        while (j < s.length) {
            val at = s.indexOf('`', j)
            if (at < 0) return null
            val m = runLength(s, at, '`')
            if (m == n) {
                var v = s.substring(i + n, at)
                if (v.length > 2 && v.startsWith(' ') && v.endsWith(' ') && v.isNotBlank()) v = v.substring(1, v.length - 1)
                return v to at + n
            }
            j = at + m
        }
        return null
    }

    private class Masked(val text: String, val href: String, val end: Int)

    private fun maskedLink(s: String, i: Int): Masked? {
        var depth = 0
        var j = i
        while (j < s.length) {
            val c = s[j]
            if (c == '\\') j++
            else if (c == '[') depth++
            else if (c == ']' && --depth == 0) break
            j++
        }
        if (j + 1 >= s.length || s[j + 1] != '(') return null
        val text = s.substring(i + 1, j)
        var k = j + 2
        var open = 1
        while (k < s.length) {
            if (s[k] == '(') open++
            else if (s[k] == ')' && --open == 0) break
            else if (s[k] == '\n') return null
            k++
        }
        if (k >= s.length || text.isBlank()) return null
        var href = s.substring(j + 2, k).trim()
        href = href.replace(Regex("\\s+\"[^\"]*\"$"), "")
        if (href.startsWith("<") && href.endsWith(">")) href = href.substring(1, href.length - 1)
        if (!isLinkable(href)) return null
        return Masked(text, href, k + 1)
    }

    private fun findCloser(s: String, from: Int, c: Char, n: Int): Int {
        var j = from
        while (j < s.length) {
            val ch = s[j]
            if (ch == '\\') { j += 2; continue }
            if (ch == '`') {
                val span = codeSpan(s, j)
                j = span?.second ?: (j + runLength(s, j, '`'))
                continue
            }
            if (ch != c) { j++; continue }
            val r = runLength(s, j, c)
            var at = -1
            when (n) {
                1 -> if (r == 1) at = j else if (r == 3 && c == '*') at = j + 2
                2 -> if (r == 2 || r > 3) at = j else if (r == 3) at = j + 1
                else -> if (r >= 3) at = j
            }
            if (at >= 0 && closerOk(s, from, at, c, n)) return at
            j += r
        }
        return -1
    }

    private fun closerOk(s: String, from: Int, at: Int, c: Char, n: Int): Boolean {
        val content = s.substring(from, at)
        if (content.isEmpty()) return false
        if (n == 1 && (content.first().isWhitespace() || content.last().isWhitespace())) return false
        if (c == '_' && at + n < s.length && isWord(s[at + n])) return false
        return true
    }

    private val italic = SpanStyle(fontStyle = FontStyle.Italic)
    private val bold = SpanStyle(fontWeight = FontWeight.Bold)
    private val underline = SpanStyle(textDecoration = TextDecoration.Underline)
    private val strike = SpanStyle(textDecoration = TextDecoration.LineThrough)

    private fun kinds(c: Char, n: Int): List<SpanStyle>? = when ("$c$n") {
        "*1", "_1" -> listOf(italic)
        "*2" -> listOf(bold)
        "*3" -> listOf(bold, italic)
        "_2" -> listOf(underline)
        "_3" -> listOf(underline, italic)
        "~2" -> listOf(strike)
        else -> null
    }

    /** Writes an emphasis that opens at [i] and gives its end, or -1 if it does not close. */
    private fun emphasis(s: String, i: Int, b: AnnotatedString.Builder, inLink: Boolean): Int {
        val c = s[i]
        val run = runLength(s, i, c)
        if (c == '~' && run < 2) return -1
        if (c == '_' && i > 0 && isWord(s[i - 1])) return -1
        val top = if (c == '~') 2 else minOf(run, 3)
        for (n in top downTo 1) {
            if (c == '~' && n != 2) continue
            val styles = kinds(c, n) ?: continue
            val at = findCloser(s, i + n, c, n)
            if (at < 0) continue
            for (st in styles) b.pushStyle(st)
            inline(s.substring(i + n, at), b, inLink)
            repeat(styles.size) { b.pop() }
            return at + n
        }
        return -1
    }

    private fun link(b: AnnotatedString.Builder, href: String, label: (AnnotatedString.Builder) -> Unit) {
        b.pushLink(LinkAnnotation.Url(href, linkStyles))
        label(b)
        b.pop()
    }

    /** The end of the own mention "@nick" at [i], or null if it names no own name. */
    private fun ownMention(s: String, i: Int): Int? {
        for (name in names) {
            val end = i + 1 + name.length
            if (end > s.length || !s.regionMatches(i + 1, name, 0, name.length, ignoreCase = true)) continue
            if (end < s.length && isWordish(s[end])) continue
            return end
        }
        return null
    }

    private fun inline(s: String, b: AnnotatedString.Builder, inLink: Boolean) {
        var i = 0
        while (i < s.length) {
            val c = s[i]
            if (c == '\\' && i + 1 < s.length && s[i + 1] in ESCAPABLE) {
                b.append(s[i + 1])
                i += 2
            } else if (c == '`') {
                val span = codeSpan(s, i)
                if (span != null) {
                    b.withStyle(SpanStyle(fontFamily = palette.mono, background = palette.codeBackground, fontSize = 0.9.em)) { append(span.first) }
                    i = span.second
                } else {
                    val n = runLength(s, i, '`')
                    b.append(s.substring(i, i + n))
                    i += n
                }
            } else if (c == '<') {
                val m = if (inLink) null else ANGLE_LINK.find(s.substring(i, minOf(s.length, i + 2100)))
                if (m != null && isLinkable(m.groupValues[1])) {
                    link(b, m.groupValues[1]) { it.append(m.groupValues[1]) }
                    i += m.value.length
                } else {
                    b.append(c)
                    i++
                }
            } else if (c == '[' && !inLink) {
                val m = maskedLink(s, i)
                if (m != null) {
                    link(b, m.href) { inline(m.text, it, true) }
                    if (maskedMismatch(m.text, m.href)) {
                        b.withStyle(SpanStyle(color = palette.muted)) { append(" (${hostOf(m.href) ?: m.href})") }
                    }
                    i = m.end
                } else {
                    b.append(c)
                    i++
                }
            } else if ((c == 'h' || c == 'H' || c == 'x' || c == 'X') && !inLink && (i == 0 || !isWord(s[i - 1]))) {
                val url = bareLinkAt(s, i)
                if (url != null) {
                    link(b, url) { it.append(url) }
                    i += url.length
                } else {
                    b.append(c)
                    i++
                }
            } else if (c == '@' && (i == 0 || !(isWordish(s[i - 1]) || s[i - 1] == '.'))) {
                val end = ownMention(s, i)
                if (end != null) {
                    b.withStyle(mentionStyle) { append(s.substring(i, end)) }
                    i = end
                } else {
                    b.append(c)
                    i++
                }
            } else if (c == '*' || c == '_' || c == '~') {
                val end = emphasis(s, i, b, inLink)
                if (end >= 0) {
                    i = end
                } else {
                    // An unclosed marker stays as text. The whole run stays, so it does not open later.
                    val n = runLength(s, i, c)
                    b.append(s.substring(i, i + n))
                    i += n
                }
            } else {
                b.append(c)
                i++
            }
        }
    }
}

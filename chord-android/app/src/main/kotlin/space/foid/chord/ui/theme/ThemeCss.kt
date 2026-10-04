package space.foid.chord.ui.theme

import androidx.compose.ui.graphics.Color
import kotlin.math.roundToInt

/*
 * Reads a Chord theme: CSS with a comment header, the same format as the desktop
 * (chord-desktop/src/lib/theme/themecss.ts). The four bundled themes are the desktop CSS files,
 * read by this code. A theme made for the desktop works here.
 *
 * Only custom properties are read: --surface-100 and the other colour tokens, per theme and per
 * [data-accent] rule. Nothing else in the CSS is run, so the desktop sanitiser (themesafe.ts,
 * which removes url() and @import) has nothing to guard here.
 */

/** The most characters of an imported theme: 200 000, as on the desktop. */
const val MAX_THEME_CHARS = 200_000

class ThemeAccent(val id: String, val name: String, val color: Color?)

class ThemeInfo(
    val name: String,
    val author: String?,
    val description: String?,
    val dark: Boolean,
    val accents: List<ThemeAccent>,
    /** The accent to use before the user picks one. */
    val defaultAccent: String?,
    /** The colour variables of `:root`. */
    internal val base: Map<String, String>,
    /** The colour variables of each `[data-accent]` rule. */
    internal val accentVars: Map<String, Map<String, String>>,
    /** Variables set only for one mode, such as `:root[data-mode='light']`. */
    internal val modeVars: Map<String, String>,
) {
    /** The accent that shows: [picked] if the theme has it, else the default. */
    fun accentOr(picked: String?): String? =
        if (picked != null && accents.any { it.id == picked }) picked else defaultAccent

    /** The full colour set for an accent. */
    fun colors(accent: String? = defaultAccent): ChordColors = buildColors(this, accent)

    /** The colours for a preview card. */
    fun swatch(accent: String? = defaultAccent): ThemeSwatch {
        val c = colors(accent)
        return ThemeSwatch(c.surface100, c.surface200, c.ink, c.brand)
    }
}

class ThemeSwatch(val surface: Color, val panel: Color, val ink: Color, val brand: Color)

// ---------------------------------------------------------------- parsing

private val COMMENT = Regex("/\\*([\\s\\S]*?)\\*/")
private val HEADER_LINE = Regex("^@([\\w-]+)\\s+(.+?)\\s*$")
private val ACCENT_SELECTOR = Regex("\\[data-accent\\s*=\\s*[\"']?([\\w-]+)[\"']?\\]")
private val MODE_SELECTOR = Regex("\\[data-mode\\s*=\\s*[\"']?(dark|light)[\"']?\\]")
private val RULE = Regex("([^{}]+)\\{([^{}]*)\\}")

private fun header(css: String): Map<String, List<String>> {
    val out = LinkedHashMap<String, MutableList<String>>()
    val block = COMMENT.find(css) ?: return out
    for (raw in block.groupValues[1].lines()) {
        val line = raw.replace(Regex("^\\s*\\*?\\s?"), "")
        val m = HEADER_LINE.find(line) ?: continue
        out.getOrPut(m.groupValues[1].lowercase()) { mutableListOf() }.add(m.groupValues[2])
    }
    return out
}

private fun title(id: String): String =
    id.replace(Regex("[-_]+"), " ").replace(Regex("\\b\\w")) { it.value.uppercase() }

/** `--name: value;` pairs of a declaration block. A value may hold `;` only inside parentheses. */
internal fun declarations(block: String): Map<String, String> {
    val out = LinkedHashMap<String, String>()
    var depth = 0
    var start = 0
    fun take(end: Int) {
        val decl = block.substring(start, end)
        val colon = decl.indexOf(':')
        if (colon > 0) {
            val name = decl.substring(0, colon).trim()
            if (name.startsWith("--")) out[name] = decl.substring(colon + 1).trim()
        }
    }
    for (i in block.indices) {
        when (block[i]) {
            '(' -> depth++
            ')' -> if (depth > 0) depth--
            ';' -> if (depth == 0) { take(i); start = i + 1 }
        }
    }
    if (start < block.length) take(block.length)
    return out
}

/** 0 (black) to 1 (white) for a colour, as the desktop does for a theme without @mode. */
fun luminance(color: Color): Float = 0.2126f * color.red + 0.7152f * color.green + 0.0722f * color.blue

/** Parses a theme. Never throws: a bad value falls back to the Chord token of the same mode. */
fun parseTheme(css: String): ThemeInfo {
    val meta = header(css)
    fun one(k: String) = meta[k]?.firstOrNull()

    val names = LinkedHashMap<String, String>()
    for (line in meta["accent"].orEmpty()) {
        val parts = line.split(Regex("\\s+"))
        val id = parts.firstOrNull().orEmpty()
        if (id.isNotEmpty()) names[id] = parts.drop(1).joinToString(" ").ifEmpty { title(id) }
    }

    // Rules, with comments taken out.
    val stripped = css.replace(COMMENT, "")
    val base = LinkedHashMap<String, String>()
    val modeRules = ArrayList<Pair<String, Map<String, String>>>()
    val accentVars = LinkedHashMap<String, MutableMap<String, String>>()
    val accentOrder = ArrayList<String>()
    for (m in RULE.findAll(stripped)) {
        val selector = m.groupValues[1].trim()
        val vars = declarations(m.groupValues[2])
        if (vars.isEmpty()) continue
        val accent = ACCENT_SELECTOR.find(selector)
        val mode = MODE_SELECTOR.find(selector)?.groupValues?.get(1)
        val isRoot = selector.startsWith(":root") || selector.startsWith("html")
        when {
            accent != null && isRoot -> {
                val id = accent.groupValues[1]
                if (id !in accentVars) accentOrder += id
                accentVars.getOrPut(id) { LinkedHashMap() }.putAll(vars)
            }
            mode != null && isRoot -> modeRules += mode to vars
            selector == ":root" || selector == "html" -> base.putAll(vars)
        }
    }

    val surface = base["--surface-100"]?.let { evalColor(it, base, 0) }
    val declared = one("mode")?.lowercase()
    val dark = when (declared) {
        "light" -> false
        "dark" -> true
        else -> (surface?.let(::luminance) ?: 0f) <= 0.5f
    }
    val modeVars = LinkedHashMap<String, String>()
    for ((m, vars) in modeRules) if ((m == "dark") == dark) modeVars.putAll(vars)

    // Accents: header order first, then the rest in rule order.
    val ids = LinkedHashSet<String>().apply { addAll(names.keys); addAll(accentOrder) }
    val accents = ids.map { id ->
        val vars = accentVars[id]
        val brand = (vars?.get("--brand") ?: base["--brand"])?.let { evalColor(it, base + (vars ?: emptyMap()), 0) }
        ThemeAccent(id, names[id] ?: title(id), brand)
    }
    val wanted = one("default")
    return ThemeInfo(
        name = one("name") ?: "Imported theme",
        author = one("author"),
        description = one("description"),
        dark = dark,
        accents = accents,
        defaultAccent = accents.firstOrNull { it.id == wanted }?.id ?: accents.firstOrNull()?.id,
        base = base,
        accentVars = accentVars,
        modeVars = modeVars,
    )
}

// ---------------------------------------------------------------- colours

private fun buildColors(info: ThemeInfo, accent: String?): ChordColors {
    val fallback = if (info.dark) ChordDark else ChordLight
    val vars = LinkedHashMap<String, String>().apply {
        putAll(info.base)
        putAll(info.modeVars)
        accent?.let { info.accentVars[it] }?.let(::putAll)
    }
    fun get(name: String, default: Color): Color =
        vars[name]?.let { evalColor(it, vars, 0) } ?: default

    val surface100 = get("--surface-100", fallback.surface100)
    val surface200 = get("--surface-200", fallback.surface200)
    val surface300 = get("--surface-300", fallback.surface300)
    val rail = get("--surface-rail", surface300)
    val ink = get("--ink", fallback.ink)
    val lineStrongDefault = mix(ink, 22f, surface100, 78f)
    val raisedDefault = if (info.dark) mix(ink, 14f, rail, 86f) else mix(Color.White, 80f, rail, 20f)
    return ChordColors(
        surface100 = surface100,
        surface200 = surface200,
        surface300 = surface300,
        surfaceRail = rail,
        surfaceSide = get("--surface-side", surface200),
        surfaceRaised = get("--surface-raised", raisedDefault),
        line = get("--line", fallback.line),
        lineStrong = get("--line-strong", lineStrongDefault),
        ink = ink,
        inkMuted = get("--ink-muted", fallback.inkMuted),
        brand = get("--brand", fallback.brand),
        brandSoft = get("--brand-soft", fallback.brandSoft),
        onBrand = get("--on-brand", fallback.onBrand),
        brandInk = get("--brand-ink", get("--brand", fallback.brandInk)),
        accent = get("--accent", fallback.accent),
        online = get("--online", fallback.online),
        away = get("--away", fallback.away),
        danger = get("--danger", fallback.danger),
        onDanger = get("--on-danger", fallback.onDanger),
        scrim = get("--scrim", fallback.scrim),
        isDark = info.dark,
    )
}

/** color-mix(in srgb): the mix of [a] at [pa] percent and [b] at [pb] percent, premultiplied. */
internal fun mix(a: Color, pa: Float, b: Color, pb: Float): Color {
    val total = pa + pb
    if (total <= 0f) return Color.Transparent
    val wa = pa / total
    val wb = pb / total
    val alpha = a.alpha * wa + b.alpha * wb
    if (alpha <= 0f) return Color.Transparent
    fun ch(x: Float, y: Float) = (x * a.alpha * wa + y * b.alpha * wb) / alpha
    return Color(ch(a.red, b.red), ch(a.green, b.green), ch(a.blue, b.blue), alpha)
}

private const val MAX_DEPTH = 12

/** The colour of a CSS value, or null if this reader does not know it. */
internal fun evalColor(value: String, vars: Map<String, String>, depth: Int): Color? {
    if (depth > MAX_DEPTH) return null
    val v = value.trim().removeSuffix("!important").trim()
    if (v.isEmpty()) return null
    if (v.startsWith("#")) return hexColor(v)
    if (v.equals("transparent", ignoreCase = true)) return Color.Transparent
    val open = v.indexOf('(')
    if (open < 0 || !v.endsWith(")")) return null
    val fn = v.substring(0, open).trim().lowercase()
    val inner = v.substring(open + 1, v.length - 1)
    return when (fn) {
        "var" -> {
            val args = splitTop(inner, ',')
            val name = args.firstOrNull()?.trim() ?: return null
            val target = vars[name]
            val own = target?.let { evalColor(it, vars, depth + 1) }
            own ?: if (args.size > 1) evalColor(args.drop(1).joinToString(","), vars, depth + 1) else null
        }
        "rgb", "rgba" -> rgbColor(inner)
        "color-mix" -> {
            val parts = splitTop(inner, ',').map { it.trim() }
            if (parts.size != 3 || !parts[0].lowercase().replace(Regex("\\s+"), " ").startsWith("in srgb")) return null
            val (ca, pa) = mixStop(parts[1], vars, depth) ?: return null
            val (cb, pb) = mixStop(parts[2], vars, depth) ?: return null
            val a = pa ?: pb?.let { 100f - it } ?: 50f
            val b = pb ?: (100f - a)
            mix(ca, a, cb, b)
        }
        else -> null
    }
}

private fun mixStop(text: String, vars: Map<String, String>, depth: Int): Pair<Color, Float?>? {
    val m = Regex("^(.*?)\\s+(\\d+(?:\\.\\d+)?)%$").find(text)
    val colorText = m?.groupValues?.get(1) ?: text
    val pct = m?.groupValues?.get(2)?.toFloat()
    val c = evalColor(colorText, vars, depth + 1) ?: return null
    return c to pct
}

/** Splits at [sep] outside parentheses. */
private fun splitTop(text: String, sep: Char): List<String> {
    val out = ArrayList<String>()
    var depth = 0
    var start = 0
    for (i in text.indices) {
        when (text[i]) {
            '(' -> depth++
            ')' -> depth--
            sep -> if (depth == 0) { out += text.substring(start, i); start = i + 1 }
        }
    }
    out += text.substring(start)
    return out
}

private fun hexColor(v: String): Color? {
    val h = v.removePrefix("#")
    if (!h.all { it in '0'..'9' || it in 'a'..'f' || it in 'A'..'F' }) return null
    val full = when (h.length) {
        3, 4 -> h.map { "$it$it" }.joinToString("")
        6, 8 -> h
        else -> return null
    }
    val r = full.substring(0, 2).toInt(16)
    val g = full.substring(2, 4).toInt(16)
    val b = full.substring(4, 6).toInt(16)
    val a = if (full.length == 8) full.substring(6, 8).toInt(16) else 255
    return Color(r, g, b, a)
}

private fun rgbColor(inner: String): Color? {
    val slash = inner.split('/')
    val chan = slash[0].trim().split(Regex("[\\s,]+")).filter { it.isNotEmpty() }
    var alphaText = slash.getOrNull(1)?.trim()
    var channels = chan
    if (alphaText == null && chan.size == 4) {
        alphaText = chan[3]
        channels = chan.take(3)
    }
    if (channels.size != 3) return null
    fun num(t: String): Float? =
        if (t.endsWith("%")) t.dropLast(1).toFloatOrNull()?.let { it * 2.55f } else t.toFloatOrNull()
    val rgb = channels.map { num(it) ?: return null }
    val a = alphaText?.let { t ->
        if (t.endsWith("%")) t.dropLast(1).toFloatOrNull()?.div(100f) else t.toFloatOrNull()
    } ?: 1f
    fun b(x: Float) = x.roundToInt().coerceIn(0, 255)
    return Color(b(rgb[0]), b(rgb[1]), b(rgb[2]), (a.coerceIn(0f, 1f) * 255f).roundToInt())
}

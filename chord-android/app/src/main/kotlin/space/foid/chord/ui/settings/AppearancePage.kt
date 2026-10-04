package space.foid.chord.ui.settings

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material3.Slider
import androidx.compose.material3.SliderDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.Stable
import kotlinx.coroutines.launch
import space.foid.chord.R
import space.foid.chord.ui.emoji.EmojiImages
import space.foid.chord.ui.emoji.EmojiPack
import space.foid.chord.ui.emoji.EmojiPackStatus
import space.foid.chord.ui.emoji.EmojiPacks
import space.foid.chord.ui.emoji.EmojiText
import space.foid.chord.ui.emoji.LocalEmojiImages
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordRadius
import space.foid.chord.ui.theme.ChordSpace
import space.foid.chord.ui.theme.ChordType
import space.foid.chord.ui.theme.ImportError
import space.foid.chord.ui.theme.ImportResult
import space.foid.chord.ui.theme.MotionMode
import space.foid.chord.ui.theme.ThemeEntry
import space.foid.chord.ui.theme.ThemeLibrary
import space.foid.chord.ui.theme.TimeFormat
import space.foid.chord.ui.theme.fetchThemeCss
import space.foid.chord.ui.theme.hostOf
import space.foid.chord.ui.theme.normalizeThemeUrl
import space.foid.chord.viewmodel.SettingsState

/** What the Appearance page can change. The defaults do nothing, for previews and screenshots. */
@Stable
class AppearanceActions(
    val onMode: (ThemeMode) -> Unit = {},
    val onPickTheme: (ThemeEntry) -> Unit = {},
    val onAccent: (themeId: String, accent: String) -> Unit = { _, _ -> },
    val onRemoveTheme: (String) -> Unit = {},
    /** Adds pasted CSS. Returns the problem, or null when it worked. */
    val onImportPaste: (String) -> ImportError? = { null },
    /** Fetches and adds a link. */
    val onImportLink: suspend (String) -> ImportError? = { null },
    val onFontSize: (Int) -> Unit = {},
    val onTimeFormat: (TimeFormat) -> Unit = {},
    val onJumboEmoji: (Boolean) -> Unit = {},
    val onUnderlineLinks: (Boolean) -> Unit = {},
    val onMotion: (MotionMode) -> Unit = {},
    val onShowPresence: (Boolean) -> Unit = {},
    val onPickPack: (EmojiPack) -> Unit = {},
)

/** What the Appearance page shows. */
@Immutable
class AppearanceModel(
    val prefs: AppPrefs,
    val library: List<ThemeEntry>,
    val packs: EmojiPackStatus = EmojiPackStatus(),
    /** The images of each installed pack, for the sample row of its card. */
    val samples: Map<EmojiPack, EmojiImages> = emptyMap(),
)

/** The page in the settings. It reads the stores and writes to them. */
@Composable
internal fun AppearancePage(state: SettingsState, a: SettingsActions) {
    val context = LocalContext.current
    val store = remember { PrefsSettingsStore.get(context) }
    val library = remember { ThemeLibrary.get(context) }
    val packs = remember { EmojiPacks.get(context) }
    val prefs = state.prefs
    val entries by library.entries.collectAsState()
    val status by packs.status.collectAsState()
    val samples by produceState(emptyMap<EmojiPack, EmojiImages>(), status.installed) {
        value = status.installed.mapNotNull { p -> packs.imagesFor(p)?.let { p to it } }.toMap()
    }
    val actions = remember(store, library, packs, a) {
        AppearanceActions(
            onMode = a.onTheme,
            onPickTheme = { e ->
                store.update { if (e.info.dark) it.copy(darkTheme = e.id) else it.copy(lightTheme = e.id) }
            },
            onAccent = { id, accent -> store.update { it.copy(accents = it.accents + (id to accent)) } },
            onRemoveTheme = { id ->
                library.remove(id)
                store.update {
                    it.copy(
                        darkTheme = if (it.darkTheme == id) space.foid.chord.ui.theme.DEFAULT_DARK_THEME else it.darkTheme,
                        lightTheme = if (it.lightTheme == id) space.foid.chord.ui.theme.DEFAULT_LIGHT_THEME else it.lightTheme,
                        accents = it.accents - id,
                    )
                }
            },
            onImportPaste = { css -> useImport(library.import(css), store) },
            onImportLink = { input ->
                val (url, bad) = normalizeThemeUrl(input)
                if (url == null) bad else {
                    val css = fetchThemeCss(url)
                    if (css == null) ImportError.Fetch else useImport(library.import(css, url), store)
                }
            },
            onFontSize = { v -> store.update { it.copy(fontSize = v) } },
            onTimeFormat = { v -> store.update { it.copy(timeFormat = v) } },
            onJumboEmoji = { v -> store.update { it.copy(jumboEmoji = v) } },
            onUnderlineLinks = { v -> store.update { it.copy(underlineLinks = v) } },
            onMotion = { v -> store.update { it.copy(motion = v) } },
            onShowPresence = a.onShowPresence,
            // A pack that is not on the phone downloads first. The choice sticks when it is ready.
            onPickPack = { p -> packs.install(p) { store.update { it.copy(emojiPack = p) } } },
        )
    }
    AppearanceContent(AppearanceModel(prefs, entries, status, samples), actions)
}

/** Uses an imported theme for its mode, and returns the error if there was one. */
private fun useImport(result: ImportResult, store: SettingsStore): ImportError? = when (result) {
    is ImportResult.Failed -> result.error
    is ImportResult.Added -> {
        val e = result.entry
        store.update { if (e.info.dark) it.copy(darkTheme = e.id) else it.copy(lightTheme = e.id) }
        null
    }
}

/** The stateless page, for the screenshot tests. */
@Composable
internal fun AppearanceContent(model: AppearanceModel, a: AppearanceActions) {
    val p = model.prefs
    var importing by rememberSaveable { mutableStateOf(false) }

    Group(stringResource(R.string.settings_mode)) {
        Box(Modifier.padding(ChordSpace.s2)) {
            Segmented(
                options = listOf(
                    ThemeMode.Dark to stringResource(R.string.settings_theme_dark),
                    ThemeMode.Light to stringResource(R.string.settings_theme_light),
                    ThemeMode.System to stringResource(R.string.settings_theme_system),
                ),
                selected = p.theme, onSelect = a.onMode, tag = "theme",
            )
        }
    }

    ThemesGroup(
        title = stringResource(R.string.appearance_dark_theme), dark = true, model = model, a = a,
        trailing = { SmallButton(stringResource(R.string.appearance_import_theme), { importing = true }, Modifier.testTag("import_theme")) },
    )
    ThemesGroup(title = stringResource(R.string.appearance_light_theme), dark = false, model = model, a = a)

    Group(stringResource(R.string.appearance_messages)) {
        MessagePreview()
        RowDivider()
        Block {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Label(stringResource(R.string.appearance_font_size))
                    Hint(stringResource(R.string.appearance_font_size_hint))
                }
                Text(stringResource(R.string.appearance_font_size_value, p.fontSize), style = ChordType.code, color = Chord.colors.inkMuted)
            }
            Slider(
                value = p.fontSize.toFloat(),
                onValueChange = { a.onFontSize(it.toInt()) },
                valueRange = FONT_SIZES.first.toFloat()..FONT_SIZES.last.toFloat(),
                steps = FONT_SIZES.last - FONT_SIZES.first - 1,
                colors = SliderDefaults.colors(
                    thumbColor = Chord.colors.brand, activeTrackColor = Chord.colors.brand,
                    inactiveTrackColor = Chord.colors.surface300, activeTickColor = Chord.colors.onBrand.copy(alpha = 0f),
                    inactiveTickColor = Chord.colors.inkMuted.copy(alpha = 0f),
                ),
                modifier = Modifier.testTag("settings_font_size"),
            )
        }
        RowDivider()
        Block {
            Label(stringResource(R.string.appearance_time_format))
            Segmented(
                options = listOf(
                    TimeFormat.System to stringResource(R.string.appearance_time_system),
                    TimeFormat.H12 to stringResource(R.string.appearance_time_12),
                    TimeFormat.H24 to stringResource(R.string.appearance_time_24),
                ),
                selected = p.timeFormat, onSelect = a.onTimeFormat, tag = "time_format",
            )
        }
        RowDivider()
        ToggleRow(
            stringResource(R.string.appearance_large_emoji), p.jumboEmoji, a.onJumboEmoji, "settings_jumbo_emoji",
            hint = stringResource(R.string.appearance_large_emoji_hint),
        )
        RowDivider()
        ToggleRow(
            stringResource(R.string.appearance_underline_links), p.underlineLinks, a.onUnderlineLinks, "settings_underline_links",
            hint = stringResource(R.string.appearance_underline_links_hint),
        )
    }

    Group(stringResource(R.string.settings_interface)) {
        Block {
            Label(stringResource(R.string.appearance_reduce_motion))
            Hint(stringResource(R.string.appearance_reduce_motion_hint))
            Segmented(
                options = listOf(
                    MotionMode.System to stringResource(R.string.appearance_motion_system),
                    MotionMode.Reduce to stringResource(R.string.appearance_motion_on),
                    MotionMode.Full to stringResource(R.string.appearance_motion_off),
                ),
                selected = p.motion, onSelect = a.onMotion, tag = "motion",
            )
        }
        RowDivider()
        ToggleRow(
            stringResource(R.string.settings_show_presence), p.showPresence, a.onShowPresence, "settings_show_presence",
            hint = stringResource(R.string.settings_show_presence_hint),
        )
    }

    Group(stringResource(R.string.appearance_emoji)) {
        Column(Modifier.padding(ChordSpace.s2), verticalArrangement = Arrangement.spacedBy(ChordSpace.s2)) {
            EmojiPack.entries.chunked(2).forEach { row ->
                Row(horizontalArrangement = Arrangement.spacedBy(ChordSpace.s2)) {
                    row.forEach { pack ->
                        PackCard(pack, model, a, Modifier.weight(1f))
                    }
                }
            }
        }
    }

    if (importing) {
        Dialog(onDismissRequest = { importing = false }) {
            Column(
                Modifier
                    .clip(RoundedCornerShape(ChordRadius.lg))
                    .background(Chord.colors.surface200)
                    .border(1.dp, Chord.colors.line, RoundedCornerShape(ChordRadius.lg))
                    .padding(ChordSpace.s4),
            ) {
                ImportThemeForm(
                    onPaste = a.onImportPaste,
                    onLink = a.onImportLink,
                    onDone = { importing = false },
                )
            }
        }
    }
}

// ---------------------------------------------------------------- themes

@Composable
private fun ThemesGroup(
    title: String,
    dark: Boolean,
    model: AppearanceModel,
    a: AppearanceActions,
    trailing: (@Composable () -> Unit)? = null,
) {
    val p = model.prefs
    val themes = model.library.filter { it.info.dark == dark }
    val pickedId = if (dark) p.darkTheme else p.lightTheme
    val picked = themes.firstOrNull { it.id == pickedId } ?: themes.firstOrNull()
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.Bottom) {
        Box(Modifier.weight(1f)) { GroupTitle(title) }
        if (trailing != null) Box(Modifier.padding(top = ChordSpace.s4, bottom = 4.dp)) { trailing() }
    }
    Group {
        themes.forEachIndexed { i, entry ->
            if (i > 0) RowDivider()
            ThemeCard(
                entry = entry,
                accent = entry.info.accentOr(p.accents[entry.id]),
                selected = entry.id == picked?.id,
                onClick = { a.onPickTheme(entry) },
                onRemove = if (entry.builtIn) null else ({ a.onRemoveTheme(entry.id) }),
            )
        }
        val accents = picked?.info?.accents.orEmpty()
        if (picked != null && accents.size > 1) {
            RowDivider()
            AccentPicker(picked, picked.info.accentOr(p.accents[picked.id]), a)
        }
    }
}

@Composable
private fun ThemeCard(
    entry: ThemeEntry,
    accent: String?,
    selected: Boolean,
    onClick: () -> Unit,
    onRemove: (() -> Unit)?,
) {
    val c = Chord.colors
    val sub = when {
        entry.builtIn -> stringResource(R.string.appearance_built_in)
        entry.url != null -> hostOf(entry.url)
        else -> entry.info.author ?: stringResource(R.string.appearance_imported)
    }
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 64.dp)
            .background(if (selected) c.selected else Color.Transparent)
            .clickable(role = Role.RadioButton, onClick = onClick)
            .semantics { this.selected = selected }
            .padding(horizontal = ChordSpace.s3, vertical = ChordSpace.s2)
            .testTag("theme_card_${entry.id}"),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(ChordSpace.s3),
    ) {
        ThemeSwatch(entry, accent)
        Column(Modifier.weight(1f)) {
            Text(entry.info.name, style = ChordType.name, color = c.ink, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(sub, style = ChordType.caption, color = c.inkMuted, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        if (onRemove != null) {
            Text(
                stringResource(R.string.appearance_remove), style = ChordType.label, color = c.danger,
                modifier = Modifier
                    .clip(RoundedCornerShape(ChordRadius.sm))
                    .clickable(role = Role.Button, onClick = onRemove)
                    .padding(horizontal = ChordSpace.s2, vertical = ChordSpace.s2)
                    .testTag("theme_remove_${entry.id}"),
            )
        }
        if (selected) CheckMark(c.brand)
    }
}

/** The small preview of a theme: the surface, a side panel, two lines of text and a dot in the accent. */
@Composable
private fun ThemeSwatch(entry: ThemeEntry, accent: String?) {
    val sw = entry.info.swatch(accent)
    val c = Chord.colors
    Box(
        Modifier
            .size(width = 52.dp, height = 40.dp)
            .clip(RoundedCornerShape(ChordRadius.sm))
            .background(sw.surface)
            .border(1.dp, c.line, RoundedCornerShape(ChordRadius.sm)),
    ) {
        Box(Modifier.width(12.dp).height(40.dp).background(sw.panel))
        Box(Modifier.padding(start = 18.dp, top = 9.dp).size(width = 26.dp, height = 3.dp).background(sw.ink, CircleShape))
        Box(Modifier.padding(start = 18.dp, top = 16.dp).size(width = 16.dp, height = 3.dp).background(sw.ink.copy(alpha = 0.55f), CircleShape))
        Box(Modifier.padding(start = 18.dp, top = 25.dp).size(8.dp).background(sw.brand, CircleShape))
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun AccentPicker(entry: ThemeEntry, accent: String?, a: AppearanceActions) {
    val c = Chord.colors
    Column(Modifier.fillMaxWidth().padding(ChordSpace.s3), verticalArrangement = Arrangement.spacedBy(ChordSpace.s2)) {
        Text(stringResource(R.string.appearance_accent), style = ChordType.label, color = c.ink)
        FlowRow(horizontalArrangement = Arrangement.spacedBy(ChordSpace.s2), verticalArrangement = Arrangement.spacedBy(ChordSpace.s2)) {
            entry.info.accents.forEach { acc ->
                val on = acc.id == accent
                Box(
                    Modifier
                        .size(36.dp)
                        .clip(CircleShape)
                        .border(2.dp, if (on) c.ink else Color.Transparent, CircleShape)
                        .padding(4.dp)
                        .clip(CircleShape)
                        .background(acc.color ?: c.brand)
                        .clickable(role = Role.RadioButton) { a.onAccent(entry.id, acc.id) }
                        .semantics { selected = on; contentDescription = acc.name }
                        .testTag("accent_${acc.id}"),
                )
            }
        }
    }
}

/** A check mark, like the desktop's. */
@Composable
internal fun CheckMark(color: Color, modifier: Modifier = Modifier) {
    Canvas(modifier.size(20.dp)) {
        val w = size.width
        val stroke = Stroke(width = 2.dp.toPx(), cap = StrokeCap.Round, join = StrokeJoin.Round)
        val path = androidx.compose.ui.graphics.Path().apply {
            moveTo(w * 0.2f, w * 0.52f)
            lineTo(w * 0.43f, w * 0.74f)
            lineTo(w * 0.8f, w * 0.28f)
        }
        drawPath(path, color, style = stroke)
    }
}

// ---------------------------------------------------------------- messages preview

@Composable
private fun MessagePreview() {
    val c = Chord.colors
    Column(Modifier.fillMaxWidth().padding(ChordSpace.s3), verticalArrangement = Arrangement.spacedBy(ChordSpace.s2)) {
        PreviewRow("R", c.accent, stringResource(R.string.appearance_preview_name_1)) {
            Text(stringResource(R.string.appearance_preview_text), style = ChordType.body, color = c.ink)
            val link = stringResource(R.string.appearance_preview_link)
            val underline = space.foid.chord.ui.theme.LocalAppearance.current.underlineLinks
            Text(
                buildLinkText(link, c.accent, underline), style = ChordType.body, color = c.ink,
            )
        }
        PreviewRow("M", c.online, stringResource(R.string.appearance_preview_name_2)) {
            val jumbo = space.foid.chord.ui.theme.LocalAppearance.current.jumboEmoji
            EmojiText(
                AnnotatedString("🚀🎉"),
                if (jumbo) ChordType.body.copy(fontSize = androidx.compose.ui.unit.TextUnit(40f, androidx.compose.ui.unit.TextUnitType.Sp), lineHeight = androidx.compose.ui.unit.TextUnit(48f, androidx.compose.ui.unit.TextUnitType.Sp)) else ChordType.body,
                c.ink,
            )
        }
    }
}

private fun buildLinkText(text: String, color: Color, underline: Boolean): AnnotatedString {
    val link = "chord.example/notes"
    val i = text.indexOf(link)
    return androidx.compose.ui.text.buildAnnotatedString {
        append(text.substring(0, i))
        pushStyle(
            androidx.compose.ui.text.SpanStyle(
                color = color,
                textDecoration = if (underline) TextDecoration.Underline else TextDecoration.None,
            ),
        )
        append(link)
        pop()
    }
}

@Composable
private fun PreviewRow(initial: String, color: Color, name: String, body: @Composable () -> Unit) {
    val c = Chord.colors
    Row(horizontalArrangement = Arrangement.spacedBy(ChordSpace.s3)) {
        Box(Modifier.size(40.dp).clip(CircleShape).background(color), contentAlignment = Alignment.Center) {
            Text(initial, style = ChordType.name, color = c.surface100)
        }
        Column(Modifier.weight(1f)) {
            Text(name, style = ChordType.name, color = c.ink)
            body()
        }
    }
}

// ---------------------------------------------------------------- emoji packs

@Composable
private fun PackCard(pack: EmojiPack, model: AppearanceModel, a: AppearanceActions, modifier: Modifier) {
    val c = Chord.colors
    val on = model.prefs.emojiPack == pack
    val ready = pack in model.packs.installed
    val busy = model.packs.installing != null
    val shape = RoundedCornerShape(ChordRadius.md)
    Column(
        modifier
            .clip(shape)
            .background(if (on) c.selected else c.surface200)
            .border(1.dp, if (on) c.inkMuted else c.line, shape)
            .clickable(enabled = !busy, role = Role.RadioButton) { a.onPickPack(pack) }
            .semantics { selected = on }
            .padding(ChordSpace.s3)
            .testTag("emoji_pack_${pack.name}"),
        verticalArrangement = Arrangement.spacedBy(ChordSpace.s2),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(pack.title, style = ChordType.name, color = c.ink, modifier = Modifier.weight(1f))
            if (on) CheckMark(c.brand, Modifier.size(16.dp))
        }
        Box(Modifier.heightIn(min = 32.dp), contentAlignment = Alignment.CenterStart) {
            val downloading = model.packs.installing == pack
            when {
                downloading -> {
                    val pct = model.packs.progress?.let { (it * 100).toInt() }
                    Text(
                        if (pct != null) stringResource(R.string.appearance_pack_downloading_percent, pct)
                        else stringResource(R.string.appearance_pack_downloading),
                        style = ChordType.caption, color = c.inkMuted,
                    )
                }
                model.packs.failed == pack -> Text(stringResource(R.string.appearance_pack_failed), style = ChordType.caption, color = c.danger)
                ready -> CompositionLocalProvider(LocalEmojiImages provides model.samples[pack]) {
                    EmojiText(AnnotatedString(SAMPLE), ChordType.body.copy(fontSize = androidx.compose.ui.unit.TextUnit(16f, androidx.compose.ui.unit.TextUnitType.Sp)), c.ink)
                }
                else -> Text(
                    stringResource(R.string.appearance_pack_download, pack.downloadMb ?: 0),
                    style = ChordType.caption, color = c.inkMuted,
                )
            }
        }
        Text(
            stringResource(
                when (pack) {
                    EmojiPack.Twemoji -> R.string.appearance_credit_twemoji
                    EmojiPack.Noto -> R.string.appearance_credit_noto
                    EmojiPack.Fluent -> R.string.appearance_credit_fluent
                    EmojiPack.System -> R.string.appearance_credit_system
                },
            ),
            style = ChordType.caption, color = c.inkMuted,
        )
    }
}

private const val SAMPLE = "😀👋🏽❤️🎉🚀"

// ---------------------------------------------------------------- import dialog

/** The form inside the import dialog. [onPaste] and [onLink] return the problem, or null. */
@Composable
internal fun ImportThemeForm(
    onPaste: (String) -> ImportError?,
    onLink: suspend (String) -> ImportError?,
    onDone: () -> Unit,
    initialTab: Int = 0,
    initialText: String = "",
    initialError: ImportError? = null,
) {
    val c = Chord.colors
    var tab by rememberSaveable { mutableStateOf(initialTab) }
    var url by rememberSaveable { mutableStateOf(if (initialTab == 0) initialText else "") }
    var css by rememberSaveable { mutableStateOf(if (initialTab == 1) initialText else "") }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf(initialError) }
    val scope = rememberCoroutineScope()
    Column(verticalArrangement = Arrangement.spacedBy(ChordSpace.s3)) {
        Text(stringResource(R.string.import_title), style = ChordType.title, color = c.ink)
        Segmented(
            options = listOf(0 to stringResource(R.string.import_tab_link), 1 to stringResource(R.string.import_tab_paste)),
            selected = tab, onSelect = { tab = it; error = null }, tag = "import_tab",
        )
        if (tab == 0) {
            Hint(stringResource(R.string.import_link_note))
            TextInput(
                value = url, onValueChange = { url = it; error = null },
                hint = stringResource(R.string.import_link_hint), onDone = {},
                modifier = Modifier.testTag("import_url"),
            )
        } else {
            Hint(stringResource(R.string.import_paste_note))
            BasicTextField(
                value = css, onValueChange = { css = it; error = null },
                textStyle = ChordType.code.copy(color = c.ink),
                cursorBrush = SolidColor(c.brand),
                modifier = Modifier.fillMaxWidth().testTag("import_css"),
                decorationBox = { inner ->
                    Box(
                        Modifier
                            .fillMaxWidth()
                            .heightIn(min = 140.dp, max = 220.dp)
                            .clip(RoundedCornerShape(ChordRadius.md))
                            .background(c.surface100)
                            .border(1.dp, c.lineStrong, RoundedCornerShape(ChordRadius.md))
                            .padding(ChordSpace.s3),
                    ) {
                        if (css.isEmpty()) Text(stringResource(R.string.import_paste_hint), style = ChordType.code, color = c.inkMuted)
                        inner()
                    }
                },
            )
        }
        error?.let { Text(importErrorText(it), style = ChordType.bodySmall, color = c.danger, modifier = Modifier.testTag("import_error")) }
        Hint(stringResource(R.string.import_trust_note))
        Row(horizontalArrangement = Arrangement.spacedBy(ChordSpace.s2, Alignment.End), modifier = Modifier.fillMaxWidth()) {
            SmallButton(stringResource(R.string.import_cancel), onDone)
            SmallButton(
                text = if (busy) stringResource(R.string.import_adding)
                else if (tab == 0) stringResource(R.string.import_add) else stringResource(R.string.import_add_theme),
                onClick = {
                    if (tab == 1) {
                        error = onPaste(css)
                        if (error == null) onDone()
                    } else {
                        busy = true
                        scope.launch {
                            error = onLink(url)
                            busy = false
                            if (error == null) onDone()
                        }
                    }
                },
                primary = true,
                enabled = !busy && (if (tab == 0) url.isNotBlank() else css.isNotBlank()),
                modifier = Modifier.testTag("import_add"),
            )
        }
    }
}

@Composable
private fun importErrorText(e: ImportError): String = stringResource(
    when (e) {
        ImportError.Empty -> R.string.import_error_empty
        ImportError.TooBig -> R.string.import_error_too_big
        ImportError.NoColours -> R.string.import_error_no_colours
        ImportError.BadLink -> R.string.import_error_bad_link
        ImportError.NotHttps -> R.string.import_error_not_https
        ImportError.Fetch -> R.string.import_error_fetch
    },
)

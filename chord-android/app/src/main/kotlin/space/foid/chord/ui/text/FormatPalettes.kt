package space.foid.chord.ui.text

import space.foid.chord.ui.theme.ChordColors
import space.foid.chord.ui.theme.ChordType

/** The format palette of a theme. Links use the accent colour, mentions brand-soft and brand-ink. */
fun ChordColors.formatPalette(): FormatPalette = FormatPalette(
    link = accent,
    muted = inkMuted,
    mentionBackground = brandSoft,
    mentionInk = brandInk,
    codeBackground = surface300,
    mono = ChordType.mono,
)

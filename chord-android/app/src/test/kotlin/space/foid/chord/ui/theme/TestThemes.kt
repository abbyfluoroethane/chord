package space.foid.chord.ui.theme

import java.io.File

/** The bundled themes, read from the CSS files in res/raw. JVM tests run in the module folder. */
object TestThemes {
    private val ids = listOf(
        "chord-dark" to "theme_chord_dark",
        "chord-light" to "theme_chord_light",
        "catppuccin-mocha" to "theme_catppuccin_mocha",
        "catppuccin-latte" to "theme_catppuccin_latte",
    )

    fun css(file: String): String = File("src/main/res/raw/$file.css").readText()

    fun bundled(): List<ThemeEntry> = ids.map { (id, file) -> ThemeEntry(id, css(file), builtIn = true) }

    fun entry(id: String): ThemeEntry = bundled().first { it.id == id }
}

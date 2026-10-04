package space.foid.chord.ui.settings

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onRoot
import com.github.takahirom.roborazzi.captureRoboImage
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordSpace
import space.foid.chord.ui.theme.ChordTheme
import space.foid.chord.update.ApkFile
import space.foid.chord.update.UpdateChannel
import space.foid.chord.update.UpdateFailure
import space.foid.chord.update.UpdateManifest
import space.foid.chord.update.UpdateStatus

@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [36], qualifiers = "w360dp-h800dp-xxhdpi")
class UpdatesScreenshotTest {
    @get:Rule val compose = createComposeRule()

    private val now = 1_790_000_000_000L
    private val base = UpdatesModel(
        enabled = true, buildChannel = "beta", channel = UpdateChannel.Beta,
        lastChecked = now - 3 * 60 * 60 * 1000L, now = now,
    )
    private val available = UpdateStatus.Available(
        UpdateManifest(
            version = "0.3.0-beta.3", build = 1_612_999, channel = "beta", commit = "1a2b3c4",
            pubDate = "2026-10-04T06:00:00Z", releaseUrl = "",
            notes = "- Replies show the quoted message.\n- Fixes a crash when a room has no name.",
            apks = emptyMap(),
        ),
        "x86_64",
        ApkFile("https://example.org/x.apk", "a".repeat(64), 19_919_549),
    )

    @Test fun idle_dark() = shot(true, "idle", base)
    @Test fun idle_light() = shot(false, "idle", base)

    @Test fun available_dark() = shot(true, "available", base.copy(status = available), notesOpen = true)
    @Test fun available_light() = shot(false, "available", base.copy(status = available))

    @Test fun downloading_dark() = shot(true, "downloading", base.copy(status = UpdateStatus.Downloading(available, 8_400_000, 19_919_549)))
    @Test fun downloading_light() = shot(false, "downloading", base.copy(status = UpdateStatus.Downloading(available, 8_400_000, 19_919_549)))

    @Test fun failed_dark() = shot(true, "failed", base.copy(status = UpdateStatus.Failed(UpdateFailure.Network)))
    @Test fun failed_light() = shot(false, "failed", base.copy(status = UpdateStatus.Failed(UpdateFailure.Checksum, available)))

    // A nightly build that follows Stable keeps itself until Stable has a newer build.
    @Test fun slower_dark() = shot(
        true, "slower",
        base.copy(buildChannel = "nightly", channel = UpdateChannel.Stable, status = UpdateStatus.UpToDate("0.2.0"), autoCheck = false),
    )
    @Test fun slower_light() = shot(
        false, "slower",
        base.copy(buildChannel = "nightly", channel = UpdateChannel.Stable, status = UpdateStatus.NoBuild),
    )

    @Test fun off_dark() = shot(true, "off", UpdatesModel(enabled = false, buildChannel = "dev"), version = "0.2.0+dev (61a3ec2)")
    @Test fun off_light() = shot(false, "off", UpdatesModel(enabled = false, buildChannel = "stable", storeBuild = true))

    private fun shot(
        dark: Boolean,
        name: String,
        model: UpdatesModel,
        notesOpen: Boolean = false,
        version: String = "0.3.0-beta.2 (09c83fb)",
    ) {
        compose.setContent {
            ChordTheme(dark = dark) {
                Column(
                    Modifier.fillMaxWidth().background(Chord.colors.surface100).padding(ChordSpace.s4),
                    verticalArrangement = Arrangement.spacedBy(ChordSpace.s2),
                ) {
                    AboutHeader(version, model.buildChannel, onOpenWebsite = {})
                    UpdatesContent(model, UpdatesActions(), notesOpen = notesOpen)
                }
            }
        }
        compose.onRoot().captureRoboImage("src/test/screenshots/updates_${name}_${if (dark) "dark" else "light"}.png")
    }
}

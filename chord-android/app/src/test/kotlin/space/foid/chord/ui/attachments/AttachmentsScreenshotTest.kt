package space.foid.chord.ui.attachments

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.painter.Painter
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.unit.dp
import com.github.takahirom.roborazzi.captureRoboImage
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import space.foid.chord.ui.components.MessageRow
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordSize
import space.foid.chord.ui.theme.ChordTheme
import space.foid.chord.ui.timeline.MessageUi
import space.foid.chord.viewmodel.UploadStage
import space.foid.chord.viewmodel.UploadUi

/** A picture with a known size, so that no decoder or network runs. */
private class FakePhoto(private val w: Float, private val h: Float) : Painter() {
    override val intrinsicSize = Size(w, h)
    override fun DrawScope.onDraw() {
        drawRect(Brush.linearGradient(listOf(Color(0xFF3B82F6), Color(0xFFF59E0B)), Offset.Zero, Offset(size.width, size.height)))
        drawCircle(Color.White.copy(alpha = 0.85f), radius = size.minDimension * 0.18f, center = Offset(size.width * 0.7f, size.height * 0.3f))
        drawRect(Color(0xFF14532D), topLeft = Offset(0f, size.height * 0.72f), size = Size(size.width, size.height * 0.28f))
    }
}

private fun fakeSource(status: ImageStatus = ImageStatus.LOADED, w: Float = 1600f, h: Float = 1000f) = ImageSource {
    ImageState(if (status == ImageStatus.LOADED) FakePhoto(w, h) else null, status)
}

@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [36], qualifiers = "w360dp-h900dp-xxhdpi")
class AttachmentsScreenshotTest {
    @get:Rule val compose = createComposeRule()

    private fun msg(name: String, attachment: String, outgoing: Boolean = false, body: String = "") = MessageUi(
        id = "m:$name", senderId = name, senderName = name, avatarUrl = null, body = body, timestamp = 0,
        timeLabel = "14:05", outgoing = outgoing, sameSenderAsPrevious = false, edited = false,
        retracted = false, attachment = attachment,
    )

    @Composable private fun Dot() {
        Box(Modifier.size(ChordSize.avatar).background(Chord.colors.accent, CircleShape))
    }

    @Composable private fun Rows(source: ImageSource) {
        Column(Modifier.background(Chord.colors.surface100).fillMaxWidth()) {
            CompositionLocalProvider(LocalImageSource provides source) {
                MessageRow(msg("Alice", "https://files.example/up/ab/photo.jpg", body = "The view from the top."), grouped = false, avatar = { Dot() })
                MessageRow(msg("Bob", "http://files.example/up/ab/plain.png"), grouped = false, avatar = { Dot() })
            }
            MessageRow(msg("Carol", "https://files.example/up/cd/Quarterly%20report.pdf"), grouped = false, avatar = { Dot() })
            MessageRow(msg("Dave", "https://files.example/up/cd/clip.mp4"), grouped = false, avatar = { Dot() })
        }
    }

    @Composable private fun Chips() {
        Column(Modifier.background(Chord.colors.surface100).fillMaxWidth().height(260.dp)) {
            FileChipContent(name = "Quarterly report.pdf", detail = "PDF file · 2.4 MB", action = "Open", onClick = {})
            FileChipContent(name = "a-very-long-file-name-that-does-not-fit-on-one-line-at-all.tar.gz", detail = "GZ file", action = "Open", onClick = {})
        }
    }

    @Composable private fun Pending() {
        Column(Modifier.background(Chord.colors.surface100).fillMaxWidth()) {
            PendingUploads(
                uploads = listOf(
                    UploadUi(1, "IMG_2041.jpg", UploadStage.PREPARING),
                    UploadUi(2, "holiday.jpg", UploadStage.UPLOADING),
                    UploadUi(3, "movie.mkv", UploadStage.FAILED, "This file is 140 MB. The limit is 100 MB."),
                ),
                onRetry = {}, onDismiss = {},
            )
        }
    }

    private fun shot(dark: Boolean, name: String, content: @Composable () -> Unit) {
        compose.setContent { ChordTheme(dark = dark) { content() } }
        compose.onRoot().captureRoboImage("src/test/screenshots/$name.png")
    }

    @Test fun attachment_rows_dark() = shot(true, "attachment_rows_dark") { Rows(fakeSource()) }
    @Test fun attachment_rows_light() = shot(false, "attachment_rows_light") { Rows(fakeSource()) }
    @Test fun attachment_image_loading_light() = shot(false, "attachment_image_loading_light") { Rows(fakeSource(ImageStatus.LOADING)) }
    @Test fun attachment_image_failed_dark() = shot(true, "attachment_image_failed_dark") { Rows(fakeSource(ImageStatus.FAILED)) }
    @Test fun attachment_tall_image_light() = shot(false, "attachment_tall_image_light") { Rows(fakeSource(w = 900f, h = 2400f)) }
    @Test fun file_chips_dark() = shot(true, "file_chips_dark") { Chips() }
    @Test fun file_chips_light() = shot(false, "file_chips_light") { Chips() }
    @Test fun pending_uploads_dark() = shot(true, "pending_uploads_dark") { Pending() }
    @Test fun pending_uploads_light() = shot(false, "pending_uploads_light") { Pending() }

    @Test fun image_viewer_dark() = shot(true, "image_viewer_dark") {
        Box(Modifier.height(640.dp)) {
            ImageViewerContent(ImageState(FakePhoto(1600f, 1000f), ImageStatus.LOADED), "photo.jpg", canSave = true, onClose = {}, onShare = {}, onSave = {})
        }
    }

    @Test fun image_viewer_no_save_light() = shot(false, "image_viewer_no_save_light") {
        Box(Modifier.height(640.dp)) {
            ImageViewerContent(ImageState(FakePhoto(1000f, 1600f), ImageStatus.LOADED), "portrait.jpg", canSave = false, onClose = {}, onShare = {}, onSave = {})
        }
    }
}

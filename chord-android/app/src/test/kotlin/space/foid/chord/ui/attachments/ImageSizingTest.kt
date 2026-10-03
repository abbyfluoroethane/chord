package space.foid.chord.ui.attachments

import org.junit.Assert.assertEquals
import org.junit.Test

class ImageSizingTest {
    @Test fun aSmallPhotoKeepsItsSize() {
        assertEquals(PixelSize(1920, 1080), downscaledSize(1920, 1080))
        assertEquals(PixelSize(2560, 1440), downscaledSize(2560, 1440))
    }

    @Test fun theLongestSideShrinksToTheLimit() {
        assertEquals(PixelSize(2560, 1440), downscaledSize(5120, 2880))
        assertEquals(PixelSize(1920, 2560), downscaledSize(3000, 4000))
        assertEquals(PixelSize(2560, 1920), downscaledSize(4000, 3000))
    }

    @Test fun aSideNeverDropsUnderOne() {
        assertEquals(PixelSize(2560, 1), downscaledSize(100_000, 10))
    }

    @Test fun theSampleSizeKeepsTheBitmapAtLeastAsBigAsTheTarget() {
        val target = downscaledSize(8000, 6000)
        assertEquals(PixelSize(2560, 1920), target)
        assertEquals(2, sampleSizeFor(8000, 6000, target))
        assertEquals(1, sampleSizeFor(3000, 2000, downscaledSize(3000, 2000)))
        assertEquals(1, sampleSizeFor(5000, 3000, downscaledSize(5000, 3000)))
    }

    @Test fun aspectIsLimitedAndHasADefault() {
        assertEquals(DEFAULT_ASPECT, thumbnailAspect(0f, 0f), 0f)
        assertEquals(1.5f, thumbnailAspect(300f, 200f), 0.001f)
        assertEquals(2.5f, thumbnailAspect(3000f, 100f), 0f)
        assertEquals(0.5f, thumbnailAspect(100f, 3000f), 0f)
    }
}

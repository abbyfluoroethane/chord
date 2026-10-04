package space.foid.chord.ui.attachments

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class AttachmentLogicTest {
    @Test fun kindByExtension() {
        assertEquals(AttachmentKind.IMAGE, kindOfName("https://x.example/a/photo.JPG"))
        assertEquals(AttachmentKind.IMAGE, kindOfName("https://x.example/a/photo.webp?token=1#frag"))
        assertEquals(AttachmentKind.VIDEO, kindOfName("clip.mp4"))
        assertEquals(AttachmentKind.AUDIO, kindOfName("voice.opus"))
        assertEquals(AttachmentKind.FILE, kindOfName("https://x.example/report.pdf"))
        assertEquals(AttachmentKind.FILE, kindOfName("https://x.example/noext"))
        assertEquals(AttachmentKind.FILE, kindOfName("https://x.example/dir.jpg/file"))
    }

    @Test fun kindByMime() {
        assertEquals(AttachmentKind.IMAGE, kindOfMime("image/png"))
        assertEquals(AttachmentKind.IMAGE, kindOfMime("IMAGE/JPEG; charset=x"))
        assertEquals(AttachmentKind.VIDEO, kindOfMime("video/webm"))
        assertEquals(AttachmentKind.AUDIO, kindOfMime("audio/mpeg"))
        assertEquals(AttachmentKind.FILE, kindOfMime("application/pdf"))
        assertNull(kindOfMime("application/octet-stream"))
        assertNull(kindOfMime(null))
        assertNull(kindOfMime(""))
    }

    @Test fun aKnownMimeBeatsTheExtension() {
        assertEquals(AttachmentKind.IMAGE, attachmentKind("https://x.example/download", "image/gif"))
        assertEquals(AttachmentKind.FILE, attachmentKind("https://x.example/a.jpg", "application/pdf"))
        assertEquals(AttachmentKind.IMAGE, attachmentKind("https://x.example/a.jpg", "application/octet-stream"))
    }

    @Test fun mimeFromName() {
        assertEquals("image/jpeg", mimeForName("IMG_1.JPG"))
        assertEquals("application/pdf", mimeForName("a.pdf"))
        assertEquals("application/octet-stream", mimeForName("thing.xyz"))
    }

    @Test fun fileNameFromUrl() {
        assertEquals("my photo.jpg", fileNameOfUrl("https://x.example/u/abc/my%20photo.jpg?x=1"))
        assertEquals("a+b.png", fileNameOfUrl("https://x.example/a+b.png"))
        assertEquals("", fileNameOfUrl("https://"))
    }

    @Test fun plainHttpImagesOfOthersDoNotLoad() {
        assertTrue(mayLoadImage("https://x.example/a.png", outgoing = false))
        assertFalse(mayLoadImage("http://x.example/a.png", outgoing = false))
        assertTrue(mayLoadImage("http://x.example/a.png", outgoing = true))
        assertFalse(mayLoadImage("file:///sdcard/a.png", outgoing = true))
        assertFalse(mayLoadImage("content://x/a.png", outgoing = false))
    }

    @Test fun byteSizes() {
        assertEquals("512 B", formatBytes(512))
        assertEquals("1 KB", formatBytes(1024))
        assertEquals("1.5 MB", formatBytes(1_572_864))
        assertEquals("12 MB", formatBytes(12L * 1024 * 1024))
    }

    @Test fun newExtension() {
        assertEquals("IMG_1.jpg", "IMG_1.HEIC".withExtension("jpg"))
        assertEquals("noext.jpg", "noext".withExtension("jpg"))
    }
}

package space.foid.chord.ui.timeline

import androidx.compose.ui.graphics.Color
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Test
import space.foid.chord.ui.text.FormatPalette
import space.foid.chord.ui.text.TextBlock
import uniffi.chord_ffi.DeliveryStatus
import uniffi.chord_ffi.ReplyPreview
import uniffi.chord_ffi.TimelineItem

class FormattedMappingTest {
    private val palette = FormatPalette(Color.Blue, Color.Gray, Color.Yellow, Color.Red, Color.LightGray)

    private fun item(
        body: String = "hello",
        outgoing: Boolean = false,
        name: String = "Alice",
        retracted: Boolean = false,
        replyTo: ReplyPreview? = null,
    ) = TimelineItem(
        id = "m:1", stanzaId = null, originId = null, sender = "alice@x.org", senderName = name, avatar = null,
        body = body, timestamp = 0, outgoing = outgoing, sameSenderAsPrevious = false, edited = false,
        retracted = retracted, reactions = emptyList(), replyTo = replyTo, attachment = null, status = DeliveryStatus.SENT,
    )

    @Test fun no_palette_means_no_formatted_text() = assertNull(item().toMessageUi().formatted)

    @Test fun palette_formats_the_body_in_the_mapping() {
        val m = item("see https://example.com").toMessageUi(palette = palette)
        assertNotNull(m.formatted)
        assertEquals(1, m.formatted!!.blocks.size)
    }

    @Test fun same_message_gives_the_same_formatted_instance() {
        val a = item("same **text** here").toMessageUi(palette = palette)
        val b = item("same **text** here").toMessageUi(palette = palette)
        assertSame(a.formatted, b.formatted)
        assertEquals(a, b)
    }

    @Test fun retracted_message_is_not_formatted() = assertNull(item(retracted = true).toMessageUi(palette = palette).formatted)

    @Test fun action_uses_the_sender_name() {
        val f = item("/me waves", name = "Alice").toMessageUi(palette = palette).formatted!!
        assertEquals("* Alice waves", (f.blocks.single() as TextBlock.Paragraph).text.text)
    }

    @Test fun own_names_come_from_the_account_and_the_own_nick() {
        val items = listOf(item(), item(outgoing = true, name = "Abby B"))
        assertEquals(listOf("Abby B", "abby", "abby@x.org"), ownMentionNames("abby@x.org", items))
        assertEquals(listOf("abby", "abby@x.org"), ownMentionNames("abby@x.org", listOf(item())))
        assertEquals(emptyList<String>(), ownMentionNames(null, emptyList()))
    }

    @Test fun empty_quoted_body_gives_an_empty_snippet() {
        val m = item(replyTo = ReplyPreview(null, "Bob", "")).toMessageUi()
        assertEquals(ReplyUi(null, "Bob", ""), m.reply)
    }
}

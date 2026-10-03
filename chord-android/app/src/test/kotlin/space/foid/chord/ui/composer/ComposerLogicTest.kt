package space.foid.chord.ui.composer

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import space.foid.chord.ui.sheets.EmojiEntry
import space.foid.chord.ui.sheets.EmojiGroup
import uniffi.chord_ffi.ChannelItem
import uniffi.chord_ffi.ChannelKind
import uniffi.chord_ffi.MemberItem

class ComposerLogicTest {
    @Test fun mention_found_at_start_and_after_space() {
        assertEquals(TokenMatch(0, "r"), findMention("@r", 2))
        assertEquals(TokenMatch(6, ""), findMention("hello @", 7))
        assertEquals(TokenMatch(6, "Ri"), findMention("hello @Ri", 9))
    }

    @Test fun mention_not_found_inside_a_word_or_after_a_space() {
        assertNull(findMention("mail@host", 9))
        assertNull(findMention("@rin hi", 7))
        assertNull(findMention("no at sign", 3))
    }

    @Test fun mention_uses_the_text_before_the_caret_only() {
        assertEquals(TokenMatch(0, "ri"), findMention("@rin hi", 3))
    }

    @Test fun suggest_nicks_starts_first_then_holds_sorted() {
        val nicks = listOf("Bart", "Rin", "Karin", "rick", "Rin", "Zed")
        assertEquals(listOf("rick", "Rin", "Karin"), suggestNicks(nicks, "ri"))
        assertEquals(listOf("Bart", "Karin", "rick", "Rin", "Zed"), suggestNicks(nicks, ""))
        assertEquals(2, suggestNicks(nicks, "", max = 2).size)
    }

    @Test fun insert_mention_replaces_the_query_and_keeps_the_rest() {
        val text = "hey @ri how are you"
        val m = findMention(text, 7)!!
        val edit = insertMention(text, 7, m, "Rin")
        assertEquals("hey @Rin  how are you", edit.text)
        assertEquals(9, edit.caret)
    }

    @Test fun shortcode_needs_two_letters_and_a_boundary() {
        assertEquals(TokenMatch(5, "fi"), findShortcode("nice :fi", 8))
        assertNull(findShortcode("nice :f", 7))
        assertNull(findShortcode("12:30", 5))
        assertNull(findShortcode("done :fire: ", 12))
    }

    private val groups = listOf(
        EmojiGroup(
            "smileys", "Smileys",
            listOf(
                EmojiEntry("🔥", "fire", "fire flame hot"),
                EmojiEntry("🐟", "fish", "fish fishy"),
                EmojiEntry("🇫🇯", "flag: Fiji", "flag: fiji fiji"),
                EmojiEntry("👍🏽", "thumbs up: medium skin tone", "thumbs up: medium skin tone fire_tone"),
                EmojiEntry("😀", "grinning face", "grinning face grin happy"),
            ),
        ),
    )

    @Test fun shortcode_names_come_from_labels_and_tags() {
        assertEquals("grinning_face", shortcodeOf("grinning face"))
        assertEquals("flag_fiji", shortcodeOf("flag: Fiji"))
        val index = ShortcodeIndex(groups)
        assertEquals(listOf("grin"), index.suggest("gri").map { it.name }.take(1))
        assertEquals("😀", index.suggest("happy").single().emoji)
    }

    @Test fun shortcode_suggestions_have_one_emoji_each_and_short_names_first() {
        val index = ShortcodeIndex(groups)
        val hits = index.suggest("fi")
        assertEquals(listOf("fiji", "fire", "fish"), hits.map { it.name })
        assertEquals(hits.map { it.emoji }.distinct(), hits.map { it.emoji })
    }

    @Test fun insert_shortcode_puts_the_emoji_and_a_space() {
        val m = findShortcode("nice :fi", 8)!!
        assertEquals(TextEdit("nice 🔥 ", 8), insertShortcode("nice :fi", 8, m, "🔥"))
    }

    @Test fun insert_at_selection_replaces_a_range() {
        assertEquals(TextEdit("a😀c", 3), insertAtSelection("abc", 1, 2, "😀"))
        assertEquals(TextEdit("abc😀", 5), insertAtSelection("abc", 3, 3, "😀"))
        assertEquals(TextEdit("a😀c", 3), insertAtSelection("abc", 2, 1, "😀"))
    }

    @Test fun quick_reactions_use_recent_then_defaults() {
        assertEquals(listOf("👍", "❤️", "😂", "👀"), quickReactions(emptyList()))
        assertEquals(listOf("🔥", "👍", "❤️", "😂"), quickReactions(listOf("🔥")))
        assertEquals(listOf("👀", "🔥", "👍", "❤️"), quickReactions(listOf("👀", "🔥")))
    }

    @Test fun links_are_found_and_cleaned() {
        assertEquals(listOf("https://a.org/x"), linksIn("see https://a.org/x. And https://a.org/x"))
        assertEquals(listOf("https://a.org/x?y=1"), linksIn("(https://a.org/x?y=1)"))
        assertTrue(linksIn("no link, xmpp:room@x.org?join").isEmpty())
        assertEquals(3, linksIn("https://a.org/1 https://a.org/2 https://a.org/3 https://a.org/4").size)
    }

    @Test fun channel_links() {
        assertEquals("xmpp:room@x.org?join", channelLink("room@x.org", direct = false))
        assertEquals("xmpp:bob@x.org?message", channelLink("bob@x.org", direct = true))
    }

    @Test fun forward_text_holds_body_and_file_once() {
        assertEquals("hello", forwardText("  hello ", null))
        assertEquals("look\nhttps://u.org/a.png", forwardText("look", "https://u.org/a.png"))
        assertEquals("https://u.org/a.png", forwardText("https://u.org/a.png", "https://u.org/a.png"))
        assertEquals("https://u.org/a.png", forwardText("", "https://u.org/a.png"))
        assertEquals("", forwardText("  ", null))
    }

    @Test fun forward_summary() {
        assertEquals("hi", forwardSummary(" hi ", null))
        assertEquals("File: a.png", forwardSummary("", "https://u.org/dir/a.png?x=1"))
        assertEquals("Empty message", forwardSummary("", null))
    }

    private fun chan(jid: String, name: String, kind: ChannelKind, joined: Boolean = true) = ChannelItem(
        jid = jid, name = name, kind = kind, category = null, joined = joined, lastActivity = null,
        unread = 0u, blocked = false, members = null,
    )

    @Test fun forward_targets_and_filter() {
        val list = listOf(
            chan("room@c.org", "general", ChannelKind.Room),
            chan("bob@x.org", "Bob", ChannelKind.Direct),
            chan("gone@c.org", "gone", ChannelKind.Room, joined = false),
            chan("ops@c.org", "ops-general", ChannelKind.Room),
        )
        val targets = forwardTargets(list)
        assertEquals(listOf("general", "Bob", "ops-general"), targets.map { it.label })
        assertTrue(targets[1].direct)
        assertFalse(targets[0].direct)
        assertEquals(listOf("general", "ops-general"), filterForwardTargets(targets, "gen").map { it.label })
        assertEquals(listOf("ops-general"), filterForwardTargets(targets, "ops").map { it.label })
        assertEquals(3, filterForwardTargets(targets, " ").size)
        assertTrue(filterForwardTargets(targets, "zzz").isEmpty())
    }

    private fun member(jid: String?, role: String, aff: String) = MemberItem(
        id = jid ?: "x", name = "n", jid = jid, role = role, affiliation = aff, show = null,
        status = null, online = true, avatar = null,
    )

    @Test fun moderator_check() {
        val list = listOf(
            member("me@x.org/phone", "participant", "none"),
            member("adm@x.org", "participant", "admin"),
            member("mod@x.org", "Moderator", "member"),
        )
        assertFalse(canModerate(list, "me@x.org"))
        assertTrue(canModerate(list, "ADM@x.org"))
        assertTrue(canModerate(list, "mod@x.org"))
        assertFalse(canModerate(list, "other@x.org"))
        assertFalse(canModerate(list, null))
    }

    private fun ui(outgoing: Boolean = false, retracted: Boolean = false, body: String = "hi", attachment: String? = null) =
        space.foid.chord.ui.timeline.MessageUi(
            id = "m:1", senderId = "a", senderName = "A", avatarUrl = null, body = body, timestamp = 0,
            timeLabel = "1", outgoing = outgoing, sameSenderAsPrevious = false, edited = false,
            retracted = retracted, attachment = attachment,
        )

    @Test fun actions_of_another_persons_message() {
        assertEquals(
            listOf(MessageAction.Reply, MessageAction.Forward, MessageAction.CopyText, MessageAction.CopyChannelLink, MessageAction.CopyId),
            messageActions(ui(), moderator = false, channelLink = true),
        )
    }

    @Test fun actions_of_an_own_message_have_edit_and_delete_in_desktop_order() {
        assertEquals(
            listOf(
                MessageAction.Edit, MessageAction.Reply, MessageAction.Forward, MessageAction.CopyText,
                MessageAction.CopyChannelLink, MessageAction.Delete, MessageAction.CopyId,
            ),
            messageActions(ui(outgoing = true), moderator = true, channelLink = true),
        )
    }

    @Test fun moderator_removes_the_message_of_someone_else() {
        assertTrue(MessageAction.Remove in messageActions(ui(), moderator = true, channelLink = false))
        assertFalse(MessageAction.CopyChannelLink in messageActions(ui(), moderator = true, channelLink = false))
    }

    @Test fun a_deleted_message_keeps_only_the_link_and_the_id() {
        assertEquals(
            listOf(MessageAction.CopyChannelLink, MessageAction.CopyId),
            messageActions(ui(outgoing = true, retracted = true), moderator = true, channelLink = true),
        )
    }

    @Test fun a_file_with_no_text_can_be_forwarded_but_not_copied() {
        val a = messageActions(ui(body = "", attachment = "https://u.org/a.png"), moderator = false, channelLink = false)
        assertTrue(MessageAction.Forward in a)
        assertFalse(MessageAction.CopyText in a)
    }
}

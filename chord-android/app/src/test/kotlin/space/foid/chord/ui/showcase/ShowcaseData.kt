package space.foid.chord.ui.showcase

import android.graphics.BitmapFactory
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import space.foid.chord.ui.components.Presence
import space.foid.chord.ui.screens.AccountUi
import space.foid.chord.ui.screens.TimelineRowUi
import space.foid.chord.ui.screens.buildTimelineRows
import space.foid.chord.ui.text.FormatPalette
import space.foid.chord.ui.text.formatMessage
import space.foid.chord.ui.timeline.MessageUi
import space.foid.chord.ui.timeline.ReactionUi
import space.foid.chord.ui.timeline.ReplyUi
import space.foid.chord.ui.timeline.GROUP_GAP_MS
import space.foid.chord.ui.timeline.clockLabel
import space.foid.chord.ui.timeline.replySnippet
import space.foid.chord.viewmodel.ProfileState
import uniffi.chord_ffi.ChannelItem
import uniffi.chord_ffi.ChannelKind
import uniffi.chord_ffi.Contact
import uniffi.chord_ffi.MemberItem
import uniffi.chord_ffi.SpaceItem
import uniffi.chord_ffi.SubscriptionState
import java.time.LocalDateTime
import java.time.ZoneOffset

/**
 * The showcase story of docs/brand/showcase/README.md as UI models: the people, the spaces, the
 * channels and the conversations. The desktop preview tells the same story.
 */
internal object ShowcaseData {
    const val DOMAIN = "chat.foid.space"
    const val ROOMS = "rooms.chat.foid.space"
    const val SPACES = "spaces.chat.foid.space"

    /** One person of the story. [art] is the file name in test resources/showcase, or null for initials. */
    data class Person(
        val name: String,
        val jid: String,
        val art: String?,
        val online: Boolean,
        val show: String?,
        val status: String?,
    ) {
        val presence: Presence get() = space.foid.chord.ui.components.presenceOf(online, show)
    }

    val nina = Person("nina", "nina@$DOMAIN", "maya.png", true, null, null)
    val marco = Person("marco", "marco@$DOMAIN", "theo.png", true, null, null)
    val jess = Person("jess", "jess@$DOMAIN", "priya.png", true, null, null)
    val theo = Person("theo", "theo@$DOMAIN", "luis.png", true, "away", "at work")
    val sam = Person("sam", "sam@$DOMAIN", null, true, null, null)
    val kai = Person("kai", "kai@xmpp.example.net", "noor.png", true, null, null)
    val lena = Person("lena", "lena@$DOMAIN", "ada.png", true, "dnd", null)
    val rory = Person("rory", "rory@$DOMAIN", "kenji.png", false, null, null)
    val ben = Person("ben", "ben@$DOMAIN", null, false, null, null)

    val people = listOf(marco, jess, theo, sam, kai, lena, rory, ben)

    val account = AccountUi(nina.jid, nina.name)

    // ---------------------------------------------------------------- spaces

    data class Space(val item: SpaceItem, val art: String?) {
        val owner: String get() = "${item.service}/${item.node}"
    }

    val basement = Space(SpaceItem(SPACES, "basement", "basement", null), "basement.png")
    val nightOwls = Space(SpaceItem(SPACES, "night-owls", "night owls", null), "night-owls.png")
    val filmClub = Space(SpaceItem(SPACES, "film-club", "film club", null), "darkroom.png")
    val climbing = Space(SpaceItem(SPACES, "climbing", "climbing", null), "crag.png")
    val sandbox = Space(SpaceItem(SPACES, "sandbox", "Sandbox", null), null)

    val spaceList = listOf(basement, nightOwls, filmClub, climbing, sandbox)
    val spaces = spaceList.map { it.item }

    /** The rail badges, keyed by `service|node` (SpaceItem.stableKey). */
    val spaceUnread = mapOf("$SPACES|night-owls" to 2, "$SPACES|climbing" to 1)

    /** jess 1 and sunday dinner 2. */
    const val HOME_UNREAD = 3

    // ---------------------------------------------------------------- channels

    private fun room(space: String, name: String, unread: Int = 0) = ChannelItem(
        jid = "$space-$name@$ROOMS", name = name, kind = ChannelKind.Room, category = null,
        joined = true, lastActivity = null, unread = unread.toUInt(), blocked = false, members = null,
    )

    val basementChannels = listOf(
        room("basement", "general", unread = 5),
        room("basement", "music"),
        room("basement", "pics", unread = 4),
        room("basement", "games"),
        room("basement", "memes", unread = 9),
    )
    val music = basementChannels.first { it.name == "music" }
    val mutedBasement = setOf(basementChannels.first { it.name == "memes" }.jid)

    private fun dm(p: Person, unread: Int = 0) = ChannelItem(
        jid = p.jid, name = p.name, kind = ChannelKind.Direct, category = null,
        joined = true, lastActivity = null, unread = unread.toUInt(), blocked = false, members = null,
    )

    val sundayDinner = ChannelItem(
        jid = "sunday-dinner@$ROOMS", name = "sunday dinner", kind = ChannelKind.Room, category = null,
        joined = true, lastActivity = null, unread = 2u, blocked = false, members = 4u,
    )

    /** Home, newest first. */
    val homeChannels = listOf(dm(jess, unread = 1), sundayDinner, dm(marco), dm(kai), dm(lena))

    // ---------------------------------------------------------------- contacts

    private fun contact(
        jid: String,
        name: String?,
        online: Boolean = false,
        show: String? = null,
        status: String? = null,
        subscription: SubscriptionState = SubscriptionState.BOTH,
        ask: Boolean = false,
        blocked: Boolean = false,
    ) = Contact(
        jid = jid, name = name, subscription = subscription, ask = ask, groups = emptyList(), approved = false,
        blocked = blocked, online = online, show = show, status = status, idleSince = null, activity = null,
    )

    val contacts: List<Contact> = people.map { contact(it.jid, it.name, it.online, it.show, it.status) } + listOf(
        // The incoming request from mika.
        contact("mika@xmpp.example.net", "mika", subscription = SubscriptionState.NONE),
        // The outgoing request to ines.
        contact("ines@xmpp.example.net", "ines", subscription = SubscriptionState.NONE, ask = true),
        contact("promo@spam.example", null, subscription = SubscriptionState.NONE, blocked = true),
    )

    /** One incoming request: mika. */
    const val PENDING_CONTACTS = 1

    // ---------------------------------------------------------------- members

    private fun member(p: Person, affiliation: String, role: String = "participant") = MemberItem(
        id = p.name, name = p.name, jid = p.jid, role = role, affiliation = affiliation,
        show = p.show, status = p.status, online = p.online, avatar = null,
    )

    val basementMembers = listOf(
        member(theo, "owner", "moderator"),
        member(nina, "admin", "moderator"),
        member(marco, "member"),
        member(jess, "member"),
        member(sam, "member"),
        member(kai, "member"),
        member(lena, "member"),
        member(rory, "member"),
        member(ben, "member"),
    )

    // ---------------------------------------------------------------- profile

    val jessProfile = ProfileState(
        address = jess.jid, name = jess.name, presence = jess.presence, isContact = true,
        affiliation = "member", sharedSpaces = listOf(basement.item, climbing.item),
    )

    // ---------------------------------------------------------------- conversations

    /** Today in the story, in UTC (the tests run in UTC). */
    private fun at(hour: Int, minute: Int): Long =
        LocalDateTime.of(2026, 10, 4, hour, minute).toInstant(ZoneOffset.UTC).toEpochMilli()

    const val HARBOR_URL = "https://upload.chat.foid.space/7c1e/harbor-sunset.png"

    /** The URL of each attachment, to the file in test resources/showcase. */
    val attachmentArt = mapOf(HARBOR_URL to "harbor-sunset.png")

    private class Builder(private val palette: FormatPalette, private val own: List<String>) {
        val out = mutableListOf<MessageUi>()
        fun msg(
            p: Person,
            hour: Int,
            minute: Int,
            body: String,
            attachment: String? = null,
            reactions: List<ReactionUi> = emptyList(),
            replyTo: MessageUi? = null,
        ): MessageUi {
            val ts = at(hour, minute)
            // The core flag: the same sender as the row before, and close in time.
            val prev = out.lastOrNull()
            val same = prev != null && prev.senderId == p.jid && ts - prev.timestamp < GROUP_GAP_MS
            val m = MessageUi(
                id = "m:${out.size + 1}", senderId = p.jid, senderName = p.name, avatarUrl = null, body = body,
                timestamp = ts, timeLabel = clockLabel(ts), stamp = "today ${clockLabel(ts)}", outgoing = p == nina,
                sameSenderAsPrevious = same, edited = false, retracted = false, reactions = reactions,
                reply = replyTo?.let { ReplyUi(it.id, it.senderName, replySnippet(it.body)) },
                attachment = attachment,
                formatted = formatMessage(body, palette, own, p.name),
            )
            out += m
            return m
        }
    }

    private val ownNames = listOf(nina.name, nina.jid)

    /** basement / #music, newest first. The New line is above message 9. */
    fun musicRows(palette: FormatPalette): List<TimelineRowUi> {
        val b = Builder(palette, ownNames)
        b.msg(nina, 21, 4, "new ninajirachi is out")
        // Android shows no link preview: the link shows as text.
        b.msg(nina, 21, 4, "https://www.youtube.com/watch?v=Ob_EDY9Eiis", reactions = listOf(ReactionUi("🔥", 4, true)))
        b.msg(marco, 21, 5, "WITH PORTER??")
        b.msg(nina, 21, 5, "with porter")
        b.msg(jess, 21, 7, "ok this goes so hard")
        val drop = b.msg(marco, 21, 9, "the drop is insane")
        b.msg(jess, 21, 10, "is this the one from coachella", replyTo = drop)
        b.msg(nina, 21, 10, "yeah he came out for it in april")
        val car = b.msg(theo, 21, 14, "adding it to the car playlist")
        b.msg(kai, 21, 15, "is she touring this year")
        return buildTimelineRows(b.out, firstUnreadId = car.id)
    }

    /** basement / #pics, newest first. */
    fun picsRows(palette: FormatPalette): List<TimelineRowUi> {
        val b = Builder(palette, ownNames)
        b.msg(theo, 19, 2, "sunset from the ferry", attachment = HARBOR_URL)
        b.msg(nina, 19, 10, "wait where is this")
        b.msg(theo, 19, 12, "coming back from the island")
        b.msg(jess, 19, 20, "jealous")
        return buildTimelineRows(b.out)
    }

    /** Home / jess, newest first. jess's last message is unread. */
    fun jessRows(palette: FormatPalette): List<TimelineRowUi> {
        val b = Builder(palette, ownNames)
        b.msg(jess, 20, 41, "are you going to the show on the 18th")
        b.msg(nina, 20, 48, "if i can get off work")
        val last = b.msg(jess, 20, 50, "ill grab 2 just in case")
        return buildTimelineRows(b.out, firstUnreadId = last.id)
    }

    // ---------------------------------------------------------------- art

    /** The PNG [name] from test resources/showcase. */
    fun art(name: String): ByteArray =
        checkNotNull(ShowcaseData::class.java.classLoader!!.getResourceAsStream("showcase/$name")) { "no art $name" }
            .use { it.readBytes() }

    fun bitmap(name: String): ImageBitmap {
        val bytes = art(name)
        return BitmapFactory.decodeByteArray(bytes, 0, bytes.size).asImageBitmap()
    }

    /** The avatar art of each owner key: people by address, spaces by `service/node`. */
    val avatarArt: Map<String, String> =
        (people + nina).mapNotNull { p -> p.art?.let { p.jid to it } }.toMap() +
            spaceList.mapNotNull { s -> s.art?.let { s.owner to it } }.toMap()
}

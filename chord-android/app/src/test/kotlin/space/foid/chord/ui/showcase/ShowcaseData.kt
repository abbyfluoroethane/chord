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
        val key: String,
        val name: String,
        val jid: String,
        val art: String?,
        val online: Boolean,
        val show: String?,
        val status: String?,
    ) {
        val presence: Presence get() = space.foid.chord.ui.components.presenceOf(online, show)
    }

    val maya = Person("maya", "Maya Okafor", "maya@$DOMAIN", "maya.png", true, null, "Playtest week")
    val priya = Person("priya", "Priya Raman", "priya@$DOMAIN", "priya.png", true, null, null)
    val theo = Person("theo", "Theo Lindqvist", "theo@$DOMAIN", "theo.png", true, null, "Profiling saves")
    val jules = Person("jules", "Jules Moreau", "jules@$DOMAIN", "jules.png", true, "away", null)
    val kenji = Person("kenji", "Kenji Ito", "kenji@$DOMAIN", "kenji.png", true, null, null)
    val ada = Person("ada", "Ada Novak", "ada@$DOMAIN", "ada.png", true, "dnd", "Writing")
    val noor = Person("noor", "Noor Haddad", "noor@xmpp.example.net", "noor.png", true, null, null)
    val luis = Person("luis", "Luis Ortega", "luis@$DOMAIN", "luis.png", false, null, null)
    val sam = Person("sam", "Sam Delgado", "sam@$DOMAIN", null, true, "away", null)
    val wren = Person("wren", "Wren Callahan", "wren@$DOMAIN", null, false, null, null)

    val people = listOf(priya, theo, jules, kenji, ada, noor, luis, sam, wren)

    val account = AccountUi(maya.jid, maya.name)

    // ---------------------------------------------------------------- spaces

    data class Space(val item: SpaceItem, val art: String?) {
        val owner: String get() = "${item.service}/${item.node}"
    }

    val lanternWorks = Space(SpaceItem(SPACES, "lantern-works", "Lantern Works", null), "lantern-works.png")
    val darkroom = Space(SpaceItem(SPACES, "darkroom", "Darkroom", null), "darkroom.png")
    val cragClub = Space(SpaceItem(SPACES, "crag-club", "Crag Club", null), "crag.png")
    val slowReaders = Space(SpaceItem(SPACES, "slow-readers", "Slow Readers", null), "slow-readers.png")
    val sandbox = Space(SpaceItem(SPACES, "sandbox", "Sandbox", null), null)

    val spaceList = listOf(lanternWorks, darkroom, cragClub, slowReaders, sandbox)
    val spaces = spaceList.map { it.item }

    /** The rail badges, keyed by `service|node` (SpaceItem.stableKey). */
    val spaceUnread = mapOf("$SPACES|darkroom" to 2, "$SPACES|slow-readers" to 1)

    /** Theo 1 and Lantern core 2. */
    const val HOME_UNREAD = 3

    // ---------------------------------------------------------------- channels

    private fun room(space: String, name: String, category: String? = null, unread: Int = 0) = ChannelItem(
        jid = "$space-$name@$ROOMS", name = name, kind = ChannelKind.Room, category = category,
        joined = true, lastActivity = null, unread = unread.toUInt(), blocked = false, members = null,
    )

    val lanternChannels = listOf(
        room("lw", "announcements"),
        room("lw", "general", unread = 4),
        room("lw", "playtest", "Playtest"),
        room("lw", "bugs", "Playtest", unread = 3),
        room("lw", "ideas", "Playtest"),
        room("lw", "art", "Studio", unread = 2),
        room("lw", "audio", "Studio"),
        room("lw", "off-topic", "Studio", unread = 12),
    )
    val playtest = lanternChannels.first { it.name == "playtest" }
    val mutedLantern = setOf(lanternChannels.first { it.name == "off-topic" }.jid)

    val darkroomChannels = listOf(
        room("dr", "general"),
        room("dr", "show-and-tell"),
        room("dr", "developing"),
        room("dr", "gear-swap"),
    )
    val showAndTell = darkroomChannels.first { it.name == "show-and-tell" }

    private fun dm(p: Person, unread: Int = 0) = ChannelItem(
        jid = p.jid, name = p.name, kind = ChannelKind.Direct, category = null,
        joined = true, lastActivity = null, unread = unread.toUInt(), blocked = false, members = null,
    )

    val lanternCore = ChannelItem(
        jid = "lantern-core@$ROOMS", name = "Lantern core", kind = ChannelKind.Room, category = null,
        joined = true, lastActivity = null, unread = 2u, blocked = false, members = 4u,
    )

    /** Home, newest first. */
    val homeChannels = listOf(dm(theo, unread = 1), lanternCore, dm(priya), dm(noor), dm(ada), dm(jules))

    // ---------------------------------------------------------------- contacts

    private fun contact(p: Person) = Contact(
        jid = p.jid, name = p.name, subscription = SubscriptionState.BOTH, ask = false, groups = emptyList(),
        approved = false, blocked = false, online = p.online, show = p.show, status = p.status, idleSince = null,
        activity = null,
    )

    val contacts: List<Contact> = people.map(::contact) + listOf(
        // The incoming request from Mika.
        Contact(
            jid = "mika@xmpp.example.net", name = "Mika Sato", subscription = SubscriptionState.NONE, ask = false,
            groups = emptyList(), approved = false, blocked = false, online = false, show = null, status = null,
            idleSince = null, activity = null,
        ),
        // The outgoing request to Ines.
        Contact(
            jid = "ines@xmpp.example.net", name = "Ines Duarte", subscription = SubscriptionState.NONE, ask = true,
            groups = emptyList(), approved = false, blocked = false, online = false, show = null, status = null,
            idleSince = null, activity = null,
        ),
        Contact(
            jid = "promo@spam.example", name = null, subscription = SubscriptionState.NONE, ask = false,
            groups = emptyList(), approved = false, blocked = true, online = false, show = null, status = null,
            idleSince = null, activity = null,
        ),
    )

    /** One incoming request: Mika. */
    const val PENDING_CONTACTS = 1

    // ---------------------------------------------------------------- members

    private fun member(p: Person, affiliation: String, role: String = "participant") = MemberItem(
        id = p.key, name = p.name.substringBefore(' '), jid = p.jid, role = role, affiliation = affiliation,
        show = p.show, status = p.status, online = p.online, avatar = null,
    )

    val lanternMembers = listOf(
        member(priya, "owner", "moderator"),
        member(maya, "admin", "moderator"),
        member(theo, "admin", "moderator"),
        member(kenji, "member"),
        member(ada, "member"),
        member(jules, "member"),
        member(sam, "member"),
        member(noor, "member"),
        member(luis, "member"),
        member(wren, "member"),
    )

    // ---------------------------------------------------------------- profile

    val priyaProfile = ProfileState(
        address = priya.jid, name = priya.name, presence = priya.presence, isContact = true,
        affiliation = "owner", role = "moderator", fullName = priya.name,
        sharedSpaces = listOf(lanternWorks.item, slowReaders.item),
    )

    // ---------------------------------------------------------------- conversations

    /** Today in the story, in UTC (the tests run in UTC). */
    private fun at(hour: Int, minute: Int): Long =
        LocalDateTime.of(2026, 10, 4, hour, minute).toInstant(ZoneOffset.UTC).toEpochMilli()

    const val LIGHTHOUSE_URL = "https://upload.chat.foid.space/f3a9/lighthouse-fog.png"
    const val HARBOR_URL = "https://upload.chat.foid.space/7c1e/harbor-sunset.png"

    /** The URL of each attachment, to the file in test resources/showcase. */
    val attachmentArt = mapOf(LIGHTHOUSE_URL to "lighthouse-fog.png", HARBOR_URL to "harbor-sunset.png")

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
            val outgoing = p == maya
            // The name the room shows: the nick, which is the first name.
            val nick = p.name.substringBefore(' ')
            val m = MessageUi(
                id = "m:${out.size + 1}", senderId = p.jid, senderName = nick, avatarUrl = null, body = body,
                timestamp = ts, timeLabel = clockLabel(ts), stamp = "today ${clockLabel(ts)}", outgoing = outgoing,
                sameSenderAsPrevious = false, edited = false, retracted = false, reactions = reactions,
                reply = replyTo?.let { ReplyUi(it.id, it.senderName, replySnippet(it.body)) },
                attachment = attachment,
                formatted = formatMessage(body, palette, own, nick),
            )
            out += m
            return m
        }
    }

    private val ownNames = listOf("Maya", "maya", maya.jid)

    /** Lantern Works / #playtest, newest first. The New line is above message 11. */
    fun playtestRows(palette: FormatPalette): List<TimelineRowUi> {
        val b = Builder(palette, ownNames)
        b.msg(priya, 9, 12, "Build 0.14.2 is up on the playtest branch. New save system, controller remapping, and the lighthouse level is finally in.")
        b.msg(priya, 9, 12, "Please break it 🙏")
        b.msg(theo, 9, 20, "downloading")
        val jules4 = b.msg(
            jules, 9, 47,
            "Played the lighthouse twice. The fog at the top looks great, but I lost the rope prompt both times. It spawns behind the camera.",
            attachment = LIGHTHOUSE_URL,
        )
        b.msg(priya, 9, 51, "Good catch. The prompt is placed in world space, I'll pin it to the screen edge when it's off camera.", replyTo = jules4)
        b.msg(
            kenji, 9, 53,
            "something like this?\n```gdscript\nif not camera.is_position_in_frustum(prompt.global_position):\n    prompt.pin_to_edge(camera)\n```",
        )
        b.msg(priya, 9, 54, "yes, almost exactly. Want to open a PR?", reactions = listOf(ReactionUi("👍", 3, true)))
        b.msg(kenji, 9, 54, "on it")
        b.msg(maya, 10, 2, "@Theo can you check that old saves still load? I don't want to ship 0.14 if anyone loses progress.")
        b.msg(theo, 10, 5, "On it. Three saves from 0.13 so far, all load fine. The cloud one takes about 4 s, I'll profile it.")
        val ada11 = b.msg(ada, 10, 31, "Draft of the patch notes: https://lanternworks.example/notes/0-14")
        b.msg(sam, 10, 40, "Read it. I'd lead with remapping, that's what people asked for most in the survey.", reactions = listOf(ReactionUi("❤️", 2, false)))
        b.msg(ada, 10, 42, "fair, swapping them")
        return buildTimelineRows(b.out, firstUnreadId = ada11.id)
    }

    /** Darkroom / #show-and-tell, newest first. */
    fun showAndTellRows(palette: FormatPalette): List<TimelineRowUi> {
        val b = Builder(palette, ownNames)
        b.msg(noor, 8, 14, "Portra 400, pushed one stop. Harbor at the end of the day.", attachment = HARBOR_URL)
        b.msg(wren, 8, 31, "the color in the water 😮")
        b.msg(noor, 8, 33, "Lab scan, no edits.")
        b.msg(jules, 9, 2, "Which lab?")
        b.msg(noor, 9, 6, "The one on 3rd, they're slow but careful.")
        return buildTimelineRows(b.out)
    }

    /** Home / Theo, newest first. Theo's last message is unread. */
    fun theoRows(palette: FormatPalette): List<TimelineRowUi> {
        val b = Builder(palette, ownNames)
        b.msg(theo, 11, 48, "lunch after the playtest call?")
        b.msg(maya, 11, 50, "Yes. Ramen place?")
        val last = b.msg(theo, 11, 51, "perfect")
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
        (people + maya).mapNotNull { p -> p.art?.let { p.jid to it } }.toMap() +
            spaceList.mapNotNull { s -> s.art?.let { s.owner to it } }.toMap()
}

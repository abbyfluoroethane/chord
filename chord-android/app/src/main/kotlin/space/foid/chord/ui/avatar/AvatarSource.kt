package space.foid.chord.ui.avatar

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withTimeoutOrNull
import uniffi.chord_ffi.ChordClient
import uniffi.chord_ffi.MemberDiff
import uniffi.chord_ffi.MemberItem
import uniffi.chord_ffi.MemberListListener

/** The stored avatar of one owner. [bytes] is null until the image has arrived. */
class AvatarData(val hash: String, val bytes: ByteArray?)

/**
 * The calls that [AvatarRepository] makes on the core. A small interface over [ChordClient], so
 * that the repository runs in JVM tests with a fake. [ClientAvatarSource] is the real one.
 */
interface AvatarSource {
    /** The stored avatar of a bare JID, or null if the core knows none. */
    suspend fun avatar(owner: String): AvatarData?

    /** Ask the server for the avatar of a bare JID again. The data arrives later. */
    suspend fun refresh(owner: String)

    /**
     * The bare JID that stands behind [occupant], a `room@service/nick` or `jid/resource` address,
     * or null if it is not known. The core takes only bare JIDs as avatar owners.
     */
    suspend fun realJid(occupant: String): String?
}

/** [AvatarSource] on a [ChordClient]. It holds no protocol logic. */
class ClientAvatarSource(private val client: ChordClient) : AvatarSource {
    private val lock = Mutex()
    private val rooms = HashMap<String, Pair<Long, List<MemberItem>>>()

    override suspend fun avatar(owner: String): AvatarData? =
        client.avatar(owner)?.let { AvatarData(it.hash, it.data) }

    override suspend fun refresh(owner: String) = client.refreshAvatar(owner)

    override suspend fun realJid(occupant: String): String? {
        val room = occupant.substringBefore('/')
        val nick = occupant.substringAfter('/')
        val members = members(room) ?: return null
        // In a room the member id is the nick. In a 1:1 chat it is a bare JID, and the part after the
        // slash is only a resource.
        return members.firstOrNull { it.id == nick }?.jid
            ?: members.firstOrNull { it.id == room }?.jid
    }

    /** A snapshot of the member list of [room], kept for a short time. */
    private suspend fun members(room: String): List<MemberItem>? = lock.withLock {
        val now = System.currentTimeMillis()
        rooms[room]?.takeIf { now - it.first < SNAPSHOT_TTL_MS }?.let { return@withLock it.second }
        val first = CompletableDeferred<List<MemberItem>>()
        val sub = client.subscribeMemberList(room, object : MemberListListener {
            override fun onDiff(diff: MemberDiff) {
                if (diff is MemberDiff.Reset) first.complete(diff.items)
            }
        })
        try {
            withTimeoutOrNull(SNAPSHOT_WAIT_MS) { first.await() }
        } finally {
            runCatching { sub.cancel(); sub.close() }
        }?.also { rooms[room] = now to it }
    }

    private companion object {
        const val SNAPSHOT_TTL_MS = 30_000L
        const val SNAPSHOT_WAIT_MS = 3_000L
    }
}

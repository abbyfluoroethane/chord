package space.foid.chord.ui.avatar

import uniffi.chord_ffi.ChordClient

/** The stored avatar of one key. [bytes] is null until the image has arrived. */
class AvatarData(val hash: String, val bytes: ByteArray?)

/**
 * The calls that [AvatarRepository] makes on the core. A small interface over [ChordClient], so
 * that the repository runs in JVM tests with a fake. [ClientAvatarSource] is the real one.
 */
interface AvatarSource {
    /**
     * The stored avatar for [key], or null if the core knows none. [key] is what the views
     * carry: an avatar hash (`TimelineItem.avatar`), or an owner key. An owner key is a bare JID,
     * `room@service/nick` for a room occupant, or `service/node` for a space.
     */
    suspend fun avatar(key: String): AvatarData?

    /** Ask the server for the avatar of [owner] again. The data arrives later. */
    suspend fun refresh(owner: String)
}

/** [AvatarSource] on a [ChordClient]. It holds no protocol logic. */
class ClientAvatarSource(private val client: ChordClient) : AvatarSource {
    override suspend fun avatar(key: String): AvatarData? =
        client.avatar(key)?.let { AvatarData(it.hash, it.data) }

    override suspend fun refresh(owner: String) = client.refreshAvatar(owner)
}

package space.foid.chord.ui.avatar

import androidx.compose.animation.Crossfade
import androidx.compose.animation.core.tween
import androidx.compose.runtime.Composable
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Dp
import space.foid.chord.ChordApp
import space.foid.chord.ui.components.Avatar
import space.foid.chord.ui.components.Presence
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordMotion
import space.foid.chord.ui.theme.ChordSize

/** The repository for avatars below this point. Null: use the one of [ChordApp], if any. */
val LocalAvatarRepository = compositionLocalOf<AvatarRepository?> { null }

/**
 * The avatar bitmap of [owner], or null while it loads or if there is none. The state is made
 * with `produceState` keyed by owner and hash, so a row recomposes only when its own bitmap arrives.
 */
@Composable
fun rememberAvatarBitmap(owner: String, hash: String?, size: Dp): ImageBitmap? {
    val repo = LocalAvatarRepository.current
        ?: (LocalContext.current.applicationContext as? ChordApp)?.avatars
    val px = with(LocalDensity.current) { size.roundToPx() }
    val image by produceState(repo?.peek(owner, hash, px), repo, owner, hash, px) {
        if (repo != null) repo.load(owner, hash, px)?.let { value = it }
    }
    return image
}

/**
 * An [Avatar] with the real picture of [owner] when there is one. The initials show first, then
 * the picture fades in.
 *
 * @param owner a bare JID, or `room@service/nick` for a room occupant.
 * @param hash the hash from the view item (`TimelineItem.avatar`, `MemberItem.avatar`), if any.
 */
@Composable
fun JidAvatar(
    owner: String,
    name: String,
    size: Dp = ChordSize.avatar,
    presence: Presence? = null,
    hash: String? = null,
    cut: Color = Chord.colors.surface100,
    modifier: Modifier = Modifier,
) {
    val image = rememberAvatarBitmap(owner, hash, size)
    JidAvatarContent(owner, name, image, size, presence, cut, modifier)
}

/** Stateless [JidAvatar]: draws [image], or the initials when it is null. */
@Composable
fun JidAvatarContent(
    owner: String,
    name: String,
    image: ImageBitmap?,
    size: Dp = ChordSize.avatar,
    presence: Presence? = null,
    cut: Color = Chord.colors.surface100,
    modifier: Modifier = Modifier,
) {
    val spec = remember { tween<Float>(ChordMotion.FAST) }
    Crossfade(targetState = image, modifier = modifier, animationSpec = spec, label = "avatar") { shown ->
        Avatar(jid = owner, name = name, image = shown, size = size, presence = presence, cut = cut)
    }
}

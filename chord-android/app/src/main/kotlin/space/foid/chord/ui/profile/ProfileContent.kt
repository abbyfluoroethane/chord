package space.foid.chord.ui.profile

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.em
import androidx.compose.ui.unit.sp
import space.foid.chord.R
import space.foid.chord.ui.avatar.JidAvatar
import space.foid.chord.ui.components.Presence
import space.foid.chord.ui.components.avatarInitials
import space.foid.chord.ui.components.avatarTint
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordRadius
import space.foid.chord.ui.theme.ChordSpace
import space.foid.chord.ui.theme.ChordType
import space.foid.chord.viewmodel.ProfileNotice
import space.foid.chord.viewmodel.ProfileState

/** The sheet and the right rail of a chat show the same profile. The rail has no Message button. */
enum class ProfileVariant { Sheet, Rail }

/** What the buttons of a profile do. */
class ProfileCallbacks(
    val onMessage: () -> Unit = {},
    val onEditProfile: () -> Unit = {},
    val onAddContact: () -> Unit = {},
    val onUnblock: () -> Unit = {},
    val onMore: () -> Unit = {},
    val onCopyAddress: () -> Unit = {},
    /** Only the rail has the link "View full profile". */
    val onViewFull: (() -> Unit)? = null,
)

@Composable
fun presenceText(p: Presence): String = stringResource(
    when (p) {
        Presence.Online -> R.string.people_presence_online
        Presence.Away -> R.string.people_presence_away
        Presence.Dnd -> R.string.people_presence_dnd
        Presence.Offline -> R.string.people_presence_offline
    },
)

@Composable
fun noticeText(n: ProfileNotice): String = stringResource(
    when (n) {
        ProfileNotice.ContactAdded -> R.string.people_notice_added
        ProfileNotice.ContactRemoved -> R.string.people_notice_removed
        ProfileNotice.Renamed -> R.string.people_notice_renamed
        ProfileNotice.Blocked -> R.string.people_notice_blocked
        ProfileNotice.Unblocked -> R.string.people_notice_unblocked
        ProfileNotice.Invited -> R.string.people_notice_invited
        ProfileNotice.Failed -> R.string.people_notice_failed
    },
)

/**
 * The profile: banner, big avatar, name, address, status, actions, then the sections. Stateless:
 * [ProfileSheet] and [DmProfilePane] feed it. [cut] is the colour behind the card (the avatar ring).
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun ProfileContent(
    state: ProfileState,
    variant: ProfileVariant,
    callbacks: ProfileCallbacks,
    cut: Color,
    modifier: Modifier = Modifier,
    notice: ProfileNotice? = null,
) {
    val c = Chord.colors
    Column(modifier.fillMaxWidth().testTag("profile_content"), verticalArrangement = Arrangement.spacedBy(ChordSpace.s3)) {
        Box(Modifier.fillMaxWidth()) {
            Box(
                Modifier.fillMaxWidth().height(88.dp).background(avatarTint(state.address))
                    .drawBehind {
                        drawLine(c.line, Offset(0f, size.height), Offset(size.width, size.height), 1.dp.toPx())
                    },
            )
            Box(Modifier.padding(top = 44.dp, start = ChordSpace.s4).background(cut, CircleShape).padding(4.dp)) {
                JidAvatar(
                    owner = state.address, name = state.name, size = 80.dp, presence = state.presence,
                    hash = state.avatarHash, cut = cut,
                )
            }
        }

        Column(Modifier.padding(horizontal = ChordSpace.s4), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    state.name,
                    style = ChordType.title.copy(fontWeight = FontWeight.SemiBold),
                    color = if (state.isMe) c.brandInk else c.ink,
                    maxLines = 2, overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f, fill = false).testTag("profile_name"),
                )
            }
            if (state.hidden) {
                Text(stringResource(R.string.people_address_hidden), style = ChordType.caption, color = c.inkMuted)
            } else {
                val copy = stringResource(R.string.people_copy_address)
                Row(
                    Modifier.clickable(role = Role.Button, onClick = callbacks.onCopyAddress)
                        .semantics { contentDescription = copy + ", " + state.address }
                        .testTag("profile_address"),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(ChordSpace.s2),
                ) {
                    Text(
                        state.address, style = ChordType.code.copy(fontSize = 13.sp), color = c.inkMuted,
                        maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f, fill = false),
                    )
                    CopyGlyph(c.inkMuted)
                }
            }
            if (state.isBlocked) {
                Box(
                    Modifier.padding(top = ChordSpace.s1)
                        .border(BorderStroke(1.dp, c.danger), RoundedCornerShape(ChordRadius.sm))
                        .padding(horizontal = ChordSpace.s2),
                ) { Text(stringResource(R.string.people_blocked), style = ChordType.caption, color = c.danger) }
            }
            Text(
                state.status ?: state.presence?.let { presenceText(it) }.orEmpty(),
                style = ChordType.bodySmall.copy(fontSize = 14.sp), color = c.ink,
                modifier = Modifier.padding(top = ChordSpace.s1).testTag("profile_status"),
            )
        }

        Row(Modifier.padding(horizontal = ChordSpace.s4), horizontalArrangement = Arrangement.spacedBy(ChordSpace.s2)) {
            if (state.isMe) {
                ActionButton(stringResource(R.string.people_edit_profile), primary = true, onClick = callbacks.onEditProfile, modifier = Modifier.weight(1f).testTag("profile_edit"))
            } else {
                if (variant == ProfileVariant.Sheet) {
                    ActionButton(stringResource(R.string.people_message), primary = true, onClick = callbacks.onMessage, modifier = Modifier.weight(1f).testTag("profile_message"))
                }
                when {
                    state.isBlocked -> ActionButton(stringResource(R.string.people_unblock), onClick = callbacks.onUnblock, modifier = Modifier.weight(1f))
                    state.isContact -> Row(
                        Modifier.weight(1f).height(40.dp), horizontalArrangement = Arrangement.Center,
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        CheckGlyph(c.inkMuted)
                        Text(
                            stringResource(R.string.people_contact), style = ChordType.label, color = c.inkMuted,
                            modifier = Modifier.padding(start = ChordSpace.s1),
                        )
                    }
                    else -> ActionButton(
                        stringResource(R.string.people_add_contact), enabled = !state.hidden,
                        onClick = callbacks.onAddContact, modifier = Modifier.weight(1f).testTag("profile_add_contact"),
                    )
                }
            }
            val more = stringResource(R.string.people_more)
            Box(
                Modifier.size(40.dp).clip(RoundedCornerShape(ChordRadius.md)).background(c.surface300)
                    .clickable(role = Role.Button, onClick = callbacks.onMore)
                    .semantics { contentDescription = more }.testTag("profile_more"),
                contentAlignment = Alignment.Center,
            ) { DotsGlyph(c.ink) }
        }
        if (notice != null) {
            Text(
                noticeText(notice), style = ChordType.bodySmall,
                color = if (notice == ProfileNotice.Failed) c.danger else c.inkMuted,
                modifier = Modifier.padding(horizontal = ChordSpace.s4).testTag("profile_notice"),
            )
        }

        val hasPanel = state.fullName != null || state.nickname != null || state.affiliation != null || state.role != null
        if (hasPanel) {
            Column(
                Modifier.padding(horizontal = ChordSpace.s4).fillMaxWidth()
                    .clip(RoundedCornerShape(ChordRadius.md)).background(c.surface100)
                    .border(BorderStroke(1.dp, c.line), RoundedCornerShape(ChordRadius.md))
                    .padding(ChordSpace.s3),
                verticalArrangement = Arrangement.spacedBy(ChordSpace.s3),
            ) {
                state.fullName?.let { Section(stringResource(R.string.people_full_name)) { Text(it, style = ChordType.bodySmall.copy(fontSize = 14.sp), color = c.ink) } }
                state.nickname?.let { Section(stringResource(R.string.people_nickname)) { Text(it, style = ChordType.bodySmall.copy(fontSize = 14.sp), color = c.ink) } }
                if (state.affiliation != null || state.role != null) {
                    Section(stringResource(R.string.people_roles)) {
                        FlowRow(horizontalArrangement = Arrangement.spacedBy(ChordSpace.s1), verticalArrangement = Arrangement.spacedBy(ChordSpace.s1)) {
                            state.affiliation?.let { Chip(it.replaceFirstChar { ch -> ch.uppercase() }, shield = false) }
                            state.role?.let { Chip(it.replaceFirstChar { ch -> ch.uppercase() }, shield = it.equals("moderator", ignoreCase = true)) }
                        }
                    }
                }
            }
        }

        if (!state.hidden) {
            Column(Modifier.padding(horizontal = ChordSpace.s4), verticalArrangement = Arrangement.spacedBy(ChordSpace.s1)) {
                Row(horizontalArrangement = Arrangement.spacedBy(ChordSpace.s2)) {
                    Heading(stringResource(R.string.people_shared_spaces))
                    Text(state.sharedSpaces.size.toString(), style = ChordType.caption, color = c.inkMuted)
                }
                if (state.sharedSpaces.isEmpty()) {
                    Text(stringResource(R.string.people_no_shared_spaces), style = ChordType.bodySmall, color = c.inkMuted)
                }
                state.sharedSpaces.forEach { sp ->
                    Row(
                        Modifier.fillMaxWidth().height(44.dp).testTag("shared_space"),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(ChordSpace.s3),
                    ) {
                        Box(
                            Modifier.size(32.dp).clip(RoundedCornerShape(10.dp)).background(avatarTint(sp.service + sp.node)),
                            contentAlignment = Alignment.Center,
                        ) {
                            Text(avatarInitials(sp.name), style = ChordType.caption.copy(fontWeight = FontWeight.SemiBold), color = Color(0xFFF6F4EF))
                        }
                        Text(sp.name, style = ChordType.label, color = c.ink, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    }
                }
            }
        }
        callbacks.onViewFull?.let { open ->
            Text(
                stringResource(R.string.people_profile) + " ›", style = ChordType.label, color = c.accent,
                modifier = Modifier.padding(horizontal = ChordSpace.s4).clickable(role = Role.Button, onClick = open),
            )
        }
        Box(Modifier.height(ChordSpace.s4))
    }
}

private val Int.sp get() = androidx.compose.ui.unit.TextUnit(this.toFloat(), androidx.compose.ui.unit.TextUnitType.Sp)

@Composable
private fun ActionButton(text: String, onClick: () -> Unit, modifier: Modifier = Modifier, primary: Boolean = false, enabled: Boolean = true) {
    val c = Chord.colors
    Box(
        modifier.height(40.dp).clip(RoundedCornerShape(ChordRadius.md))
            .background(if (primary) c.brand else c.surface300)
            .clickable(enabled = enabled, role = Role.Button, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            text, style = ChordType.label,
            color = (if (primary) c.onBrand else c.ink).copy(alpha = if (enabled) 1f else 0.4f),
            maxLines = 1, textAlign = TextAlign.Center,
        )
    }
}

@Composable
private fun Heading(text: String) {
    Text(
        text.uppercase(),
        style = ChordType.caption.copy(fontWeight = FontWeight.Bold, letterSpacing = 0.06.em),
        color = Chord.colors.inkMuted,
    )
}

@Composable
private fun Section(title: String, content: @Composable () -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Heading(title)
        content()
    }
}

@Composable
private fun Chip(text: String, shield: Boolean) {
    val c = Chord.colors
    Row(
        Modifier.border(BorderStroke(1.dp, c.line), RoundedCornerShape(ChordRadius.sm)).padding(horizontal = ChordSpace.s2, vertical = 2.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (shield) {
            ShieldGlyph(c.ink, 12.dp)
            Box(Modifier.width(ChordSpace.s1))
        }
        Text(text, style = ChordType.caption.copy(fontWeight = FontWeight.Medium), color = c.ink)
    }
}

/** The shield of a moderator, a 24 unit line icon. */
@Composable
fun ShieldGlyph(tint: Color, size: androidx.compose.ui.unit.Dp = 13.dp) {
    Box(
        Modifier.size(size).drawBehind {
            val k = this.size.width / 24f
            val p = Path().apply {
                moveTo(12 * k, 3 * k); lineTo(20 * k, 6 * k); lineTo(20 * k, 12 * k)
                cubicTo(20 * k, 17 * k, 16 * k, 20 * k, 12 * k, 21.5f * k)
                cubicTo(8 * k, 20 * k, 4 * k, 17 * k, 4 * k, 12 * k)
                lineTo(4 * k, 6 * k); close()
            }
            drawPath(p, tint, style = Stroke(2f * k, cap = StrokeCap.Round, join = StrokeJoin.Round))
        },
    )
}

@Composable
private fun CopyGlyph(tint: Color) = Box(
    Modifier.size(14.dp).drawBehind {
        val k = size.width / 24f
        val st = Stroke(2f * k, cap = StrokeCap.Round, join = StrokeJoin.Round)
        drawRoundRect(tint, Offset(9 * k, 9 * k), androidx.compose.ui.geometry.Size(11 * k, 11 * k), androidx.compose.ui.geometry.CornerRadius(2 * k), style = st)
        drawPath(Path().apply { moveTo(5 * k, 15 * k); lineTo(5 * k, 6 * k); quadraticTo(5 * k, 4 * k, 7 * k, 4 * k); lineTo(15 * k, 4 * k) }, tint, style = st)
    },
)

@Composable
private fun CheckGlyph(tint: Color) = Box(
    Modifier.size(16.dp).drawBehind {
        val k = size.width / 24f
        drawPath(
            Path().apply { moveTo(5 * k, 12.5f * k); lineTo(10 * k, 17.5f * k); lineTo(19 * k, 7 * k) },
            tint, style = Stroke(2f * k, cap = StrokeCap.Round, join = StrokeJoin.Round),
        )
    },
)

@Composable
private fun DotsGlyph(tint: Color) = Box(
    Modifier.size(20.dp).drawBehind {
        val k = size.width / 24f
        for (x in listOf(5f, 12f, 19f)) drawCircle(tint, 1.8f * k, Offset(x * k, 12 * k))
    },
)

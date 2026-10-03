package space.foid.chord.ui.join

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import space.foid.chord.R
import space.foid.chord.ui.sheets.ChordModalSheet
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordRadius
import space.foid.chord.ui.theme.ChordSpace
import space.foid.chord.ui.theme.ChordType
import space.foid.chord.viewmodel.JoinState
import space.foid.chord.viewmodel.JoinTab
import space.foid.chord.viewmodel.JoinViewModel
import space.foid.chord.viewmodel.SpaceRow
import space.foid.chord.viewmodel.SpaceStatus
import space.foid.chord.viewmodel.SpacesState

/** What the buttons of the new conversation sheet do. */
class JoinCallbacks(
    val onTab: (JoinTab) -> Unit = {},
    val onAddress: (String) -> Unit = {},
    val onNick: (String) -> Unit = {},
    val onPassword: (String) -> Unit = {},
    val onJoin: () -> Unit = {},
    val onPersonJid: (String) -> Unit = {},
    val onPersonName: (String) -> Unit = {},
    val onMessage: () -> Unit = {},
    val onReloadSpaces: () -> Unit = {},
    val onJoinSpace: (SpaceRow) -> Unit = {},
) {
    companion object {
        fun of(vm: JoinViewModel) = JoinCallbacks(
            onTab = vm::selectTab, onAddress = vm::onAddress, onNick = vm::onNick, onPassword = vm::onPassword,
            onJoin = vm::joinRoom, onPersonJid = vm::onPersonJid, onPersonName = vm::onPersonName,
            onMessage = vm::messagePerson, onReloadSpaces = { vm.loadSpaces(force = true) }, onJoinSpace = vm::joinSpace,
        )
    }
}

/**
 * The sheet to start a conversation: join a room, message someone, or browse spaces.
 * [onDismiss] runs when the user closes it.
 */
@Composable
fun NewConversationSheet(state: JoinState, callbacks: JoinCallbacks, onDismiss: () -> Unit) {
    ChordModalSheet(onDismiss) { _ ->
        NewConversationContent(state, callbacks)
    }
}

/** The inside of [NewConversationSheet], with no sheet window. */
@Composable
fun NewConversationContent(state: JoinState, callbacks: JoinCallbacks, modifier: Modifier = Modifier) {
    Column(
        modifier
            .fillMaxWidth()
            .navigationBarsPadding()
            .imePadding()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = ChordSpace.s4)
            .padding(bottom = ChordSpace.s4)
            .testTag("new_conversation_sheet"),
    ) {
        JoinTitle(stringResource(R.string.join_title))
        Spacer(Modifier.height(ChordSpace.s3))
        TabBar(state.tab, callbacks.onTab)
        Spacer(Modifier.height(ChordSpace.s4))
        when (state.tab) {
            JoinTab.Room -> RoomPart(state, callbacks)
            JoinTab.Person -> PersonPart(state, callbacks)
            JoinTab.Spaces -> SpacesPart(state.spaces, callbacks)
        }
    }
}

@Composable
private fun TabBar(tab: JoinTab, onTab: (JoinTab) -> Unit) {
    val c = Chord.colors
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(ChordRadius.md)).background(c.surface300).padding(3.dp),
        horizontalArrangement = Arrangement.spacedBy(3.dp),
    ) {
        listOf(
            Triple(JoinTab.Room, R.string.join_tab_room, "tab_room"),
            Triple(JoinTab.Person, R.string.join_tab_person, "tab_person"),
            Triple(JoinTab.Spaces, R.string.join_tab_spaces, "tab_spaces"),
        ).forEach { (t, label, tag) ->
            val on = t == tab
            Box(
                Modifier
                    .weight(1f)
                    .height(40.dp)
                    .clip(RoundedCornerShape(ChordRadius.md - 2.dp))
                    .background(if (on) c.surfaceRaised else androidx.compose.ui.graphics.Color.Transparent)
                    .clickable(role = Role.Tab) { onTab(t) }
                    .semantics { selected = on }
                    .testTag(tag),
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    stringResource(label), style = ChordType.label,
                    color = if (on) c.ink else c.inkMuted, maxLines = 1, overflow = TextOverflow.Ellipsis,
                )
            }
        }
    }
}

@Composable
private fun Hint(text: String) {
    Text(text, style = ChordType.bodySmall, color = Chord.colors.inkMuted, modifier = Modifier.fillMaxWidth())
}

@Composable
private fun RoomPart(state: JoinState, cb: JoinCallbacks) {
    Hint(stringResource(R.string.join_room_hint))
    Spacer(Modifier.height(ChordSpace.s3))
    JoinField(
        value = state.address, onChange = cb.onAddress,
        label = stringResource(R.string.join_address), tag = "join_address",
        placeholder = stringResource(R.string.join_address_placeholder),
        enabled = !state.joining,
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri, imeAction = ImeAction.Next),
        isError = state.roomError != null && state.passwordPrompt == null,
    )
    Spacer(Modifier.height(ChordSpace.s3))
    JoinField(
        value = state.nick, onChange = cb.onNick,
        label = stringResource(R.string.join_nick), tag = "join_nick",
        placeholder = stringResource(R.string.join_nick_placeholder),
        enabled = !state.joining,
        keyboardOptions = KeyboardOptions(imeAction = if (state.passwordPrompt == null) ImeAction.Go else ImeAction.Next),
        keyboardActions = KeyboardActions(onGo = { if (state.canJoin) cb.onJoin() }),
    )
    if (state.passwordPrompt != null) {
        Spacer(Modifier.height(ChordSpace.s3))
        Text(
            stringResource(R.string.join_password_needed),
            style = ChordType.bodySmall, color = Chord.colors.ink,
            modifier = Modifier.fillMaxWidth().testTag("join_password_prompt"),
        )
        Spacer(Modifier.height(ChordSpace.s2))
        JoinField(
            value = state.password, onChange = cb.onPassword,
            label = stringResource(R.string.join_password), tag = "join_password",
            enabled = !state.joining,
            visualTransformation = PasswordVisualTransformation(),
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password, imeAction = ImeAction.Go),
            keyboardActions = KeyboardActions(onGo = { if (state.canJoin) cb.onJoin() }),
            isError = state.passwordPrompt.wrong,
        )
    }
    if (state.roomError != null) {
        Spacer(Modifier.height(ChordSpace.s2))
        JoinError(state.roomError, "join_error")
    }
    Spacer(Modifier.height(ChordSpace.s4))
    JoinButton(
        text = stringResource(if (state.joining) R.string.join_joining else R.string.join_submit),
        onClick = cb.onJoin, tag = "join_submit", modifier = Modifier.fillMaxWidth(),
        enabled = state.canJoin, busy = state.joining,
    )
}

@Composable
private fun PersonPart(state: JoinState, cb: JoinCallbacks) {
    Hint(stringResource(R.string.join_person_hint))
    Spacer(Modifier.height(ChordSpace.s3))
    JoinField(
        value = state.personJid, onChange = cb.onPersonJid,
        label = stringResource(R.string.join_address), tag = "person_jid",
        placeholder = stringResource(R.string.join_person_placeholder),
        enabled = !state.messaging,
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri, imeAction = ImeAction.Next),
        isError = state.personError != null,
    )
    Spacer(Modifier.height(ChordSpace.s3))
    JoinField(
        value = state.personName, onChange = cb.onPersonName,
        label = stringResource(R.string.join_person_name), tag = "person_name",
        placeholder = stringResource(R.string.join_person_name_placeholder),
        enabled = !state.messaging,
        keyboardOptions = KeyboardOptions(imeAction = ImeAction.Go),
        keyboardActions = KeyboardActions(onGo = { if (state.canMessage) cb.onMessage() }),
    )
    if (state.personError != null) {
        Spacer(Modifier.height(ChordSpace.s2))
        JoinError(state.personError, "person_error")
    }
    Spacer(Modifier.height(ChordSpace.s4))
    JoinButton(
        text = stringResource(if (state.messaging) R.string.join_opening else R.string.join_person_submit),
        onClick = cb.onMessage, tag = "person_submit", modifier = Modifier.fillMaxWidth(),
        enabled = state.canMessage, busy = state.messaging,
    )
}

@Composable
private fun SpacesPart(spaces: SpacesState, cb: JoinCallbacks) {
    val c = Chord.colors
    Hint(stringResource(R.string.join_spaces_hint))
    Spacer(Modifier.height(ChordSpace.s3))
    when (spaces) {
        SpacesState.Idle, SpacesState.Loading -> Row(
            Modifier.fillMaxWidth().height(96.dp).testTag("spaces_loading"),
            horizontalArrangement = Arrangement.Center, verticalAlignment = Alignment.CenterVertically,
        ) {
            CircularProgressIndicator(Modifier.size(24.dp), color = c.brand, strokeWidth = 2.dp)
        }
        is SpacesState.Failed -> Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(ChordSpace.s3)) {
            JoinError(spaces.message, "spaces_error")
            JoinTextButton(stringResource(R.string.join_retry), cb.onReloadSpaces, "spaces_retry")
        }
        is SpacesState.Loaded -> if (spaces.rows.isEmpty()) {
            Text(
                stringResource(R.string.join_spaces_empty), style = ChordType.body, color = c.inkMuted,
                modifier = Modifier.fillMaxWidth().padding(vertical = ChordSpace.s4).testTag("spaces_empty"),
            )
        } else {
            LazyColumn(
                Modifier.fillMaxWidth().heightIn(max = 360.dp).testTag("spaces_list"),
                verticalArrangement = Arrangement.spacedBy(ChordSpace.s2),
            ) {
                items(spaces.rows, key = { it.key }) { row -> SpaceCard(row) { cb.onJoinSpace(row) } }
            }
        }
    }
}

@Composable
private fun SpaceCard(row: SpaceRow, onJoin: () -> Unit) {
    val c = Chord.colors
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(ChordRadius.md)).background(c.surface300)
            .padding(ChordSpace.s3).testTag("space_row_${row.info.node}"),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(ChordSpace.s3),
    ) {
        Column(Modifier.weight(1f)) {
            Text(row.info.name.ifBlank { row.info.node }, style = ChordType.name, color = c.ink, maxLines = 1, overflow = TextOverflow.Ellipsis)
            val about = row.info.description?.takeIf { it.isNotBlank() }
            if (about != null) {
                Text(about, style = ChordType.bodySmall, color = c.inkMuted, maxLines = 2, overflow = TextOverflow.Ellipsis)
            }
            if (row.info.accessModel != null && row.info.accessModel != "open") {
                Text(stringResource(R.string.join_space_private), style = ChordType.caption, color = c.inkMuted)
            }
            if (row.error != null) JoinError(row.error, "space_error_${row.info.node}")
        }
        JoinButton(
            text = stringResource(
                when (row.status) {
                    SpaceStatus.Idle -> R.string.join_space_join
                    SpaceStatus.Joining -> R.string.join_space_joining
                    SpaceStatus.Requested -> R.string.join_space_requested
                },
            ),
            onClick = onJoin, tag = "space_join_${row.info.node}",
            enabled = row.status == SpaceStatus.Idle, busy = row.status == SpaceStatus.Joining, compact = true,
        )
    }
}

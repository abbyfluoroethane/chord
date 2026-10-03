package space.foid.chord.ui.spaces

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
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import space.foid.chord.R
import space.foid.chord.ui.join.JoinButton
import space.foid.chord.ui.join.JoinError
import space.foid.chord.ui.join.JoinField
import space.foid.chord.ui.join.JoinTextButton
import space.foid.chord.ui.join.JoinTitle
import space.foid.chord.ui.join.RadioRow
import space.foid.chord.ui.join.SpaceCard
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordRadius
import space.foid.chord.ui.theme.ChordSpace
import space.foid.chord.ui.theme.ChordType
import space.foid.chord.viewmodel.SpaceRow
import space.foid.chord.viewmodel.SpacesState
import uniffi.chord_ffi.SpaceAccess

/** The two tabs of "Add a space". */
enum class AddSpaceTab { Create, Join }

/** Plain label of who may join a space. */
fun accessLabel(access: SpaceAccess): Int = when (access) {
    SpaceAccess.OPEN -> R.string.spaces_access_open
    SpaceAccess.AUTHORIZE -> R.string.spaces_access_authorize
    SpaceAccess.WHITELIST -> R.string.spaces_access_whitelist
}

/** What the buttons of "Add a space" do. */
class AddSpaceCallbacks(
    val onTab: (AddSpaceTab) -> Unit = {},
    val onName: (String) -> Unit = {},
    val onDescription: (String) -> Unit = {},
    val onAccess: (SpaceAccess) -> Unit = {},
    val onCreate: () -> Unit = {},
    val onQuery: (String) -> Unit = {},
    val onReload: () -> Unit = {},
    val onJoin: (SpaceRow) -> Unit = {},
)

/** What "Add a space" shows. */
data class AddSpaceUi(
    val tab: AddSpaceTab = AddSpaceTab.Create,
    val name: String = "",
    val description: String = "",
    val access: SpaceAccess = SpaceAccess.OPEN,
    val query: String = "",
    val busy: Boolean = false,
    val error: String? = null,
)

/** The inside of the "Add a space" sheet (AddCircleModal.svelte): create a space, or join a public one. */
@Composable
fun AddSpaceContent(
    ui: AddSpaceUi,
    spaces: SpacesState,
    joined: Set<String>,
    cb: AddSpaceCallbacks,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier.fillMaxWidth().navigationBarsPadding().imePadding().verticalScroll(rememberScrollState())
            .padding(horizontal = ChordSpace.s4).padding(bottom = ChordSpace.s4).testTag("add_space_sheet"),
    ) {
        JoinTitle(stringResource(if (ui.tab == AddSpaceTab.Create) R.string.spaces_add_create_title else R.string.spaces_add_join_title))
        Spacer(Modifier.height(ChordSpace.s3))
        Tabs(ui.tab, cb.onTab)
        Spacer(Modifier.height(ChordSpace.s4))
        when (ui.tab) {
            AddSpaceTab.Create -> CreatePart(ui, cb)
            AddSpaceTab.Join -> JoinPart(ui, spaces, joined, cb)
        }
    }
}

@Composable
private fun Tabs(tab: AddSpaceTab, onTab: (AddSpaceTab) -> Unit) {
    val c = Chord.colors
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(ChordRadius.md)).background(c.surface300).padding(3.dp),
        horizontalArrangement = Arrangement.spacedBy(3.dp),
    ) {
        listOf(
            Triple(AddSpaceTab.Create, R.string.spaces_add_tab_create, "tab_create_space"),
            Triple(AddSpaceTab.Join, R.string.spaces_add_tab_join, "tab_join_space"),
        ).forEach { (t, label, tag) ->
            val on = t == tab
            Box(
                Modifier.weight(1f).height(40.dp).clip(RoundedCornerShape(ChordRadius.md - 2.dp))
                    .background(if (on) c.surfaceRaised else androidx.compose.ui.graphics.Color.Transparent)
                    .clickable(role = Role.Tab) { onTab(t) }
                    .semantics { selected = on }
                    .testTag(tag),
                contentAlignment = Alignment.Center,
            ) {
                Text(stringResource(label), style = ChordType.label, color = if (on) c.ink else c.inkMuted, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
    }
}

@Composable
private fun CreatePart(ui: AddSpaceUi, cb: AddSpaceCallbacks) {
    Text(stringResource(R.string.spaces_add_create_hint), style = ChordType.bodySmall, color = Chord.colors.inkMuted)
    Spacer(Modifier.height(ChordSpace.s3))
    JoinField(
        ui.name, cb.onName, stringResource(R.string.spaces_add_name), "add_space_name",
        placeholder = stringResource(R.string.spaces_add_name_placeholder), enabled = !ui.busy,
    )
    Spacer(Modifier.height(ChordSpace.s3))
    JoinField(
        ui.description, cb.onDescription, stringResource(R.string.spaces_add_description), "add_space_description",
        enabled = !ui.busy, singleLine = false, minLines = 2,
    )
    Text(
        stringResource(R.string.spaces_add_who).uppercase(), style = ChordType.caption, color = Chord.colors.inkMuted,
        modifier = Modifier.padding(top = ChordSpace.s4),
    )
    SpaceAccess.entries.forEach { a ->
        AccessRow(a, ui.access == a) { cb.onAccess(a) }
    }
    if (ui.error != null) {
        Spacer(Modifier.height(ChordSpace.s2))
        JoinError(ui.error, "add_space_error")
    }
    Spacer(Modifier.height(ChordSpace.s4))
    JoinButton(
        stringResource(if (ui.busy) R.string.spaces_add_creating else R.string.spaces_add_create),
        cb.onCreate, "add_space_create", Modifier.fillMaxWidth(), enabled = ui.name.isNotBlank(), busy = ui.busy,
    )
}

@Composable
private fun AccessRow(access: SpaceAccess, on: Boolean, onClick: () -> Unit) {
    RadioRow(stringResource(accessLabel(access)), on, onClick, "access_${access.name.lowercase()}")
}

@Composable
private fun JoinPart(ui: AddSpaceUi, spaces: SpacesState, joined: Set<String>, cb: AddSpaceCallbacks) {
    val c = Chord.colors
    JoinField(
        ui.query, cb.onQuery, stringResource(R.string.spaces_add_search), "add_space_search",
    )
    Spacer(Modifier.height(ChordSpace.s3))
    when (spaces) {
        SpacesState.Idle, SpacesState.Loading -> Row(
            Modifier.fillMaxWidth().height(96.dp).testTag("add_space_loading"),
            horizontalArrangement = Arrangement.Center, verticalAlignment = Alignment.CenterVertically,
        ) { CircularProgressIndicator(Modifier.size(24.dp), color = c.brand, strokeWidth = 2.dp) }
        is SpacesState.Failed -> Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(ChordSpace.s3)) {
            JoinError(spaces.message, "add_space_load_error")
            JoinTextButton(stringResource(R.string.join_retry), cb.onReload, "add_space_retry")
        }
        is SpacesState.Loaded -> {
            val q = ui.query.trim().lowercase()
            val rows = spaces.rows.filter { q.isEmpty() || it.info.name.lowercase().contains(q) }
            if (rows.isEmpty()) {
                Text(
                    stringResource(R.string.spaces_add_none), style = ChordType.body, color = c.inkMuted,
                    modifier = Modifier.fillMaxWidth().padding(vertical = ChordSpace.s4).testTag("add_space_empty"),
                )
            } else {
                LazyColumn(
                    Modifier.fillMaxWidth().heightIn(max = 360.dp).testTag("add_space_list"),
                    verticalArrangement = Arrangement.spacedBy(ChordSpace.s2),
                ) {
                    items(rows, key = { it.key }) { row -> SpaceCard(row, { cb.onJoin(row) }, joined = row.key in joined) }
                }
            }
        }
    }
}

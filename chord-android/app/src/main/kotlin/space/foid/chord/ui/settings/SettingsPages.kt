package space.foid.chord.ui.settings

import android.graphics.BitmapFactory
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay
import space.foid.chord.R
import space.foid.chord.ui.components.Avatar
import space.foid.chord.ui.components.Presence
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordRadius
import space.foid.chord.ui.theme.ChordSpace
import space.foid.chord.ui.theme.ChordType
import space.foid.chord.viewmodel.PasswordError
import space.foid.chord.viewmodel.SettingsState
import uniffi.chord_ffi.Availability

/** The connection, as the Advanced page shows it. */
enum class SettingsConnection { Connected, Connecting, Offline }

private fun Availability.presence(): Presence = when (this) {
    Availability.AVAILABLE -> Presence.Online
    Availability.AWAY, Availability.EXTENDED_AWAY -> Presence.Away
    Availability.DND -> Presence.Dnd
    Availability.INVISIBLE -> Presence.Offline
}

@Composable
private fun availabilityLabel(a: Availability): String = stringResource(
    when (a) {
        Availability.AVAILABLE -> R.string.settings_available
        Availability.AWAY, Availability.EXTENDED_AWAY -> R.string.settings_away
        Availability.DND -> R.string.settings_dnd
        Availability.INVISIBLE -> R.string.settings_invisible
    },
)

@Composable
private fun avatarImage(state: SettingsState) = remember(state.avatar) {
    state.avatar?.let { BitmapFactory.decodeByteArray(it, 0, it.size)?.asImageBitmap() }
}

// ---- Home ----

/** The home page: the profile card, then the groups of rows. */
@Composable
internal fun HomePage(state: SettingsState, go: (SettingsPage) -> Unit, onSignOut: () -> Unit) {
    val c = Chord.colors
    Column(verticalArrangement = Arrangement.spacedBy(ChordSpace.s2)) {
        ProfileCard(state, onClick = { go(SettingsPage.Account) })
        Group(stringResource(R.string.settings_group_user)) {
            NavRow(stringResource(R.string.settings_my_account), { go(SettingsPage.Account) }, Modifier.testTag("row_account"))
            RowDivider()
            NavRow(stringResource(R.string.settings_privacy), { go(SettingsPage.Privacy) }, Modifier.testTag("row_privacy"))
        }
        Group(stringResource(R.string.settings_group_app)) {
            NavRow(stringResource(R.string.settings_notifications), { go(SettingsPage.Notifications) }, Modifier.testTag("row_notifications"))
            RowDivider()
            NavRow(stringResource(R.string.settings_appearance), { go(SettingsPage.Appearance) }, Modifier.testTag("row_appearance"))
            RowDivider()
            NavRow(stringResource(R.string.settings_advanced), { go(SettingsPage.Advanced) }, Modifier.testTag("row_advanced"))
        }
        Spacer4()
        Group {
            NavRow(stringResource(R.string.settings_about), { go(SettingsPage.About) }, Modifier.testTag("row_about"))
        }
        Spacer4()
        Group {
            NavRow(
                stringResource(R.string.settings_sign_out), onSignOut, Modifier.testTag("settings_sign_out"),
                danger = true, chevron = false,
            )
        }
    }
}

@Composable
private fun Spacer4() = Box(Modifier.padding(top = ChordSpace.s2))

/** The card on top of the home page. It shows who you are and your status. It only reads. */
@Composable
internal fun ProfileCard(state: SettingsState, onClick: (() -> Unit)?) {
    val c = Chord.colors
    Row(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(ChordRadius.lg))
            .background(c.surface200)
            .then(if (onClick != null) Modifier.clickable(role = Role.Button, onClick = onClick) else Modifier)
            .padding(ChordSpace.s4)
            .testTag("settings_profile_card"),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(ChordSpace.s4),
    ) {
        Avatar(
            state.jid, name = state.displayName, image = avatarImage(state), size = 56.dp,
            presence = if (state.prefs.showPresence && state.loaded) state.availability.presence() else null,
            cut = c.surface200,
        )
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(state.displayName, style = ChordType.title, color = c.ink, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(state.jid, style = ChordType.code.copy(fontSize = ChordType.caption.fontSize), color = c.inkMuted, maxLines = 1, overflow = TextOverflow.Ellipsis)
            StatusLine(state)
        }
    }
}

@Composable
private fun StatusLine(state: SettingsState) {
    val c = Chord.colors
    val label = availabilityLabel(state.availability)
    val text = if (state.status.isNotEmpty()) "$label · ${state.status}" else label
    Text(
        if (state.loaded) text else "",
        style = ChordType.bodySmall, color = c.inkMuted, maxLines = 1, overflow = TextOverflow.Ellipsis,
        modifier = Modifier.testTag("settings_status_line"),
    )
}

// ---- My account ----

enum class AccountTab { Profile, Account }

@Composable
internal fun AccountPage(
    tab: AccountTab,
    onTab: (AccountTab) -> Unit,
    state: SettingsState,
    a: SettingsActions,
    go: (SettingsPage) -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(ChordSpace.s2)) {
        Segmented(
            listOf(
                AccountTab.Profile to stringResource(R.string.settings_tab_profile),
                AccountTab.Account to stringResource(R.string.settings_tab_account),
            ),
            tab, onTab, "account_tab",
        )
        when (tab) {
            AccountTab.Profile -> ProfileTab(state, a)
            AccountTab.Account -> AccountTabBody(state, a, go)
        }
    }
}

@Composable
private fun ProfileTab(state: SettingsState, a: SettingsActions) {
    GroupTitle(stringResource(R.string.settings_preview))
    ProfileCard(
        state.copy(nickname = state.nicknameDraft.trim()),
        onClick = null,
    )
    Group(stringResource(R.string.settings_profile)) {
        Block {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(ChordSpace.s3)) {
                SmallButton(stringResource(R.string.settings_avatar_change), a.onPickAvatar, Modifier.testTag("settings_avatar_change"))
                if (state.avatar != null) {
                    SmallButton(stringResource(R.string.settings_avatar_remove), a.onRemoveAvatar, danger = true, modifier = Modifier.testTag("settings_avatar_remove"))
                }
            }
            Label(stringResource(R.string.settings_nickname))
            TextInput(
                state.nicknameDraft, a.onNicknameChange, stringResource(R.string.settings_nickname_hint),
                onDone = a.onSaveNickname, modifier = Modifier.testTag("settings_nickname"),
            )
            Hint(stringResource(R.string.settings_profile_note))
            if (state.nicknameChanged) {
                Row(horizontalArrangement = Arrangement.spacedBy(ChordSpace.s2)) {
                    SmallButton(stringResource(R.string.settings_reset), a.onResetNickname, Modifier.testTag("settings_nickname_reset"))
                    SmallButton(stringResource(R.string.settings_save), a.onSaveNickname, Modifier.testTag("settings_nickname_save"), primary = true)
                }
            }
        }
    }
    Group(stringResource(R.string.settings_at_sign_in)) {
        Block {
            Label(stringResource(R.string.settings_availability))
        }
        Choices(
            options = listOf(
                SignInShow.Last to stringResource(R.string.settings_sign_in_last),
                SignInShow.Chat to stringResource(R.string.settings_available),
                SignInShow.Away to stringResource(R.string.settings_away),
                SignInShow.Dnd to stringResource(R.string.settings_dnd),
            ),
            selected = state.prefs.signInShow, onSelect = a.onSignInShow, tag = "signin",
        )
        Block {
            Label(stringResource(R.string.settings_status))
            TextInput(
                state.signInStatusDraft, a.onSignInStatusChange, stringResource(R.string.settings_status_hint),
                onDone = a.onSaveSignInStatus, modifier = Modifier.testTag("settings_status"),
            )
            Hint(stringResource(R.string.settings_at_sign_in_hint))
            if (state.signInStatusChanged) {
                SmallButton(stringResource(R.string.settings_save), a.onSaveSignInStatus, Modifier.testTag("settings_status_save"), primary = true)
            }
        }
    }
}

@Composable
private fun AccountTabBody(state: SettingsState, a: SettingsActions, go: (SettingsPage) -> Unit) {
    val c = Chord.colors
    var copied by remember { mutableStateOf(false) }
    LaunchedEffect(copied) { if (copied) { delay(2000); copied = false } }
    Group(stringResource(R.string.settings_sign_in)) {
        Block {
            Label(stringResource(R.string.settings_jid))
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(ChordSpace.s3)) {
                Text(
                    state.jid, style = ChordType.code, color = c.inkMuted, maxLines = 1, overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f).testTag("settings_jid"),
                )
                SmallButton(
                    stringResource(if (copied) R.string.settings_copied else R.string.settings_copy),
                    { a.onCopyJid(); copied = true },
                    Modifier.testTag("settings_copy_jid"),
                )
            }
        }
        RowDivider()
        NavRow(
            stringResource(R.string.settings_password_change), { go(SettingsPage.Password) },
            Modifier.testTag("settings_password_open"),
        )
    }
}

// ---- Change password ----

@Composable
internal fun PasswordPage(state: SettingsState, a: SettingsActions) {
    var new by rememberSaveable { mutableStateOf("") }
    var repeat by rememberSaveable { mutableStateOf("") }
    val p = state.password
    Group {
        Block {
            Label(stringResource(R.string.settings_password_new))
            TextInput(new, { new = it }, "", onDone = {}, password = true, imeAction = androidx.compose.ui.text.input.ImeAction.Next, modifier = Modifier.testTag("password_new"))
            Label(stringResource(R.string.settings_password_repeat))
            TextInput(repeat, { repeat = it }, "", onDone = { a.onChangePassword(new, repeat) }, password = true, modifier = Modifier.testTag("password_repeat"))
            Hint(stringResource(R.string.settings_password_note))
            val problem = when (p.error) {
                PasswordError.Empty -> stringResource(R.string.settings_password_empty)
                PasswordError.Mismatch -> stringResource(R.string.settings_password_mismatch)
                PasswordError.SaveFailed -> stringResource(R.string.settings_password_failed)
                PasswordError.Server -> p.message.orEmpty()
                null -> null
            }
            if (problem != null) Text(problem, style = ChordType.bodySmall, color = Chord.colors.danger, modifier = Modifier.testTag("password_error"))
            if (p.done) Text(stringResource(R.string.settings_password_done), style = ChordType.bodySmall, color = Chord.colors.online, modifier = Modifier.testTag("password_done"))
            SmallButton(
                stringResource(R.string.settings_password_change), { a.onChangePassword(new, repeat) },
                Modifier.testTag("password_submit"), primary = true, enabled = !p.busy,
            )
        }
    }
}

// ---- Privacy ----

@Composable
internal fun PrivacyPage(state: SettingsState, a: SettingsActions) {
    val c = Chord.colors
    var confirmAll by rememberSaveable { mutableStateOf(false) }
    Group(stringResource(R.string.settings_activity)) {
        ToggleRow(
            stringResource(R.string.settings_share_idle), state.prefs.shareIdle, a.onShareIdle, "settings_share_idle",
            hint = stringResource(R.string.settings_share_idle_hint),
        )
        if (state.prefs.shareIdle) {
            RowDivider()
            Block { Label(stringResource(R.string.settings_idle_after)) }
            Choices(
                options = IDLE_MINUTES.map { it to stringResource(R.string.settings_minutes, it) },
                selected = state.prefs.idleMinutes, onSelect = a.onIdleMinutes, tag = "idle",
            )
        }
    }
    Group(stringResource(R.string.settings_software)) {
        ToggleRow(
            stringResource(R.string.settings_share_info), state.prefs.shareInfo, a.onShareInfo, "settings_share_info",
            hint = stringResource(R.string.settings_share_info_hint),
        )
    }
    Group(stringResource(R.string.settings_blocked)) {
        if (state.blocked.isEmpty()) {
            Block { Hint(stringResource(R.string.settings_blocked_none)) }
        } else {
            state.blocked.forEachIndexed { i, jid ->
                if (i > 0) RowDivider()
                Row(
                    Modifier.fillMaxWidth().heightIn(min = 52.dp).padding(horizontal = ChordSpace.s3),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(ChordSpace.s3),
                ) {
                    Text(jid, style = ChordType.body, color = c.ink, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
                    SmallButton(stringResource(R.string.settings_unblock), { a.onUnblock(jid) }, Modifier.testTag("unblock_$jid"))
                }
            }
            RowDivider()
            NavRow(
                stringResource(R.string.settings_unblock_all), { confirmAll = true },
                Modifier.testTag("settings_unblock_all"), danger = true, chevron = false,
            )
        }
    }
    if (confirmAll) {
        ConfirmDialog(
            title = stringResource(R.string.settings_unblock_all_title),
            body = pluralStringResource(R.plurals.settings_unblock_all_body, state.blocked.size, state.blocked.size),
            confirm = stringResource(R.string.settings_unblock_all),
            danger = false,
            tag = "confirm_unblock_all",
            onDismiss = { confirmAll = false },
            onConfirm = { confirmAll = false; a.onUnblockAll() },
        )
    }
}

// ---- Notifications ----

@Composable
internal fun NotificationsPage(state: SettingsState, notificationsOn: Boolean, is24: Boolean, a: SettingsActions) {
    val c = Chord.colors
    var picking by rememberSaveable { mutableStateOf<String?>(null) }
    Group {
        Block {
            Text(
                stringResource(if (notificationsOn) R.string.settings_notifications_on else R.string.settings_notifications_off),
                style = ChordType.body,
                color = if (notificationsOn) c.ink else c.danger,
                modifier = Modifier.testTag("settings_notifications_state"),
            )
        }
        RowDivider()
        NavRow(stringResource(R.string.settings_notifications_open), a.onOpenNotificationSettings, Modifier.testTag("settings_notifications_open"))
    }
    Group {
        ToggleRow(
            stringResource(R.string.settings_notice_preview), state.prefs.noticePreview, a.onNoticePreview, "settings_notice_preview",
            hint = stringResource(R.string.settings_notice_preview_hint),
        )
    }
    Group(stringResource(R.string.settings_quiet)) {
        ToggleRow(
            stringResource(R.string.settings_quiet_on), state.prefs.quietHours, a.onQuietHours, "settings_quiet",
            hint = stringResource(R.string.settings_quiet_hint),
        )
        if (state.prefs.quietHours) {
            RowDivider()
            NavRow(
                stringResource(R.string.settings_quiet_from), { picking = "from" }, Modifier.testTag("settings_quiet_from"),
                value = formatMinute(state.prefs.quietFrom, is24),
            )
            RowDivider()
            NavRow(
                stringResource(R.string.settings_quiet_until), { picking = "to" }, Modifier.testTag("settings_quiet_to"),
                value = formatMinute(state.prefs.quietTo, is24),
            )
        }
    }
    picking?.let { which ->
        val from = which == "from"
        TimeDialog(
            title = stringResource(if (from) R.string.settings_quiet_from else R.string.settings_quiet_until),
            current = if (from) state.prefs.quietFrom else state.prefs.quietTo,
            is24 = is24,
            onDismiss = { picking = null },
            onPick = { m -> picking = null; if (from) a.onQuietFrom(m) else a.onQuietTo(m) },
        )
    }
}

/** A list of times in half hours. */
@Composable
private fun TimeDialog(title: String, current: Int, is24: Boolean, onDismiss: () -> Unit, onPick: (Int) -> Unit) {
    val c = Chord.colors
    val times = remember { (0 until 48).map { it * 30 } }
    AlertDialog(
        onDismissRequest = onDismiss,
        containerColor = c.surface300,
        title = { Text(title, style = ChordType.title, color = c.ink) },
        text = {
            LazyColumn(Modifier.heightIn(max = 360.dp).testTag("time_list")) {
                items(times) { m ->
                    val on = m == current
                    Text(
                        formatMinute(m, is24),
                        style = ChordType.body, color = if (on) c.brandInk else c.ink,
                        modifier = Modifier
                            .fillMaxWidth()
                            .heightIn(min = 44.dp)
                            .background(if (on) c.brandSoft else androidx.compose.ui.graphics.Color.Transparent)
                            .clickable { onPick(m) }
                            .padding(horizontal = ChordSpace.s3, vertical = ChordSpace.s3)
                            .testTag("time_$m"),
                    )
                }
            }
        },
        confirmButton = {},
        dismissButton = {
            TextButton(onClick = onDismiss) { Text(stringResource(R.string.settings_cancel), style = ChordType.label, color = c.ink) }
        },
    )
}

// ---- Appearance ----

@Composable
internal fun AppearancePage(state: SettingsState, a: SettingsActions) {
    Group(stringResource(R.string.settings_mode)) {
        Choices(
            options = listOf(
                ThemeMode.System to stringResource(R.string.settings_theme_system),
                ThemeMode.Dark to stringResource(R.string.settings_theme_dark),
                ThemeMode.Light to stringResource(R.string.settings_theme_light),
            ),
            selected = state.prefs.theme, onSelect = a.onTheme, tag = "theme",
        )
    }
    Group(stringResource(R.string.settings_interface)) {
        ToggleRow(
            stringResource(R.string.settings_show_presence), state.prefs.showPresence, a.onShowPresence, "settings_show_presence",
            hint = stringResource(R.string.settings_show_presence_hint),
        )
    }
}

// ---- Advanced ----

@Composable
internal fun AdvancedPage(connection: SettingsConnection, a: SettingsActions) {
    var confirmReset by rememberSaveable { mutableStateOf(false) }
    Group(stringResource(R.string.settings_connection)) {
        NavRow(
            stringResource(R.string.settings_connection_status), {}, Modifier.testTag("settings_connection"),
            value = stringResource(
                when (connection) {
                    SettingsConnection.Connected -> R.string.settings_conn_connected
                    SettingsConnection.Connecting -> R.string.settings_conn_connecting
                    SettingsConnection.Offline -> R.string.settings_conn_offline
                },
            ),
            chevron = false,
        )
    }
    Group(stringResource(R.string.settings_troubleshooting)) {
        NavRow(
            stringResource(R.string.settings_reset_settings), { confirmReset = true }, Modifier.testTag("settings_reset"),
            danger = true, chevron = false,
        )
        Block { Hint(stringResource(R.string.settings_reset_settings_hint)) }
    }
    if (confirmReset) {
        ConfirmDialog(
            title = stringResource(R.string.settings_reset_title),
            body = stringResource(R.string.settings_reset_body),
            confirm = stringResource(R.string.settings_reset_settings),
            danger = true,
            tag = "confirm_reset",
            onDismiss = { confirmReset = false },
            onConfirm = { confirmReset = false; a.onResetSettings() },
        )
    }
}

// ---- About ----

@Composable
internal fun AboutPage(version: String, licenses: List<String>, licenseText: (String) -> String, a: SettingsActions) {
    val c = Chord.colors
    Group {
        Block {
            Text("Chord", style = ChordType.title, color = c.ink)
            Text(stringResource(R.string.settings_version, version), style = ChordType.bodySmall, color = c.inkMuted, modifier = Modifier.testTag("settings_version"))
            Text(
                stringResource(R.string.settings_website),
                style = ChordType.body.copy(textDecoration = TextDecoration.Underline),
                color = c.brandInk,
                modifier = Modifier
                    .heightIn(min = 40.dp)
                    .clickable(role = Role.Button, onClick = a.onOpenWebsite)
                    .padding(vertical = ChordSpace.s2)
                    .testTag("settings_website"),
            )
        }
    }
    Group(stringResource(R.string.settings_licenses)) {
        licenses.forEachIndexed { i, name ->
            if (i > 0) RowDivider()
            var open by rememberSaveable(name) { mutableStateOf(false) }
            Column(Modifier.fillMaxWidth().testTag("license_$name")) {
                Text(
                    name.substringBeforeLast('.'),
                    style = ChordType.body, color = c.brandInk,
                    modifier = Modifier
                        .fillMaxWidth()
                        .heightIn(min = 48.dp)
                        .clickable(role = Role.Button) { open = !open }
                        .padding(horizontal = ChordSpace.s3, vertical = ChordSpace.s3),
                )
                if (open) {
                    Text(licenseText(name), style = ChordType.caption, color = c.inkMuted, modifier = Modifier.padding(horizontal = ChordSpace.s3, vertical = ChordSpace.s2))
                }
            }
        }
    }
}

// ---- Dialogs ----

@Composable
internal fun ConfirmDialog(
    title: String,
    body: String,
    confirm: String,
    danger: Boolean,
    tag: String,
    onDismiss: () -> Unit,
    onConfirm: () -> Unit,
) {
    val c = Chord.colors
    AlertDialog(
        onDismissRequest = onDismiss,
        containerColor = c.surface300,
        title = { Text(title, style = ChordType.title, color = c.ink) },
        text = { Text(body, style = ChordType.body, color = c.inkMuted) },
        confirmButton = {
            TextButton(onClick = onConfirm, modifier = Modifier.testTag(tag)) {
                Text(confirm, style = ChordType.label, color = if (danger) c.danger else c.brandInk)
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text(stringResource(R.string.settings_cancel), style = ChordType.label, color = c.ink) }
        },
    )
}

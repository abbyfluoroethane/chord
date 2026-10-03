package space.foid.chord.ui.settings

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.graphics.BitmapFactory
import android.provider.Settings
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.em
import androidx.core.app.NotificationManagerCompat
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import space.foid.chord.ChordApp
import space.foid.chord.R
import space.foid.chord.ui.components.Avatar
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordRadius
import space.foid.chord.ui.theme.ChordSpace
import space.foid.chord.ui.theme.ChordType
import space.foid.chord.viewmodel.ClientSettingsApi
import space.foid.chord.viewmodel.SettingsState
import space.foid.chord.viewmodel.SettingsViewModel
import uniffi.chord_ffi.Availability

private const val WEBSITE = "https://github.com/abbyfluoroethane/chord"

/** What the settings screen can do. The defaults do nothing, for previews and screenshots. */
class SettingsActions(
    val onBack: () -> Unit = {},
    val onNicknameChange: (String) -> Unit = {},
    val onSaveNickname: () -> Unit = {},
    val onPickAvatar: () -> Unit = {},
    val onRemoveAvatar: () -> Unit = {},
    val onCopyJid: () -> Unit = {},
    val onAvailability: (Availability) -> Unit = {},
    val onStatusChange: (String) -> Unit = {},
    val onSaveStatus: () -> Unit = {},
    val onTheme: (ThemeMode) -> Unit = {},
    val onOpenNotificationSettings: () -> Unit = {},
    val onShareInfo: (Boolean) -> Unit = {},
    val onUnblock: (String) -> Unit = {},
    val onOpenWebsite: () -> Unit = {},
    val onSignOut: () -> Unit = {},
)

/**
 * The settings screen, fed by [SettingsViewModel]. [onBack] leaves it. [onSignedOut] runs after a
 * sign-out, and the nav host then shows the sign-in screen.
 */
@Composable
fun SettingsScreen(onBack: () -> Unit, onSignedOut: () -> Unit, modifier: Modifier = Modifier) {
    val context = LocalContext.current
    val app = context.applicationContext as ChordApp
    val vm: SettingsViewModel = viewModel(
        factory = viewModelFactory {
            initializer {
                SettingsViewModel(
                    api = ClientSettingsApi(requireNotNull(app.session.client.value) { "not signed in" }),
                    store = PrefsSettingsStore.get(app),
                    signOutAction = { app.session.signOut() },
                )
            }
        },
    )
    val state by vm.state.collectAsState()
    val signedOut by vm.signedOut.collectAsState()
    LaunchedEffect(signedOut) { if (signedOut) onSignedOut() }

    val scope = rememberCoroutineScope()
    val picker = rememberLauncherForActivityResult(ActivityResultContracts.PickVisualMedia()) { uri ->
        if (uri != null) scope.launch {
            val prepared = withContext(Dispatchers.IO) { prepareAvatar(context, uri) }
            if (prepared == null) vm.avatarFailed()
            else vm.setAvatar(prepared.mime, prepared.data, prepared.width, prepared.height)
        }
    }

    var notificationsOn by remember { mutableStateOf(true) }
    // The user may change it in the system settings and come back.
    LifecycleResumeEffect(Unit) {
        notificationsOn = NotificationManagerCompat.from(context).areNotificationsEnabled()
        onPauseOrDispose {}
    }

    val uriHandler = LocalUriHandler.current
    val licenses = remember { licenseNames(context) }
    SettingsContent(
        state = state,
        notificationsOn = notificationsOn,
        version = remember { versionName(context) },
        licenses = licenses,
        licenseText = { name -> licenseText(context, name) },
        actions = SettingsActions(
            onBack = onBack,
            onNicknameChange = vm::onNicknameChange,
            onSaveNickname = vm::saveNickname,
            onPickAvatar = { picker.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly)) },
            onRemoveAvatar = vm::removeAvatar,
            onCopyJid = { copyText(context, state.jid) },
            onAvailability = vm::setAvailability,
            onStatusChange = vm::onStatusChange,
            onSaveStatus = vm::saveStatus,
            onTheme = vm::setTheme,
            onOpenNotificationSettings = {
                context.startActivity(
                    Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS)
                        .putExtra(Settings.EXTRA_APP_PACKAGE, context.packageName)
                        .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
                )
            },
            onShareInfo = vm::setShareInfo,
            onUnblock = vm::unblock,
            onOpenWebsite = { uriHandler.openUri(WEBSITE) },
            onSignOut = vm::signOut,
        ),
        modifier = modifier,
    )
}

private fun versionName(context: Context): String = try {
    context.packageManager.getPackageInfo(context.packageName, 0).versionName ?: ""
} catch (e: Exception) {
    ""
}

private fun licenseNames(context: Context): List<String> =
    (context.assets.list("licenses") ?: emptyArray()).sorted()

private fun licenseText(context: Context, name: String): String = try {
    context.assets.open("licenses/$name").bufferedReader().use { it.readText() }
} catch (e: Exception) {
    ""
}

private fun copyText(context: Context, text: String) {
    val clipboard = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
    clipboard.setPrimaryClip(ClipData.newPlainText("address", text))
}

/**
 * The settings screen without a ViewModel.
 *
 * @param notificationsOn the system lets the app show notifications.
 * @param licenses the file names in `assets/licenses`.
 * @param licenseText the text of one licence file. Shown when the user opens it.
 */
@Composable
fun SettingsContent(
    state: SettingsState,
    notificationsOn: Boolean,
    version: String,
    licenses: List<String>,
    actions: SettingsActions,
    modifier: Modifier = Modifier,
    licenseText: (String) -> String = { "" },
) {
    val c = Chord.colors
    var confirmSignOut by rememberSaveable { mutableStateOf(false) }
    Column(modifier.fillMaxSize().background(c.surface100).statusBarsPadding().testTag("settings_screen")) {
        TopBar(actions.onBack)
        Column(
            Modifier
                .weight(1f)
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .imePadding()
                .padding(horizontal = ChordSpace.s4)
                .padding(bottom = ChordSpace.s6),
            verticalArrangement = Arrangement.spacedBy(ChordSpace.s2),
        ) {
            state.error?.let { ErrorNote(it) }
            ProfileSection(state, actions)
            PresenceSection(state, actions)
            AppearanceSection(state, actions)
            NotificationsSection(notificationsOn, actions)
            PrivacySection(state, actions)
            AboutSection(version, licenses, licenseText, actions)
            DangerButton(stringResource(R.string.settings_sign_out), { confirmSignOut = true }, Modifier.testTag("settings_sign_out"))
            Box(Modifier.navigationBarsPadding())
        }
    }
    if (confirmSignOut) {
        AlertDialog(
            onDismissRequest = { confirmSignOut = false },
            containerColor = c.surface300,
            title = { Text(stringResource(R.string.settings_sign_out_title), style = ChordType.title, color = c.ink) },
            text = { Text(stringResource(R.string.settings_sign_out_body), style = ChordType.body, color = c.inkMuted) },
            confirmButton = {
                TextButton(
                    onClick = { confirmSignOut = false; actions.onSignOut() },
                    modifier = Modifier.testTag("confirm_sign_out"),
                ) { Text(stringResource(R.string.settings_sign_out), style = ChordType.label, color = c.danger) }
            },
            dismissButton = {
                TextButton(onClick = { confirmSignOut = false }) {
                    Text(stringResource(R.string.settings_cancel), style = ChordType.label, color = c.ink)
                }
            },
        )
    }
}

@Composable
private fun TopBar(onBack: () -> Unit) {
    val c = Chord.colors
    Row(Modifier.fillMaxWidth().heightIn(min = 48.dp), verticalAlignment = Alignment.CenterVertically) {
        val back = stringResource(R.string.settings_back)
        Box(
            Modifier
                .size(48.dp)
                .clickable(role = Role.Button, onClickLabel = back, onClick = onBack)
                .semantics { contentDescription = back }
                .testTag("settings_back"),
            contentAlignment = Alignment.Center,
        ) {
            Canvas(Modifier.size(20.dp)) {
                val w = 2.dp.toPx()
                val mid = size.height / 2
                drawLine(c.ink, Offset(size.width, mid), Offset(2.dp.toPx(), mid), w, StrokeCap.Round)
                drawLine(c.ink, Offset(2.dp.toPx(), mid), Offset(9.dp.toPx(), mid - 7.dp.toPx()), w, StrokeCap.Round)
                drawLine(c.ink, Offset(2.dp.toPx(), mid), Offset(9.dp.toPx(), mid + 7.dp.toPx()), w, StrokeCap.Round)
            }
        }
        Text(stringResource(R.string.settings_title), style = ChordType.title, color = c.ink)
    }
}

@Composable
private fun ErrorNote(text: String) {
    val c = Chord.colors
    Text(
        text,
        style = ChordType.bodySmall,
        color = c.danger,
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(ChordRadius.md))
            .background(c.danger.copy(alpha = 0.14f))
            .padding(ChordSpace.s3)
            .testTag("settings_error"),
    )
}

/** A titled group of rows on a raised surface. */
@Composable
private fun Section(title: String, content: @Composable () -> Unit) {
    val c = Chord.colors
    Text(
        title.uppercase(),
        style = ChordType.caption.copy(fontWeight = androidx.compose.ui.text.font.FontWeight.Bold, letterSpacing = 0.06.em),
        color = c.inkMuted,
        modifier = Modifier.padding(start = ChordSpace.s1, top = ChordSpace.s4, bottom = ChordSpace.s1),
    )
    Column(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(ChordRadius.md))
            .background(c.surface200)
            .padding(ChordSpace.s3),
        verticalArrangement = Arrangement.spacedBy(ChordSpace.s3),
    ) { content() }
}

@Composable
private fun Label(text: String) =
    Text(text, style = ChordType.label, color = Chord.colors.ink)

@Composable
private fun Hint(text: String) =
    Text(text, style = ChordType.caption, color = Chord.colors.inkMuted)

@Composable
private fun TextInput(
    value: String,
    onValueChange: (String) -> Unit,
    hint: String,
    onDone: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val c = Chord.colors
    BasicTextField(
        value = value,
        onValueChange = onValueChange,
        singleLine = true,
        textStyle = ChordType.body.copy(color = c.ink),
        cursorBrush = SolidColor(c.brand),
        keyboardOptions = KeyboardOptions(imeAction = ImeAction.Done),
        keyboardActions = KeyboardActions(onDone = { onDone() }),
        modifier = modifier.fillMaxWidth(),
        decorationBox = { inner ->
            Box(
                Modifier
                    .fillMaxWidth()
                    .heightIn(min = 44.dp)
                    .clip(RoundedCornerShape(ChordRadius.md))
                    .background(c.surface100)
                    .border(1.dp, c.lineStrong, RoundedCornerShape(ChordRadius.md))
                    .padding(horizontal = ChordSpace.s3),
                contentAlignment = Alignment.CenterStart,
            ) {
                if (value.isEmpty()) Text(hint, style = ChordType.body, color = c.inkMuted)
                inner()
            }
        },
    )
}

@Composable
private fun SmallButton(text: String, onClick: () -> Unit, modifier: Modifier = Modifier, danger: Boolean = false) {
    val c = Chord.colors
    Box(
        modifier
            .heightIn(min = 40.dp)
            .clip(RoundedCornerShape(ChordRadius.md))
            .background(if (danger) c.danger.copy(alpha = 0.16f) else c.surface300)
            .clickable(role = Role.Button, onClick = onClick)
            .padding(horizontal = ChordSpace.s4),
        contentAlignment = Alignment.Center,
    ) {
        Text(text, style = ChordType.label, color = if (danger) c.danger else c.ink)
    }
}

@Composable
private fun DangerButton(text: String, onClick: () -> Unit, modifier: Modifier = Modifier) {
    val c = Chord.colors
    Box(
        modifier
            .padding(top = ChordSpace.s6)
            .fillMaxWidth()
            .heightIn(min = 48.dp)
            .clip(RoundedCornerShape(ChordRadius.md))
            .background(c.danger.copy(alpha = 0.16f))
            .clickable(role = Role.Button, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) { Text(text, style = ChordType.label, color = c.danger) }
}

/** One choice of a row of choices. Colour and a filled ring both mark the chosen one. */
@Composable
private fun <T> Choices(
    options: List<Pair<T, String>>,
    selected: T,
    onSelect: (T) -> Unit,
    tag: String,
) {
    val c = Chord.colors
    Column(verticalArrangement = Arrangement.spacedBy(ChordSpace.s1)) {
        options.forEach { (value, label) ->
            val on = value == selected
            Row(
                Modifier
                    .fillMaxWidth()
                    .heightIn(min = 44.dp)
                    .clip(RoundedCornerShape(ChordRadius.md))
                    .background(if (on) c.brandSoft else c.surface200)
                    .clickable(role = Role.RadioButton) { onSelect(value) }
                    .semantics { this.selected = on }
                    .padding(horizontal = ChordSpace.s3)
                    .testTag("${tag}_$label"),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(ChordSpace.s3),
            ) {
                Box(
                    Modifier
                        .size(18.dp)
                        .border(2.dp, if (on) c.brand else c.lineStrong, androidx.compose.foundation.shape.CircleShape)
                        .padding(4.dp)
                        .clip(androidx.compose.foundation.shape.CircleShape)
                        .background(if (on) c.brand else Color.Transparent),
                )
                Text(label, style = ChordType.body, color = if (on) c.brandInk else c.ink)
            }
        }
    }
}

@Composable
private fun ProfileSection(state: SettingsState, a: SettingsActions) {
    val c = Chord.colors
    Section(stringResource(R.string.settings_profile)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(ChordSpace.s4)) {
            val image = remember(state.avatar) {
                state.avatar?.let { BitmapFactory.decodeByteArray(it, 0, it.size)?.asImageBitmap() }
            }
            Avatar(
                state.jid, name = state.nickname.ifEmpty { state.jid.substringBefore('@') },
                image = image, size = 64.dp, cut = c.surface200,
            )
            Column(verticalArrangement = Arrangement.spacedBy(ChordSpace.s2)) {
                SmallButton(stringResource(R.string.settings_avatar_change), a.onPickAvatar, Modifier.testTag("settings_avatar_change"))
                if (state.avatar != null) {
                    SmallButton(stringResource(R.string.settings_avatar_remove), a.onRemoveAvatar, danger = true)
                }
            }
        }
        Column(verticalArrangement = Arrangement.spacedBy(ChordSpace.s1)) {
            Label(stringResource(R.string.settings_nickname))
            TextInput(
                state.nicknameDraft, a.onNicknameChange, stringResource(R.string.settings_nickname_hint),
                onDone = a.onSaveNickname, modifier = Modifier.testTag("settings_nickname"),
            )
            if (state.nicknameChanged) {
                SmallButton(stringResource(R.string.settings_save), a.onSaveNickname, Modifier.testTag("settings_nickname_save"))
            }
        }
        var copied by remember { mutableStateOf(false) }
        LaunchedEffect(copied) { if (copied) { delay(2000); copied = false } }
        Column(verticalArrangement = Arrangement.spacedBy(ChordSpace.s1)) {
            Label(stringResource(R.string.settings_jid))
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(ChordSpace.s3)) {
                Text(
                    state.jid, style = ChordType.code, color = c.inkMuted, maxLines = 1, overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
                SmallButton(
                    stringResource(if (copied) R.string.settings_copied else R.string.settings_copy),
                    { a.onCopyJid(); copied = true },
                    Modifier.testTag("settings_copy_jid"),
                )
            }
        }
    }
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
private fun PresenceSection(state: SettingsState, a: SettingsActions) {
    Section(stringResource(R.string.settings_presence)) {
        val shown = if (state.availability == Availability.EXTENDED_AWAY) Availability.AWAY else state.availability
        Choices(
            options = state.availabilities.map { it to availabilityLabel(it) },
            selected = shown, onSelect = a.onAvailability, tag = "presence",
        )
        Column(verticalArrangement = Arrangement.spacedBy(ChordSpace.s1)) {
            Label(stringResource(R.string.settings_status))
            TextInput(
                state.statusDraft, a.onStatusChange, stringResource(R.string.settings_status_hint),
                onDone = a.onSaveStatus, modifier = Modifier.testTag("settings_status"),
            )
            if (state.statusChanged) {
                SmallButton(stringResource(R.string.settings_save), a.onSaveStatus, Modifier.testTag("settings_status_save"))
            }
        }
    }
}

@Composable
private fun AppearanceSection(state: SettingsState, a: SettingsActions) {
    Section(stringResource(R.string.settings_appearance)) {
        Choices(
            options = listOf(
                ThemeMode.System to stringResource(R.string.settings_theme_system),
                ThemeMode.Dark to stringResource(R.string.settings_theme_dark),
                ThemeMode.Light to stringResource(R.string.settings_theme_light),
            ),
            selected = state.theme, onSelect = a.onTheme, tag = "theme",
        )
    }
}

@Composable
private fun NotificationsSection(on: Boolean, a: SettingsActions) {
    Section(stringResource(R.string.settings_notifications)) {
        Text(
            stringResource(if (on) R.string.settings_notifications_on else R.string.settings_notifications_off),
            style = ChordType.body,
            color = if (on) Chord.colors.ink else Chord.colors.danger,
            modifier = Modifier.testTag("settings_notifications_state"),
        )
        SmallButton(stringResource(R.string.settings_notifications_open), a.onOpenNotificationSettings, Modifier.testTag("settings_notifications_open"))
    }
}

@Composable
private fun PrivacySection(state: SettingsState, a: SettingsActions) {
    val c = Chord.colors
    Section(stringResource(R.string.settings_privacy)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(ChordSpace.s3)) {
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(ChordSpace.s1)) {
                Label(stringResource(R.string.settings_share_info))
                Hint(stringResource(R.string.settings_share_info_hint))
            }
            Switch(
                checked = state.shareInfo,
                onCheckedChange = a.onShareInfo,
                colors = SwitchDefaults.colors(
                    checkedTrackColor = c.brand, checkedThumbColor = c.onBrand,
                    uncheckedTrackColor = c.surface300, uncheckedThumbColor = c.inkMuted, uncheckedBorderColor = c.lineStrong,
                ),
                modifier = Modifier.testTag("settings_share_info"),
            )
        }
        Label(stringResource(R.string.settings_blocked))
        if (state.blocked.isEmpty()) {
            Hint(stringResource(R.string.settings_blocked_none))
        } else {
            state.blocked.forEach { jid ->
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(ChordSpace.s3)) {
                    Text(jid, style = ChordType.body, color = c.ink, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
                    SmallButton(stringResource(R.string.settings_unblock), { a.onUnblock(jid) }, Modifier.testTag("unblock_$jid"))
                }
            }
        }
    }
}

@Composable
private fun AboutSection(version: String, licenses: List<String>, licenseText: (String) -> String, a: SettingsActions) {
    val c = Chord.colors
    Section(stringResource(R.string.settings_about)) {
        Text("Chord", style = ChordType.name, color = c.ink)
        Text(stringResource(R.string.settings_version, version), style = ChordType.bodySmall, color = c.inkMuted)
        Text(
            stringResource(R.string.settings_website),
            style = ChordType.body.copy(textDecoration = androidx.compose.ui.text.style.TextDecoration.Underline),
            color = c.brandInk,
            modifier = Modifier
                .heightIn(min = 40.dp)
                .clickable(role = Role.Button, onClick = a.onOpenWebsite)
                .padding(vertical = ChordSpace.s2)
                .testTag("settings_website"),
        )
        Label(stringResource(R.string.settings_licenses))
        licenses.forEach { name ->
            var open by rememberSaveable(name) { mutableStateOf(false) }
            Column(Modifier.fillMaxWidth().testTag("license_$name")) {
                Text(
                    name.substringBeforeLast('.'),
                    style = ChordType.body, color = c.brandInk,
                    modifier = Modifier
                        .fillMaxWidth()
                        .heightIn(min = 40.dp)
                        .clickable(role = Role.Button) { open = !open }
                        .padding(vertical = ChordSpace.s2),
                )
                if (open) Text(licenseText(name), style = ChordType.caption, color = c.inkMuted)
            }
        }
    }
}

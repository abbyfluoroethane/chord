package space.foid.chord.ui.settings

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.provider.Settings
import android.text.format.DateFormat
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.core.app.NotificationManagerCompat
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import space.foid.chord.ChordApp
import space.foid.chord.R
import space.foid.chord.secure.KeystoreCredentialStore
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordSpace
import space.foid.chord.viewmodel.ClientSettingsApi
import space.foid.chord.viewmodel.SettingsState
import space.foid.chord.viewmodel.SettingsViewModel
import uniffi.chord_ffi.ConnectionState

private const val WEBSITE = "https://github.com/abbyfluoroethane/chord"

/** The pages of the settings. [Home] is the list. System back goes from a page to its [parent]. */
enum class SettingsPage(val parent: SettingsPage?) {
    Home(null),
    Account(Home),
    Password(Account),
    Privacy(Home),
    Notifications(Home),
    Appearance(Home),
    Advanced(Home),
    About(Home),
}

@Composable
private fun pageTitle(page: SettingsPage): String = stringResource(
    when (page) {
        SettingsPage.Home -> R.string.settings_title
        SettingsPage.Account -> R.string.settings_my_account
        SettingsPage.Password -> R.string.settings_password_change
        SettingsPage.Privacy -> R.string.settings_privacy
        SettingsPage.Notifications -> R.string.settings_notifications
        SettingsPage.Appearance -> R.string.settings_appearance
        SettingsPage.Advanced -> R.string.settings_advanced
        SettingsPage.About -> R.string.settings_about
    },
)

/** What the settings screen can do. The defaults do nothing, for previews and screenshots. */
class SettingsActions(
    val onBack: () -> Unit = {},
    val onNicknameChange: (String) -> Unit = {},
    val onSaveNickname: () -> Unit = {},
    val onResetNickname: () -> Unit = {},
    val onPickAvatar: () -> Unit = {},
    val onRemoveAvatar: () -> Unit = {},
    val onCopyJid: () -> Unit = {},
    val onSignInShow: (SignInShow) -> Unit = {},
    val onSignInStatusChange: (String) -> Unit = {},
    val onSaveSignInStatus: () -> Unit = {},
    val onChangePassword: (String, String) -> Unit = { _, _ -> },
    val onTheme: (ThemeMode) -> Unit = {},
    val onShowPresence: (Boolean) -> Unit = {},
    val onOpenNotificationSettings: () -> Unit = {},
    val onNoticePreview: (Boolean) -> Unit = {},
    val onQuietHours: (Boolean) -> Unit = {},
    val onQuietFrom: (Int) -> Unit = {},
    val onQuietTo: (Int) -> Unit = {},
    val onShareInfo: (Boolean) -> Unit = {},
    val onShareIdle: (Boolean) -> Unit = {},
    val onIdleMinutes: (Int) -> Unit = {},
    val onUnblock: (String) -> Unit = {},
    val onUnblockAll: () -> Unit = {},
    val onResetSettings: () -> Unit = {},
    val onOpenWebsite: () -> Unit = {},
    val onSignOut: () -> Unit = {},
    /** Leaving the change-password page: forget its result. */
    val onLeavePassword: () -> Unit = {},
)

/**
 * The settings, fed by [SettingsViewModel]. [onBack] leaves them from the home page. [onSignedOut]
 * runs after a sign-out, and the nav host then shows the sign-in screen.
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
                    savePassword = { password ->
                        withContext(Dispatchers.IO) {
                            val store = KeystoreCredentialStore(app)
                            val saved = requireNotNull(store.load()) { "no saved credentials" }
                            store.save(saved.jid, password, saved.server)
                        }
                    },
                )
            }
        },
    )
    val state by vm.state.collectAsState()
    val signedOut by vm.signedOut.collectAsState()
    LaunchedEffect(signedOut) { if (signedOut) onSignedOut() }
    val connection by app.session.connection.collectAsState()

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

    var page by rememberSaveable { mutableStateOf(SettingsPage.Home) }
    val uriHandler = LocalUriHandler.current
    val licenses = remember { licenseNames(context) }
    SettingsContent(
        page = page,
        onPage = { page = it },
        state = state,
        notificationsOn = notificationsOn,
        is24 = DateFormat.is24HourFormat(context),
        version = remember { versionName(context) },
        licenses = licenses,
        licenseText = { name -> licenseText(context, name) },
        connection = when (connection) {
            is ConnectionState.Connected -> SettingsConnection.Connected
            ConnectionState.Connecting -> SettingsConnection.Connecting
            else -> SettingsConnection.Offline
        },
        actions = SettingsActions(
            onBack = onBack,
            onNicknameChange = vm::onNicknameChange,
            onSaveNickname = vm::saveNickname,
            onResetNickname = vm::resetNickname,
            onPickAvatar = { picker.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly)) },
            onRemoveAvatar = vm::removeAvatar,
            onCopyJid = { copyText(context, state.jid) },
            onSignInShow = vm::setSignInShow,
            onSignInStatusChange = vm::onSignInStatusChange,
            onSaveSignInStatus = vm::saveSignInStatus,
            onChangePassword = vm::changePassword,
            onTheme = vm::setTheme,
            onShowPresence = vm::setShowPresence,
            onOpenNotificationSettings = {
                context.startActivity(
                    Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS)
                        .putExtra(Settings.EXTRA_APP_PACKAGE, context.packageName)
                        .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
                )
            },
            onNoticePreview = vm::setNoticePreview,
            onQuietHours = vm::setQuietHours,
            onQuietFrom = vm::setQuietFrom,
            onQuietTo = vm::setQuietTo,
            onShareInfo = vm::setShareInfo,
            onShareIdle = vm::setShareIdle,
            onIdleMinutes = vm::setIdleMinutes,
            onUnblock = vm::unblock,
            onUnblockAll = vm::unblockAll,
            onResetSettings = vm::resetSettings,
            onOpenWebsite = { uriHandler.openUri(WEBSITE) },
            onSignOut = vm::signOut,
            onLeavePassword = vm::clearPassword,
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
 * The settings without a ViewModel. [page] is the page on screen and [onPage] changes it. System
 * back, and the back arrow, go up one page; on the home page they call `actions.onBack`.
 *
 * @param notificationsOn the system lets the app show notifications.
 * @param is24 the phone shows a 24 hour clock.
 * @param licenses the file names in `assets/licenses`.
 * @param licenseText the text of one licence file. Shown when the user opens it.
 * @param initialTab the tab of the My account page.
 */
@Composable
fun SettingsContent(
    page: SettingsPage,
    onPage: (SettingsPage) -> Unit,
    state: SettingsState,
    notificationsOn: Boolean,
    version: String,
    licenses: List<String>,
    actions: SettingsActions,
    modifier: Modifier = Modifier,
    is24: Boolean = true,
    connection: SettingsConnection = SettingsConnection.Connected,
    licenseText: (String) -> String = { "" },
    initialTab: AccountTab = AccountTab.Profile,
) {
    val c = Chord.colors
    var confirmSignOut by rememberSaveable { mutableStateOf(false) }
    var tab by rememberSaveable { mutableStateOf(initialTab) }

    val up = {
        val parent = page.parent
        if (parent == null) actions.onBack()
        else {
            if (page == SettingsPage.Password) actions.onLeavePassword()
            onPage(parent)
        }
    }
    // On a sub-page the system back goes up one page. On the home page the nav host handles it.
    BackHandler(enabled = page != SettingsPage.Home) { up() }

    Column(modifier.fillMaxSize().background(c.surface100).statusBarsPadding().testTag("settings_screen")) {
        SettingsTopBar(pageTitle(page), up)
        Column(
            Modifier
                .weight(1f)
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .imePadding()
                .padding(horizontal = ChordSpace.s4)
                .padding(bottom = ChordSpace.s6)
                .testTag("settings_page_${page.name}"),
            verticalArrangement = Arrangement.spacedBy(ChordSpace.s2),
        ) {
            state.error?.let { ErrorNote(it) }
            when (page) {
                SettingsPage.Home -> HomePage(state, onPage) { confirmSignOut = true }
                SettingsPage.Account -> AccountPage(tab, { tab = it }, state, actions, onPage)
                SettingsPage.Password -> PasswordPage(state, actions)
                SettingsPage.Privacy -> PrivacyPage(state, actions)
                SettingsPage.Notifications -> NotificationsPage(state, notificationsOn, is24, actions)
                SettingsPage.Appearance -> AppearancePage(state, actions)
                SettingsPage.Advanced -> AdvancedPage(connection, actions)
                SettingsPage.About -> AboutPage(version, licenses, licenseText, actions)
            }
            Box(Modifier.navigationBarsPadding())
        }
    }
    if (confirmSignOut) {
        ConfirmDialog(
            title = stringResource(R.string.settings_sign_out_title),
            body = stringResource(R.string.settings_sign_out_body),
            confirm = stringResource(R.string.settings_sign_out),
            danger = true,
            tag = "confirm_sign_out",
            onDismiss = { confirmSignOut = false },
            onConfirm = { confirmSignOut = false; actions.onSignOut() },
        )
    }
}

package space.foid.chord.ui.settings

import android.content.Intent
import android.text.format.DateUtils
import android.text.format.Formatter
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.LifecycleResumeEffect
import kotlinx.coroutines.delay
import space.foid.chord.BuildConfig
import space.foid.chord.R
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordRadius
import space.foid.chord.ui.theme.ChordSpace
import space.foid.chord.ui.theme.ChordType
import space.foid.chord.update.ApkInstaller
import space.foid.chord.update.UpdateChannel
import space.foid.chord.update.UpdateFailure
import space.foid.chord.update.UpdateStatus
import space.foid.chord.update.Updater
import space.foid.chord.update.effectiveChannel
import space.foid.chord.update.isSlowerThanBuild

/** What the Updates section shows. */
@Immutable
data class UpdatesModel(
    /** False: the build does not update itself, and the section says so. */
    val enabled: Boolean,
    /** The channel of the installed build: stable, beta, nightly or dev. */
    val buildChannel: String,
    /** True for a store build (-Pchord.updater=false). */
    val storeBuild: Boolean = false,
    val channel: UpdateChannel = UpdateChannel.Stable,
    val autoCheck: Boolean = true,
    val status: UpdateStatus = UpdateStatus.Idle,
    /** ms, 0 for never. */
    val lastChecked: Long = 0L,
    /** The time to measure [lastChecked] from. */
    val now: Long = 0L,
    /** The user must let Chord install apps first. */
    val needsPermission: Boolean = false,
)

class UpdatesActions(
    val onChannel: (UpdateChannel) -> Unit = {},
    val onAutoCheck: (Boolean) -> Unit = {},
    val onCheck: () -> Unit = {},
    val onInstall: () -> Unit = {},
    val onRetry: () -> Unit = {},
)

/** The label of the channel a build came from, for the version line. */
@Composable
fun buildChannelLabel(buildChannel: String): String = stringResource(
    when (buildChannel) {
        "stable" -> R.string.updates_build_stable
        "beta" -> R.string.updates_build_beta
        "nightly" -> R.string.updates_build_nightly
        else -> R.string.updates_build_dev
    },
)

@Composable
private fun channelLabel(c: UpdateChannel): String = stringResource(
    when (c) {
        UpdateChannel.Stable -> R.string.updates_channel_stable
        UpdateChannel.Beta -> R.string.updates_channel_beta
        UpdateChannel.Nightly -> R.string.updates_channel_nightly
    },
)

/** The top of the About page: the name, the version with its channel, the website link. */
@Composable
internal fun AboutHeader(version: String, buildChannel: String, onOpenWebsite: () -> Unit) {
    val c = Chord.colors
    Group {
        Block {
            Text("Chord", style = ChordType.title, color = c.ink)
            Text(
                stringResource(R.string.updates_version_line, version, buildChannelLabel(buildChannel)),
                style = ChordType.bodySmall, color = c.inkMuted, modifier = Modifier.testTag("settings_version"),
            )
            Text(
                stringResource(R.string.settings_website),
                style = ChordType.body.copy(textDecoration = TextDecoration.Underline),
                color = c.brandInk,
                modifier = Modifier
                    .heightIn(min = 40.dp)
                    .clickable(role = Role.Button, onClick = onOpenWebsite)
                    .padding(vertical = ChordSpace.s2)
                    .testTag("settings_website"),
            )
        }
    }
}

/** The Updates section, fed by [Updater] and the settings store. */
@Composable
internal fun UpdatesSection() {
    val context = LocalContext.current
    val updater = remember { Updater.get(context) }
    val store = remember { PrefsSettingsStore.get(context) }
    val prefs by store.prefs.collectAsState()
    val status by updater.status.collectAsState()
    val lastChecked by updater.lastChecked.collectAsState()
    var needsPermission by rememberSaveable { mutableStateOf(false) }
    // The "Last checked" line moves on while the page is open.
    var now by remember { mutableStateOf(System.currentTimeMillis()) }
    LaunchedEffect(Unit) {
        while (true) {
            delay(30_000)
            now = System.currentTimeMillis()
        }
    }
    // Back from the system page that lets Chord install apps: go on with the install.
    LifecycleResumeEffect(needsPermission) {
        now = System.currentTimeMillis()
        if (needsPermission && ApkInstaller.canInstall(context)) {
            needsPermission = false
            updater.install()
        }
        onPauseOrDispose {}
    }
    val install = {
        if (ApkInstaller.canInstall(context)) {
            updater.install()
        } else {
            needsPermission = true
            context.startActivity(ApkInstaller.unknownSourcesIntent(context).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        }
    }
    val actions = remember(updater, store) {
        UpdatesActions(
            onChannel = { ch ->
                store.update { it.copy(updateChannel = ch) }
                updater.onChannelChanged()
            },
            onAutoCheck = { on -> store.update { it.copy(autoUpdateCheck = on) } },
            onCheck = updater::check,
            onInstall = install,
            onRetry = {
                val failed = updater.status.value as? UpdateStatus.Failed
                if (failed?.available != null) install() else updater.retry()
            },
        )
    }
    UpdatesContent(
        UpdatesModel(
            enabled = updater.enabled,
            buildChannel = BuildConfig.CHANNEL,
            storeBuild = !BuildConfig.UPDATER,
            channel = effectiveChannel(prefs.updateChannel, BuildConfig.CHANNEL),
            autoCheck = prefs.autoUpdateCheck,
            status = status,
            lastChecked = lastChecked,
            now = now,
            needsPermission = needsPermission,
        ),
        actions,
    )
}

/** The stateless Updates section, for the screenshot tests. */
@Composable
internal fun UpdatesContent(model: UpdatesModel, a: UpdatesActions, notesOpen: Boolean = false) {
    Group(stringResource(R.string.updates_title)) {
        if (!model.enabled) {
            Block {
                Hint(
                    stringResource(if (model.storeBuild) R.string.updates_off_store else R.string.updates_off_dev),
                    Modifier.testTag("updates_off"),
                )
            }
            return@Group
        }
        Block {
            Label(stringResource(R.string.updates_channel))
            Segmented(
                options = UpdateChannel.entries.map { it to channelLabel(it) },
                selected = model.channel, onSelect = a.onChannel, tag = "update_channel",
            )
            Hint(
                stringResource(
                    when (model.channel) {
                        UpdateChannel.Stable -> R.string.updates_hint_stable
                        UpdateChannel.Beta -> R.string.updates_hint_beta
                        UpdateChannel.Nightly -> R.string.updates_hint_nightly
                    },
                ),
            )
            if (isSlowerThanBuild(model.channel, model.buildChannel)) {
                Text(
                    stringResource(R.string.updates_keep_build, channelLabel(model.channel)),
                    style = ChordType.bodySmall, color = Chord.colors.ink,
                    modifier = Modifier
                        .fillMaxWidth()
                        .clip(RoundedCornerShape(ChordRadius.sm))
                        .background(Chord.colors.brandSoft)
                        .padding(horizontal = ChordSpace.s3, vertical = ChordSpace.s2)
                        .testTag("updates_keep_build"),
                )
            }
        }
        RowDivider()
        ToggleRow(
            stringResource(R.string.updates_auto), model.autoCheck, a.onAutoCheck, "updates_auto",
            hint = stringResource(R.string.updates_auto_hint),
        )
        RowDivider()
        StatusBlock(model, a, notesOpen)
    }
}

@Composable
private fun lastCheckedText(model: UpdatesModel): String {
    if (model.lastChecked <= 0L) return stringResource(R.string.updates_never_checked)
    val ago = model.now - model.lastChecked
    val whenText = if (ago < DateUtils.MINUTE_IN_MILLIS) {
        stringResource(R.string.updates_just_now)
    } else {
        DateUtils.getRelativeTimeSpanString(model.lastChecked, model.now, DateUtils.MINUTE_IN_MILLIS).toString()
    }
    return stringResource(R.string.updates_last_checked, whenText)
}

@Composable
private fun failureText(f: UpdateFailure): String = stringResource(
    when (f) {
        UpdateFailure.Network -> R.string.updates_failed_network
        UpdateFailure.BadManifest -> R.string.updates_failed_manifest
        UpdateFailure.NoApk -> R.string.updates_failed_no_apk
        UpdateFailure.Checksum -> R.string.updates_failed_checksum
        UpdateFailure.Install -> R.string.updates_failed_install
    },
)

@Composable
private fun StatusBlock(model: UpdatesModel, a: UpdatesActions, notesOpen: Boolean) {
    val c = Chord.colors
    val context = LocalContext.current
    val s = model.status
    Column(
        Modifier.fillMaxWidth().padding(ChordSpace.s3).testTag("updates_status"),
        verticalArrangement = Arrangement.spacedBy(ChordSpace.s2),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(ChordSpace.s3)) {
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                when (s) {
                    UpdateStatus.Idle -> Text(lastCheckedText(model), style = ChordType.body, color = c.ink)
                    UpdateStatus.Checking -> Text(stringResource(R.string.updates_checking), style = ChordType.body, color = c.inkMuted)
                    is UpdateStatus.UpToDate -> {
                        Text(stringResource(R.string.updates_up_to_date), style = ChordType.body, color = c.ink)
                        Hint(lastCheckedText(model))
                    }
                    UpdateStatus.NoBuild -> {
                        Text(stringResource(R.string.updates_no_build), style = ChordType.body, color = c.ink)
                        Hint(lastCheckedText(model))
                    }
                    is UpdateStatus.Available -> {
                        Text(stringResource(R.string.updates_available, s.manifest.version), style = ChordType.label, color = c.brandInk)
                        if (s.apk.size > 0) Hint(stringResource(R.string.updates_size, Formatter.formatShortFileSize(context, s.apk.size)))
                    }
                    is UpdateStatus.Downloading -> {
                        Text(stringResource(R.string.updates_downloading, s.available.manifest.version), style = ChordType.body, color = c.ink)
                        Hint(
                            if (s.total > 0) {
                                stringResource(
                                    R.string.updates_progress,
                                    Formatter.formatShortFileSize(context, s.done),
                                    Formatter.formatShortFileSize(context, s.total),
                                )
                            } else {
                                Formatter.formatShortFileSize(context, s.done)
                            },
                        )
                    }
                    is UpdateStatus.Installing -> {
                        Text(stringResource(R.string.updates_installing, s.available.manifest.version), style = ChordType.body, color = c.ink)
                        Hint(stringResource(R.string.updates_install_confirm))
                    }
                    is UpdateStatus.Failed -> Text(failureText(s.reason), style = ChordType.bodySmall, color = c.danger, modifier = Modifier.testTag("updates_error"))
                }
            }
            when (s) {
                UpdateStatus.Idle, is UpdateStatus.UpToDate, UpdateStatus.NoBuild ->
                    SmallButton(stringResource(R.string.updates_check), a.onCheck, Modifier.testTag("updates_check"))
                UpdateStatus.Checking ->
                    SmallButton(stringResource(R.string.updates_check), {}, Modifier.testTag("updates_check"), enabled = false)
                is UpdateStatus.Available ->
                    SmallButton(stringResource(R.string.updates_install), a.onInstall, Modifier.testTag("updates_install"), primary = true)
                is UpdateStatus.Failed ->
                    SmallButton(stringResource(R.string.updates_retry), a.onRetry, Modifier.testTag("updates_retry"))
                else -> Unit
            }
        }
        if (s is UpdateStatus.Downloading) ProgressBar(s.fraction)
        if (s is UpdateStatus.Available) {
            if (model.needsPermission) Hint(stringResource(R.string.updates_permission))
            if (s.manifest.notes.isNotBlank()) Notes(s.manifest.notes, notesOpen)
        }
    }
}

@Composable
private fun ProgressBar(fraction: Float?) {
    val c = Chord.colors
    Box(
        Modifier
            .fillMaxWidth()
            .height(6.dp)
            .clip(RoundedCornerShape(3.dp))
            .background(c.surface300)
            .testTag("updates_progress"),
    ) {
        Box(Modifier.fillMaxWidth(fraction ?: 0f).fillMaxHeight().clip(RoundedCornerShape(3.dp)).background(c.brand))
    }
}

/** The release notes, closed at first. The Markdown shows as plain text. */
@Composable
private fun Notes(notes: String, initiallyOpen: Boolean) {
    val c = Chord.colors
    var open by rememberSaveable { mutableStateOf(initiallyOpen) }
    Text(
        stringResource(if (open) R.string.updates_notes_hide else R.string.updates_notes_show),
        style = ChordType.label, color = c.brandInk,
        modifier = Modifier
            .heightIn(min = 40.dp)
            .clickable(role = Role.Button) { open = !open }
            .padding(vertical = ChordSpace.s2)
            .testTag("updates_notes_toggle"),
    )
    if (open) {
        Text(
            notes.trim(),
            style = ChordType.bodySmall, color = c.inkMuted,
            modifier = Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(ChordRadius.sm))
                .background(c.surface100)
                .padding(ChordSpace.s3)
                .testTag("updates_notes"),
        )
    }
}

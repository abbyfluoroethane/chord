package space.foid.chord.ui.register

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.autofill.ContentType
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import space.foid.chord.R
import space.foid.chord.ui.forms.ChordTextField
import space.foid.chord.ui.forms.DataFormView
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordSpace
import space.foid.chord.ui.theme.ChordType
import space.foid.chord.viewmodel.RegisterState
import space.foid.chord.viewmodel.RegisterStep
import space.foid.chord.viewmodel.RegisterViewModel
import uniffi.chord_ffi.DataForm

/**
 * "Create an account": the server step, then the form of the server. [address] and [server] come
 * from the sign-in form. [onBack] leaves to the sign-in form. [onSignedIn] runs after the new account signed in.
 */
@Composable
fun RegisterScreen(
    address: String,
    server: String,
    onBack: () -> Unit,
    onSignedIn: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val vm: RegisterViewModel = viewModel(factory = RegisterViewModel.factory(address, server))
    val state by vm.state.collectAsStateWithLifecycle()
    LaunchedEffect(state.signedIn) {
        if (state.signedIn) onSignedIn()
    }
    val back = { if (!vm.back()) onBack() }
    BackHandler(onBack = back)
    RegisterContent(
        state = state,
        onBack = back,
        onDomainChange = vm::onDomainChange,
        onContinue = vm::fetchForm,
        onFormChange = vm::onFormChange,
        onLegacyChange = vm::onLegacyChange,
        onSubmit = vm::submit,
        modifier = modifier,
    )
}

/** The stateless registration screen. The tests render it with fixed states. */
@Composable
fun RegisterContent(
    state: RegisterState,
    onBack: () -> Unit,
    onDomainChange: (String) -> Unit,
    onContinue: () -> Unit,
    onFormChange: (DataForm) -> Unit,
    onLegacyChange: (name: String, value: String) -> Unit,
    onSubmit: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val c = Chord.colors
    val uriHandler = LocalUriHandler.current
    val onServerStep = state.step == RegisterStep.SERVER
    Column(
        modifier
            .fillMaxSize()
            .background(c.surface100)
            .safeDrawingPadding()
            .imePadding(),
    ) {
        Column(
            Modifier
                .weight(1f)
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = ChordSpace.s6, vertical = ChordSpace.s3),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Column(Modifier.widthIn(max = 420.dp).fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(ChordSpace.s3)) {
                AuthBack(stringResource(R.string.register_back), onBack, Modifier.testTag("register_back"))
                Text(stringResource(R.string.register_title), style = ChordType.title, color = c.ink)
                if (onServerStep) {
                    ChordTextField(
                        value = state.domain,
                        onChange = onDomainChange,
                        label = stringResource(R.string.register_server),
                        placeholder = stringResource(R.string.register_server_hint),
                        tag = "register_domain",
                        enabled = !state.busy,
                        mono = true,
                        keyboardOptions = KeyboardOptions(
                            capitalization = KeyboardCapitalization.None,
                            autoCorrectEnabled = false,
                            keyboardType = KeyboardType.Uri,
                            imeAction = ImeAction.Go,
                        ),
                        keyboardActions = KeyboardActions(onGo = { if (state.canContinue) onContinue() }),
                    )
                    Text(stringResource(R.string.register_server_help), style = ChordType.caption, color = c.inkMuted)
                } else {
                    val info = state.info
                    info?.instructions?.takeIf { it.isNotBlank() }?.let {
                        Text(it, style = ChordType.body, color = c.inkMuted)
                    }
                    if (state.form != null) {
                        DataFormView(
                            form = state.form,
                            onChange = onFormChange,
                            enabled = !state.busy,
                            showProblems = state.showProblems,
                            idPrefix = "register",
                        )
                    } else if (info != null) {
                        legacyFields(info.fields).forEach { f ->
                            ChordTextField(
                                value = state.legacy[f.name] ?: "",
                                onChange = { onLegacyChange(f.name, it) },
                                label = f.label,
                                tag = "register_${f.name}",
                                enabled = !state.busy,
                                mono = f.name == "username",
                                contentType = if (f.name == "username") ContentType.NewUsername else if (f.secret) ContentType.NewPassword else null,
                                visualTransformation = if (f.secret) PasswordVisualTransformation() else VisualTransformation.None,
                                keyboardOptions = KeyboardOptions(
                                    capitalization = KeyboardCapitalization.None,
                                    autoCorrectEnabled = false,
                                    keyboardType = if (f.secret) KeyboardType.Password else KeyboardType.Text,
                                ),
                            )
                        }
                    }
                    info?.oob?.let { oob ->
                        Text(oob.desc ?: stringResource(R.string.register_oob_default), style = ChordType.body, color = c.inkMuted)
                        if (oob.url.startsWith("https://") || oob.url.startsWith("http://")) {
                            AuthLink(
                                label = stringResource(R.string.register_oob_open),
                                onClick = { uriHandler.openUri(oob.url) },
                                tag = "register_oob",
                                color = c.accent,
                            )
                        }
                    }
                }
                if (state.error != null) AuthError(state.error, "register_error")
                if (!onServerStep && state.linkOnly) {
                    Text(stringResource(R.string.register_link_only), style = ChordType.body, color = c.inkMuted)
                }
            }
        }
        if (!(!onServerStep && state.linkOnly)) {
            Box(
                Modifier.fillMaxWidth().padding(horizontal = ChordSpace.s6, vertical = ChordSpace.s3),
                contentAlignment = Alignment.Center,
            ) {
                if (onServerStep) {
                    AuthButton(
                        label = stringResource(R.string.register_continue),
                        busyLabel = stringResource(R.string.register_connecting),
                        busy = state.busy,
                        enabled = state.canContinue,
                        onClick = onContinue,
                        tag = "register_continue",
                    )
                } else {
                    AuthButton(
                        label = stringResource(R.string.register_create),
                        busyLabel = stringResource(if (state.signingIn) R.string.register_signing_in else R.string.register_creating),
                        busy = state.busy,
                        enabled = true,
                        onClick = onSubmit,
                        tag = "register_submit",
                    )
                }
            }
        }
    }
}

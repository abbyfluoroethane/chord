package space.foid.chord.ui.screens

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.autofill.ContentType
import androidx.compose.ui.focus.FocusDirection
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import space.foid.chord.R
import space.foid.chord.ui.forms.ChordTextField
import space.foid.chord.ui.register.AuthButton
import space.foid.chord.ui.register.AuthError
import space.foid.chord.ui.register.AuthLink
import space.foid.chord.ui.register.RegisterScreen
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordSpace
import space.foid.chord.ui.theme.ChordType
import space.foid.chord.viewmodel.ChordViewModels
import space.foid.chord.viewmodel.SignInState
import space.foid.chord.viewmodel.SignInViewModel

/**
 * The sign-in screen. [onSignedIn] runs once, after the sign-in succeeds, also after a new
 * account signed in. "Create an account" swaps this screen for the registration.
 */
@Composable
fun SignInScreen(onSignedIn: () -> Unit, modifier: Modifier = Modifier) {
    val vm: SignInViewModel = viewModel(factory = ChordViewModels.signIn)
    val state by vm.state.collectAsStateWithLifecycle()
    var registering by rememberSaveable { mutableStateOf(false) }
    LaunchedEffect(state.signedIn) {
        if (state.signedIn) onSignedIn()
    }
    if (registering) {
        RegisterScreen(
            address = state.jid,
            server = state.server,
            onBack = { registering = false },
            onSignedIn = onSignedIn,
            modifier = modifier,
        )
        return
    }
    SignInContent(
        state = state,
        onJidChange = vm::onJidChange,
        onPasswordChange = vm::onPasswordChange,
        onServerChange = vm::onServerChange,
        onSubmit = vm::submit,
        onCreateAccount = { registering = true },
        modifier = modifier,
    )
}

/** The stateless sign-in form. The tests render it with fixed states. */
@Composable
fun SignInContent(
    state: SignInState,
    onJidChange: (String) -> Unit,
    onPasswordChange: (String) -> Unit,
    onServerChange: (String) -> Unit,
    onSubmit: () -> Unit,
    modifier: Modifier = Modifier,
    onCreateAccount: () -> Unit = {},
    advancedInitiallyOpen: Boolean = false,
    passwordInitiallyVisible: Boolean = false,
) {
    var advancedOpen by rememberSaveable { mutableStateOf(advancedInitiallyOpen) }
    var passwordVisible by rememberSaveable { mutableStateOf(passwordInitiallyVisible) }
    val focus = LocalFocusManager.current
    val enabled = !state.submitting
    val submit = {
        focus.clearFocus()
        if (state.canSubmit) onSubmit()
    }

    Column(
        modifier
            .fillMaxSize()
            .background(Chord.colors.surface100)
            .safeDrawingPadding()
            .imePadding(),
    ) {
        // The form scrolls. The button below stays on screen over the keyboard.
        Column(
            Modifier
                .weight(1f)
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = ChordSpace.s6, vertical = ChordSpace.s6),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Brand()
            Spacer(Modifier.size(ChordSpace.s8))
            Column(
                Modifier.widthIn(max = 420.dp).fillMaxWidth(),
                verticalArrangement = Arrangement.spacedBy(ChordSpace.s3),
            ) {
                Text(stringResource(R.string.signin_title), style = ChordType.title, color = Chord.colors.ink)
                ChordTextField(
                    value = state.jid,
                    onChange = onJidChange,
                    label = stringResource(R.string.signin_address),
                    placeholder = stringResource(R.string.signin_address_hint),
                    tag = "login_jid",
                    enabled = enabled,
                    mono = true,
                    isError = state.error != null,
                    contentType = ContentType.Username,
                    keyboardOptions = KeyboardOptions(
                        capitalization = KeyboardCapitalization.None,
                        autoCorrectEnabled = false,
                        keyboardType = KeyboardType.Email,
                        imeAction = ImeAction.Next,
                    ),
                    keyboardActions = KeyboardActions(onNext = { focus.moveFocus(FocusDirection.Down) }),
                )
                ChordTextField(
                    value = state.password,
                    onChange = onPasswordChange,
                    label = stringResource(R.string.signin_password),
                    tag = "login_password",
                    enabled = enabled,
                    isError = state.error != null,
                    contentType = ContentType.Password,
                    visualTransformation = if (passwordVisible) VisualTransformation.None else PasswordVisualTransformation(),
                    keyboardOptions = KeyboardOptions(
                        capitalization = KeyboardCapitalization.None,
                        autoCorrectEnabled = false,
                        keyboardType = KeyboardType.Password,
                        imeAction = if (advancedOpen) ImeAction.Next else ImeAction.Done,
                    ),
                    keyboardActions = KeyboardActions(
                        onNext = { focus.moveFocus(FocusDirection.Down) },
                        onDone = { submit() },
                    ),
                    trailing = {
                        Text(
                            text = stringResource(if (passwordVisible) R.string.signin_hide else R.string.signin_show),
                            style = ChordType.label,
                            color = Chord.colors.brandInk,
                            modifier = Modifier
                                .clickable(role = Role.Button) { passwordVisible = !passwordVisible }
                                .padding(horizontal = ChordSpace.s3, vertical = ChordSpace.s3),
                        )
                    },
                )
                AdvancedToggle(open = advancedOpen, onToggle = { advancedOpen = !advancedOpen })
                AnimatedVisibility(advancedOpen) {
                    Column(verticalArrangement = Arrangement.spacedBy(ChordSpace.s1)) {
                        ChordTextField(
                            value = state.server,
                            onChange = onServerChange,
                            label = stringResource(R.string.signin_server),
                            placeholder = stringResource(R.string.signin_server_hint),
                            tag = "login_server",
                            enabled = enabled,
                            mono = true,
                            keyboardOptions = KeyboardOptions(
                                capitalization = KeyboardCapitalization.None,
                                autoCorrectEnabled = false,
                                keyboardType = KeyboardType.Uri,
                                imeAction = ImeAction.Done,
                            ),
                            keyboardActions = KeyboardActions(onDone = { submit() }),
                        )
                        Text(
                            stringResource(R.string.signin_server_help),
                            style = ChordType.caption,
                            color = Chord.colors.inkMuted,
                        )
                    }
                }
                if (state.error != null) AuthError(state.error, "login_error")
            }
        }
        Column(
            Modifier.fillMaxWidth().padding(horizontal = ChordSpace.s6, vertical = ChordSpace.s3),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            AuthButton(
                label = stringResource(R.string.signin_submit),
                busyLabel = stringResource(R.string.signin_submitting),
                busy = state.submitting,
                enabled = state.canSubmit,
                onClick = submit,
                tag = "login_submit",
            )
            if (state.submitting) {
                val host = state.host.ifEmpty { stringResource(R.string.signin_status_default) }
                Text(
                    stringResource(R.string.signin_status, host),
                    style = ChordType.caption,
                    color = Chord.colors.inkMuted,
                    modifier = Modifier.padding(top = ChordSpace.s2).testTag("login_status"),
                )
            }
            AuthLink(
                label = stringResource(R.string.signin_create_account),
                onClick = onCreateAccount,
                enabled = !state.submitting,
                tag = "login_create_account",
            )
        }
    }
}

@Composable
private fun Brand() {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(ChordSpace.s3)) {
        Image(
            painter = painterResource(if (Chord.colors.isDark) R.drawable.chord_mark_dark else R.drawable.chord_mark_light),
            contentDescription = null,
            modifier = Modifier.size(48.dp),
        )
        Text("chord", style = ChordType.title.copy(fontSize = 40.sp, lineHeight = 44.sp), color = Chord.colors.ink)
    }
}

/** "Advanced" with a chevron that points down when open and right when closed (desktop: `.adv`). */
@Composable
private fun AdvancedToggle(open: Boolean, onToggle: () -> Unit) {
    val color = Chord.colors.inkMuted
    Row(
        Modifier
            .heightIn(min = 48.dp)
            .testTag("login_advanced")
            .clickable(role = Role.Button, onClick = onToggle),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(ChordSpace.s1),
    ) {
        Canvas(Modifier.size(16.dp)) {
            val w = 1.5.dp.toPx()
            val cx = size.width / 2
            val cy = size.height / 2
            val r = size.width * 0.22f
            if (open) {
                drawLine(color, Offset(cx - r * 1.4f, cy - r * 0.6f), Offset(cx, cy + r * 0.8f), w, StrokeCap.Round)
                drawLine(color, Offset(cx, cy + r * 0.8f), Offset(cx + r * 1.4f, cy - r * 0.6f), w, StrokeCap.Round)
            } else {
                drawLine(color, Offset(cx - r * 0.6f, cy - r * 1.4f), Offset(cx + r * 0.8f, cy), w, StrokeCap.Round)
                drawLine(color, Offset(cx + r * 0.8f, cy), Offset(cx - r * 0.6f, cy + r * 1.4f), w, StrokeCap.Round)
            }
        }
        Text(stringResource(R.string.signin_advanced), style = ChordType.label, color = color)
    }
}

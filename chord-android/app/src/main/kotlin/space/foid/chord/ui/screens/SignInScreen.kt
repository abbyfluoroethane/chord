package space.foid.chord.ui.screens

import androidx.compose.animation.AnimatedVisibility
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
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
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
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentType
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
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
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordRadius
import space.foid.chord.ui.theme.ChordSpace
import space.foid.chord.ui.theme.ChordType
import space.foid.chord.viewmodel.ChordViewModels
import space.foid.chord.viewmodel.SignInState
import space.foid.chord.viewmodel.SignInViewModel

/** The sign-in screen. [onSignedIn] runs once, after the sign-in succeeds. */
@Composable
fun SignInScreen(onSignedIn: () -> Unit, modifier: Modifier = Modifier) {
    val vm: SignInViewModel = viewModel(factory = ChordViewModels.signIn)
    val state by vm.state.collectAsStateWithLifecycle()
    LaunchedEffect(state.signedIn) {
        if (state.signedIn) onSignedIn()
    }
    SignInContent(
        state = state,
        onJidChange = vm::onJidChange,
        onPasswordChange = vm::onPasswordChange,
        onServerChange = vm::onServerChange,
        onSubmit = vm::submit,
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
                Text(stringResource(R.string.sign_in_title), style = ChordType.title, color = Chord.colors.ink)
                Text(stringResource(R.string.sign_in_intro), style = ChordType.bodySmall, color = Chord.colors.inkMuted)
                Field(
                    value = state.jid,
                    onChange = onJidChange,
                    label = stringResource(R.string.sign_in_address),
                    placeholder = "name@server.example",
                    tag = "login_jid",
                    enabled = enabled,
                    contentType = ContentType.Username,
                    keyboardOptions = KeyboardOptions(
                        capitalization = KeyboardCapitalization.None,
                        autoCorrectEnabled = false,
                        keyboardType = KeyboardType.Email,
                        imeAction = ImeAction.Next,
                    ),
                    keyboardActions = KeyboardActions(onNext = { focus.moveFocus(FocusDirection.Down) }),
                )
                Field(
                    value = state.password,
                    onChange = onPasswordChange,
                    label = stringResource(R.string.sign_in_password),
                    tag = "login_password",
                    enabled = enabled,
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
                            text = stringResource(if (passwordVisible) R.string.sign_in_hide else R.string.sign_in_show),
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
                        Field(
                            value = state.server,
                            onChange = onServerChange,
                            label = stringResource(R.string.sign_in_server),
                            placeholder = stringResource(R.string.sign_in_server_hint),
                            tag = "login_server",
                            enabled = enabled,
                            keyboardOptions = KeyboardOptions(
                                capitalization = KeyboardCapitalization.None,
                                autoCorrectEnabled = false,
                                keyboardType = KeyboardType.Uri,
                                imeAction = ImeAction.Done,
                            ),
                            keyboardActions = KeyboardActions(onDone = { submit() }),
                        )
                        Text(
                            stringResource(R.string.sign_in_server_help),
                            style = ChordType.caption,
                            color = Chord.colors.inkMuted,
                        )
                    }
                }
                if (state.error != null) {
                    Text(
                        text = state.error,
                        style = ChordType.bodySmall,
                        color = Chord.colors.danger,
                        modifier = Modifier
                            .fillMaxWidth()
                            .semantics { liveRegion = LiveRegionMode.Polite }
                            .testTag("login_error"),
                    )
                }
            }
        }
        Box(
            Modifier.fillMaxWidth().padding(horizontal = ChordSpace.s6, vertical = ChordSpace.s3),
            contentAlignment = Alignment.Center,
        ) {
            SubmitButton(state = state, onClick = submit)
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

@Composable
private fun AdvancedToggle(open: Boolean, onToggle: () -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 48.dp)
            .testTag("login_advanced")
            .clickable(role = Role.Button, onClick = onToggle),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            text = stringResource(R.string.sign_in_advanced) + if (open) "  ▴" else "  ▾",
            style = ChordType.label,
            color = Chord.colors.inkMuted,
        )
    }
}

@Composable
private fun SubmitButton(state: SignInState, onClick: () -> Unit) {
    val c = Chord.colors
    Button(
        onClick = onClick,
        enabled = state.canSubmit,
        shape = RoundedCornerShape(ChordRadius.md),
        colors = ButtonDefaults.buttonColors(
            containerColor = c.brand,
            contentColor = c.onBrand,
            // While it submits, the button keeps its colour so the progress state stays readable.
            disabledContainerColor = if (state.submitting) c.brand else c.surface300,
            disabledContentColor = if (state.submitting) c.onBrand else c.inkMuted,
        ),
        modifier = Modifier
            .widthIn(max = 420.dp)
            .fillMaxWidth()
            .heightIn(min = 52.dp)
            .testTag("login_submit"),
    ) {
        if (state.submitting) {
            CircularProgressIndicator(
                modifier = Modifier.size(20.dp),
                color = c.onBrand,
                strokeWidth = 2.dp,
            )
            Spacer(Modifier.size(ChordSpace.s3))
            Text(stringResource(R.string.sign_in_submitting), style = ChordType.name)
        } else {
            Text(stringResource(R.string.sign_in_submit), style = ChordType.name)
        }
    }
}

@Composable
private fun Field(
    value: String,
    onChange: (String) -> Unit,
    label: String,
    tag: String,
    enabled: Boolean,
    keyboardOptions: KeyboardOptions,
    placeholder: String? = null,
    contentType: ContentType? = null,
    visualTransformation: VisualTransformation = VisualTransformation.None,
    keyboardActions: KeyboardActions = KeyboardActions.Default,
    trailing: (@Composable () -> Unit)? = null,
) {
    val c = Chord.colors
    OutlinedTextField(
        value = value,
        onValueChange = onChange,
        enabled = enabled,
        singleLine = true,
        label = { Text(label, style = ChordType.bodySmall) },
        placeholder = placeholder?.let { { Text(it, style = ChordType.body) } },
        trailingIcon = trailing,
        visualTransformation = visualTransformation,
        keyboardOptions = keyboardOptions,
        keyboardActions = keyboardActions,
        textStyle = ChordType.body.copy(color = c.ink),
        shape = RoundedCornerShape(ChordRadius.md),
        colors = OutlinedTextFieldDefaults.colors(
            focusedTextColor = c.ink,
            unfocusedTextColor = c.ink,
            disabledTextColor = c.inkMuted,
            focusedContainerColor = c.surface200,
            unfocusedContainerColor = c.surface200,
            disabledContainerColor = c.surface200,
            focusedBorderColor = c.brand,
            unfocusedBorderColor = c.lineStrong,
            disabledBorderColor = c.line,
            focusedLabelColor = c.brandInk,
            unfocusedLabelColor = c.inkMuted,
            disabledLabelColor = c.inkMuted,
            cursorColor = c.brand,
            focusedPlaceholderColor = c.inkMuted,
            unfocusedPlaceholderColor = c.inkMuted,
            disabledPlaceholderColor = c.inkMuted,
        ),
        modifier = Modifier
            .fillMaxWidth()
            .testTag(tag)
            .then(if (contentType != null) Modifier.semantics { this.contentType = contentType } else Modifier),
    )
}

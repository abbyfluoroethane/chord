package space.foid.chord.ui.join

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordRadius
import space.foid.chord.ui.theme.ChordSpace
import space.foid.chord.ui.theme.ChordType

/** A text field with the Chord tokens. */
@Composable
internal fun JoinField(
    value: String,
    onChange: (String) -> Unit,
    label: String,
    tag: String,
    modifier: Modifier = Modifier,
    placeholder: String? = null,
    enabled: Boolean = true,
    keyboardOptions: KeyboardOptions = KeyboardOptions.Default,
    keyboardActions: KeyboardActions = KeyboardActions.Default,
    visualTransformation: VisualTransformation = VisualTransformation.None,
    isError: Boolean = false,
) {
    val c = Chord.colors
    OutlinedTextField(
        value = value,
        onValueChange = onChange,
        enabled = enabled,
        singleLine = true,
        isError = isError,
        label = { Text(label, style = ChordType.bodySmall) },
        placeholder = placeholder?.let { { Text(it, style = ChordType.body) } },
        visualTransformation = visualTransformation,
        keyboardOptions = keyboardOptions,
        keyboardActions = keyboardActions,
        textStyle = ChordType.body.copy(color = c.ink),
        shape = RoundedCornerShape(ChordRadius.md),
        colors = OutlinedTextFieldDefaults.colors(
            focusedTextColor = c.ink,
            unfocusedTextColor = c.ink,
            disabledTextColor = c.inkMuted,
            errorTextColor = c.ink,
            focusedContainerColor = c.surface200,
            unfocusedContainerColor = c.surface200,
            disabledContainerColor = c.surface200,
            errorContainerColor = c.surface200,
            focusedBorderColor = c.brand,
            unfocusedBorderColor = c.lineStrong,
            disabledBorderColor = c.line,
            errorBorderColor = c.danger,
            focusedLabelColor = c.brandInk,
            unfocusedLabelColor = c.inkMuted,
            disabledLabelColor = c.inkMuted,
            errorLabelColor = c.danger,
            cursorColor = c.brand,
            errorCursorColor = c.brand,
            focusedPlaceholderColor = c.inkMuted,
            unfocusedPlaceholderColor = c.inkMuted,
            disabledPlaceholderColor = c.inkMuted,
            errorPlaceholderColor = c.inkMuted,
        ),
        modifier = modifier.fillMaxWidth().testTag(tag),
    )
}

/** The main button of a form. While [busy] it shows a spinner and keeps its colour. */
@Composable
internal fun JoinButton(
    text: String,
    onClick: () -> Unit,
    tag: String,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    busy: Boolean = false,
    danger: Boolean = false,
    compact: Boolean = false,
) {
    val c = Chord.colors
    val live = enabled && !busy
    val fill = when {
        busy -> if (danger) c.danger else c.brand
        !enabled -> c.surface300
        danger -> c.danger
        else -> c.brand
    }
    val ink = when {
        busy || enabled -> if (danger) c.onDanger else c.onBrand
        else -> c.inkMuted
    }
    Row(
        modifier
            .heightIn(min = if (compact) 40.dp else 52.dp)
            .clip(RoundedCornerShape(ChordRadius.md))
            .background(fill)
            .clickable(enabled = live, role = Role.Button, onClick = onClick)
            .padding(horizontal = ChordSpace.s4)
            .testTag(tag),
        horizontalArrangement = Arrangement.Center,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (busy) {
            CircularProgressIndicator(Modifier.size(18.dp), color = ink, strokeWidth = 2.dp)
            Spacer(Modifier.width(ChordSpace.s2))
        }
        Text(text, style = if (compact) ChordType.label else ChordType.name, color = ink)
    }
}

/** A quiet button, with no fill. */
@Composable
internal fun JoinTextButton(
    text: String,
    onClick: () -> Unit,
    tag: String,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    color: Color = Chord.colors.ink,
) {
    Box(
        modifier
            .heightIn(min = 40.dp)
            .clip(RoundedCornerShape(ChordRadius.md))
            .background(Chord.colors.surface300)
            .clickable(enabled = enabled, role = Role.Button, onClick = onClick)
            .padding(horizontal = ChordSpace.s4)
            .alpha(if (enabled) 1f else 0.5f)
            .testTag(tag),
        contentAlignment = Alignment.Center,
    ) { Text(text, style = ChordType.label, color = color) }
}

/** The text of a failure, under a form. */
@Composable
internal fun JoinError(text: String, tag: String, modifier: Modifier = Modifier) {
    Text(
        text,
        style = ChordType.bodySmall,
        color = Chord.colors.danger,
        modifier = modifier.fillMaxWidth().testTag(tag).semantics { contentDescription = "Error: $text" },
    )
}

/** The title row of a sheet. */
@Composable
internal fun JoinTitle(text: String, modifier: Modifier = Modifier) {
    Text(text, style = ChordType.title, color = Chord.colors.ink, modifier = modifier.fillMaxWidth())
}

/** A question card for a leave or remove. It has no window: [ConfirmDialog] adds one. */
@Composable
internal fun ConfirmCard(
    title: String,
    text: String,
    confirmLabel: String,
    cancelLabel: String,
    onConfirm: () -> Unit,
    onCancel: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val c = Chord.colors
    Column(
        modifier.fillMaxWidth().clip(RoundedCornerShape(ChordRadius.lg)).background(c.surface200)
            .border(1.dp, c.line, RoundedCornerShape(ChordRadius.lg))
            .padding(ChordSpace.s6),
    ) {
        Text(title, style = ChordType.title, color = c.ink)
        Spacer(Modifier.height(ChordSpace.s2))
        Text(text, style = ChordType.body, color = c.inkMuted)
        Spacer(Modifier.height(ChordSpace.s6))
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End, verticalAlignment = Alignment.CenterVertically) {
            JoinTextButton(cancelLabel, onCancel, "confirm_cancel")
            Spacer(Modifier.width(ChordSpace.s2))
            JoinButton(confirmLabel, onConfirm, "confirm_ok", danger = true, compact = true)
        }
    }
}

@Composable
internal fun ConfirmDialog(
    title: String,
    text: String,
    confirmLabel: String,
    cancelLabel: String,
    onConfirm: () -> Unit,
    onCancel: () -> Unit,
) {
    androidx.compose.ui.window.Dialog(onDismissRequest = onCancel) {
        ConfirmCard(title, text, confirmLabel, cancelLabel, onConfirm, onCancel)
    }
}

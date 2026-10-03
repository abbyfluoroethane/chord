package space.foid.chord.ui.forms

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.autofill.ContentType
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentType
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.input.VisualTransformation
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordRadius
import space.foid.chord.ui.theme.ChordType

/** The text field of the sign-in, the registration and the data forms: Material plumbing in the Chord tokens. */
@Composable
fun ChordTextField(
    value: String,
    onChange: (String) -> Unit,
    label: String,
    tag: String,
    enabled: Boolean,
    keyboardOptions: KeyboardOptions,
    modifier: Modifier = Modifier,
    placeholder: String? = null,
    contentType: ContentType? = null,
    visualTransformation: VisualTransformation = VisualTransformation.None,
    keyboardActions: KeyboardActions = KeyboardActions.Default,
    trailing: (@Composable () -> Unit)? = null,
    singleLine: Boolean = true,
    minLines: Int = 1,
    isError: Boolean = false,
    mono: Boolean = false,
) {
    val c = Chord.colors
    val textStyle: TextStyle =
        (if (mono) ChordType.body.copy(fontFamily = ChordType.mono) else ChordType.body).copy(color = c.ink)
    OutlinedTextField(
        value = value,
        onValueChange = onChange,
        enabled = enabled,
        singleLine = singleLine,
        minLines = minLines,
        isError = isError,
        label = { Text(label, style = ChordType.bodySmall) },
        placeholder = placeholder?.let { { Text(it, style = textStyle.copy(color = c.inkMuted)) } },
        trailingIcon = trailing,
        visualTransformation = visualTransformation,
        keyboardOptions = keyboardOptions,
        keyboardActions = keyboardActions,
        textStyle = textStyle,
        shape = RoundedCornerShape(ChordRadius.md),
        colors = OutlinedTextFieldDefaults.colors(
            focusedTextColor = c.ink,
            unfocusedTextColor = c.ink,
            disabledTextColor = c.inkMuted,
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
            errorCursorColor = c.danger,
            focusedPlaceholderColor = c.inkMuted,
            unfocusedPlaceholderColor = c.inkMuted,
            disabledPlaceholderColor = c.inkMuted,
        ),
        modifier = modifier
            .fillMaxWidth()
            .testTag(tag)
            .then(if (contentType != null) Modifier.semantics { this.contentType = contentType } else Modifier),
    )
}

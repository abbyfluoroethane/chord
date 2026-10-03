package space.foid.chord.ui.forms

import android.graphics.BitmapFactory
import android.util.Base64
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CheckboxDefaults
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp
import space.foid.chord.R
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordRadius
import space.foid.chord.ui.theme.ChordSpace
import space.foid.chord.ui.theme.ChordType
import uniffi.chord_ffi.DataField
import uniffi.chord_ffi.DataFieldKind
import uniffi.chord_ffi.DataForm

/**
 * A renderer for XEP-0004 data forms: registration, and later room configuration and ad-hoc
 * commands. It shows every field type. It is stateless: a change calls [onChange] with a new
 * form, and the caller keeps it. The caller sends the form back with `submission(form)`.
 *
 * @param showTitle show the title and the instructions of the form above the fields
 * @param showProblems show what is wrong under each field (turn it on after a failed send)
 * @param idPrefix keeps test tags apart when two forms are on a screen
 */
@Composable
fun DataFormView(
    form: DataForm,
    onChange: (DataForm) -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    showTitle: Boolean = true,
    showProblems: Boolean = false,
    idPrefix: String = "form",
) {
    Column(modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(ChordSpace.s3)) {
        if (showTitle) {
            form.title?.takeIf { it.isNotBlank() }?.let {
                Text(it, style = ChordType.name, color = Chord.colors.ink)
            }
            form.instructions?.takeIf { it.isNotBlank() }?.let {
                Text(it, style = ChordType.body, color = Chord.colors.inkMuted)
            }
        }
        form.fields.forEachIndexed { i, f ->
            val problem = if (showProblems) fieldProblem(f) else null
            val set = { values: List<String> -> onChange(form.withValues(i, values)) }
            FieldView(f, set, enabled, problem, "${idPrefix}_$i")
        }
    }
}

@Composable
private fun FieldView(f: DataField, set: (List<String>) -> Unit, enabled: Boolean, problem: String?, tag: String) {
    val c = Chord.colors
    when (f.kind) {
        // Data for the server. The user never sees it.
        DataFieldKind.HIDDEN -> Unit
        DataFieldKind.FIXED -> Text(f.values.joinToString("\n"), style = ChordType.name, color = c.ink)
        DataFieldKind.BOOLEAN -> Row(
            Modifier.fillMaxWidth().heightIn(min = 48.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(ChordSpace.s4),
        ) {
            Column(Modifier.weight(1f)) {
                Text(fieldName(f), style = ChordType.label, color = c.ink)
                f.desc?.takeIf { it.isNotBlank() }?.let { Text(it, style = ChordType.caption, color = c.inkMuted) }
            }
            Switch(
                checked = isOn(f),
                onCheckedChange = { set(boolValue(it)) },
                enabled = enabled,
                colors = SwitchDefaults.colors(
                    checkedThumbColor = c.onBrand,
                    checkedTrackColor = c.brand,
                    checkedBorderColor = c.brand,
                    uncheckedThumbColor = c.inkMuted,
                    uncheckedTrackColor = c.surface300,
                    uncheckedBorderColor = c.lineStrong,
                ),
                modifier = Modifier.testTag(tag),
            )
        }
        else -> Column(verticalArrangement = Arrangement.spacedBy(ChordSpace.s1)) {
            when (f.kind) {
                DataFieldKind.LIST_MULTI -> ListMulti(f, set, enabled, tag)
                DataFieldKind.LIST_SINGLE -> ListSingle(f, set, enabled, problem != null, tag)
                DataFieldKind.TEXT_MULTI, DataFieldKind.JID_MULTI -> ChordTextField(
                    value = toMultiline(f.values),
                    onChange = { set(fromMultiline(it)) },
                    label = fieldName(f) + if (f.required) " *" else "",
                    tag = tag,
                    enabled = enabled,
                    singleLine = false,
                    minLines = 3,
                    isError = problem != null,
                    mono = f.kind == DataFieldKind.JID_MULTI,
                    keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false),
                )
                else -> {
                    val secret = f.kind == DataFieldKind.TEXT_PRIVATE
                    ChordTextField(
                        value = firstValue(f),
                        onChange = { set(listOf(it)) },
                        label = fieldName(f) + if (f.required) " *" else "",
                        tag = tag,
                        enabled = enabled,
                        isError = problem != null,
                        mono = f.kind == DataFieldKind.JID_SINGLE,
                        visualTransformation = if (secret) PasswordVisualTransformation() else VisualTransformation.None,
                        keyboardOptions = KeyboardOptions(
                            capitalization = KeyboardCapitalization.None,
                            autoCorrectEnabled = false,
                            keyboardType = if (secret) KeyboardType.Password else KeyboardType.Text,
                        ),
                    )
                }
            }
            f.media.forEach { m ->
                val uri = inlineImage(m)
                if (uri != null) FormImage(uri, "Image for ${fieldName(f)}", tag)
            }
            if (otherMedia(f).isNotEmpty()) {
                Text(stringResource(R.string.form_media_not_loaded), style = ChordType.caption, color = c.inkMuted)
            }
            if (f.kind == DataFieldKind.JID_MULTI) {
                Text(stringResource(R.string.form_one_address_per_line), style = ChordType.caption, color = c.inkMuted)
            }
            f.desc?.takeIf { it.isNotBlank() }?.let { Text(it, style = ChordType.caption, color = c.inkMuted) }
            if (problem != null) {
                Text(problem, style = ChordType.caption, color = c.danger, modifier = Modifier.testTag("${tag}_problem"))
            }
        }
    }
}

@Composable
private fun ListMulti(f: DataField, set: (List<String>) -> Unit, enabled: Boolean, tag: String) {
    val c = Chord.colors
    Text(fieldName(f) + if (f.required) " *" else "", style = ChordType.label, color = c.ink)
    f.options.forEachIndexed { j, o ->
        val on = o.value in f.values
        Row(
            Modifier
                .fillMaxWidth()
                .heightIn(min = 48.dp)
                .clickable(enabled = enabled, role = Role.Checkbox) { set(toggleValue(f, o.value, !on)) }
                .testTag("${tag}_$j"),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Checkbox(
                checked = on,
                onCheckedChange = null,
                enabled = enabled,
                colors = CheckboxDefaults.colors(
                    checkedColor = c.brand,
                    checkmarkColor = c.onBrand,
                    uncheckedColor = c.lineStrong,
                ),
            )
            Text(optionText(o), style = ChordType.body, color = c.ink, modifier = Modifier.padding(start = ChordSpace.s3))
        }
    }
}

@Composable
private fun ListSingle(f: DataField, set: (List<String>) -> Unit, enabled: Boolean, bad: Boolean, tag: String) {
    val c = Chord.colors
    var open by remember { mutableStateOf(false) }
    val selected = f.options.firstOrNull { it.value == selectedOption(f) }
    Text(fieldName(f) + if (f.required) " *" else "", style = ChordType.label, color = c.ink)
    Box {
        Row(
            Modifier
                .fillMaxWidth()
                .heightIn(min = 52.dp)
                .background(c.surface200, RoundedCornerShape(ChordRadius.md))
                .border(1.dp, if (bad) c.danger else c.lineStrong, RoundedCornerShape(ChordRadius.md))
                .clickable(enabled = enabled, role = Role.DropdownList) { open = true }
                .padding(horizontal = ChordSpace.s4)
                .testTag(tag),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                selected?.let { optionText(it) } ?: stringResource(R.string.form_choose),
                style = ChordType.body,
                color = if (selected != null) c.ink else c.inkMuted,
                modifier = Modifier.weight(1f),
            )
            Text("▾", style = ChordType.body, color = c.inkMuted)
        }
        DropdownMenu(expanded = open, onDismissRequest = { open = false }, containerColor = c.surface300) {
            f.options.forEach { o ->
                DropdownMenuItem(
                    text = { Text(optionText(o), style = ChordType.body, color = c.ink) },
                    onClick = {
                        open = false
                        set(listOf(o.value))
                    },
                    modifier = Modifier.widthIn(min = 200.dp),
                )
            }
        }
    }
}

/** An inline image of a field, for example a CAPTCHA. On a white plate so it reads in the dark theme. */
@Composable
private fun FormImage(uri: String, description: String, tag: String) {
    val bitmap = remember(uri) {
        runCatching {
            val bytes = Base64.decode(uri.substringAfter("base64,"), Base64.DEFAULT)
            BitmapFactory.decodeByteArray(bytes, 0, bytes.size)?.asImageBitmap()
        }.getOrNull()
    }
    if (bitmap == null) {
        Text(stringResource(R.string.form_media_not_loaded), style = ChordType.caption, color = Chord.colors.inkMuted)
        return
    }
    Image(
        bitmap = bitmap,
        contentDescription = description,
        modifier = Modifier
            .background(Color.White, RoundedCornerShape(ChordRadius.md))
            .padding(ChordSpace.s2)
            .testTag("${tag}_image"),
    )
}

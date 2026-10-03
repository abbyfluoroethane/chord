package space.foid.chord.ui.settings

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.em
import space.foid.chord.R
import space.foid.chord.ui.theme.Chord
import space.foid.chord.ui.theme.ChordRadius
import space.foid.chord.ui.theme.ChordSpace
import space.foid.chord.ui.theme.ChordType

// The small pieces that the settings pages share. They use the Chord tokens, not the stock look.

@Composable
internal fun SettingsTopBar(title: String, onBack: () -> Unit) {
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
        Text(title, style = ChordType.title, color = c.ink, modifier = Modifier.testTag("settings_page_title"))
    }
}

@Composable
internal fun ErrorNote(text: String) {
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

/** The small caps title over a group. */
@Composable
internal fun GroupTitle(title: String) {
    Text(
        title.uppercase(),
        style = ChordType.caption.copy(fontWeight = FontWeight.Bold, letterSpacing = 0.06.em),
        color = Chord.colors.inkMuted,
        modifier = Modifier.padding(start = ChordSpace.s1, top = ChordSpace.s4, bottom = ChordSpace.s1),
    )
}

/** A raised surface for rows. Pass [title] for a caps title over it. */
@Composable
internal fun Group(title: String? = null, content: @Composable () -> Unit) {
    if (title != null) GroupTitle(title)
    Column(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(ChordRadius.md))
            .background(Chord.colors.surface200),
    ) { content() }
}

/** A padded block inside a [Group], for fields and notes. */
@Composable
internal fun Block(content: @Composable () -> Unit) {
    Column(
        Modifier.fillMaxWidth().padding(ChordSpace.s3),
        verticalArrangement = Arrangement.spacedBy(ChordSpace.s2),
    ) { content() }
}

@Composable
internal fun RowDivider() {
    Box(Modifier.fillMaxWidth().padding(start = ChordSpace.s3).height(1.dp).background(Chord.colors.line))
}

@Composable
internal fun Label(text: String) = Text(text, style = ChordType.label, color = Chord.colors.ink)

@Composable
internal fun Hint(text: String, modifier: Modifier = Modifier) =
    Text(text, style = ChordType.caption, color = Chord.colors.inkMuted, modifier = modifier)

/** A row that opens a page or a dialog. [value] shows at the right, in muted ink. */
@Composable
internal fun NavRow(
    label: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    value: String? = null,
    danger: Boolean = false,
    chevron: Boolean = true,
) {
    val c = Chord.colors
    Row(
        modifier
            .fillMaxWidth()
            .heightIn(min = 52.dp)
            .clickable(role = Role.Button, onClick = onClick)
            .padding(horizontal = ChordSpace.s3),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(ChordSpace.s3),
    ) {
        Text(label, style = ChordType.body, color = if (danger) c.danger else c.ink, modifier = Modifier.weight(1f))
        if (value != null) {
            Text(value, style = ChordType.bodySmall, color = c.inkMuted, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        if (chevron) Chevron(if (danger) c.danger else c.inkMuted)
    }
}

@Composable
internal fun Chevron(color: Color) {
    Canvas(Modifier.size(width = 8.dp, height = 14.dp)) {
        val w = 2.dp.toPx()
        drawLine(color, Offset(1.dp.toPx(), 1.dp.toPx()), Offset(size.width - 1.dp.toPx(), size.height / 2), w, StrokeCap.Round)
        drawLine(color, Offset(size.width - 1.dp.toPx(), size.height / 2), Offset(1.dp.toPx(), size.height - 1.dp.toPx()), w, StrokeCap.Round)
    }
}

/** A row with a label, an optional hint and a switch. */
@Composable
internal fun ToggleRow(
    label: String,
    checked: Boolean,
    onChange: (Boolean) -> Unit,
    tag: String,
    hint: String? = null,
) {
    val c = Chord.colors
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 52.dp)
            .clickable(role = Role.Switch) { onChange(!checked) }
            .padding(horizontal = ChordSpace.s3, vertical = ChordSpace.s2),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(ChordSpace.s3),
    ) {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(label, style = ChordType.body, color = c.ink)
            if (hint != null) Hint(hint)
        }
        Switch(
            checked = checked,
            onCheckedChange = onChange,
            colors = SwitchDefaults.colors(
                checkedTrackColor = c.brand, checkedThumbColor = c.onBrand,
                uncheckedTrackColor = c.surface300, uncheckedThumbColor = c.inkMuted, uncheckedBorderColor = c.lineStrong,
            ),
            modifier = Modifier.testTag(tag),
        )
    }
}

@Composable
internal fun TextInput(
    value: String,
    onValueChange: (String) -> Unit,
    hint: String,
    onDone: () -> Unit,
    modifier: Modifier = Modifier,
    password: Boolean = false,
    imeAction: ImeAction = ImeAction.Done,
) {
    val c = Chord.colors
    BasicTextField(
        value = value,
        onValueChange = onValueChange,
        singleLine = true,
        textStyle = ChordType.body.copy(color = c.ink),
        cursorBrush = SolidColor(c.brand),
        visualTransformation = if (password) PasswordVisualTransformation() else VisualTransformation.None,
        keyboardOptions = KeyboardOptions(
            imeAction = imeAction,
            keyboardType = if (password) KeyboardType.Password else KeyboardType.Text,
        ),
        keyboardActions = KeyboardActions(onDone = { onDone() }, onNext = { onDone() }),
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
internal fun SmallButton(
    text: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    danger: Boolean = false,
    primary: Boolean = false,
    enabled: Boolean = true,
) {
    val c = Chord.colors
    val bg = when {
        primary -> c.brand
        danger -> c.danger.copy(alpha = 0.16f)
        else -> c.surface300
    }
    val fg = when {
        primary -> c.onBrand
        danger -> c.danger
        else -> c.ink
    }
    Box(
        modifier
            .heightIn(min = 40.dp)
            .clip(RoundedCornerShape(ChordRadius.md))
            .background(bg)
            .clickable(enabled = enabled, role = Role.Button, onClick = onClick)
            .padding(horizontal = ChordSpace.s4),
        contentAlignment = Alignment.Center,
    ) {
        Text(text, style = ChordType.label, color = if (enabled) fg else fg.copy(alpha = 0.5f))
    }
}

/** A row of choices in one group. Colour and a filled ring both mark the chosen one. */
@Composable
internal fun <T> Choices(
    options: List<Pair<T, String>>,
    selected: T,
    onSelect: (T) -> Unit,
    tag: String,
) {
    val c = Chord.colors
    Column {
        options.forEach { (value, label) ->
            val on = value == selected
            Row(
                Modifier
                    .fillMaxWidth()
                    .heightIn(min = 48.dp)
                    .background(if (on) c.brandSoft else Color.Transparent)
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
                        .border(2.dp, if (on) c.brand else c.lineStrong, CircleShape)
                        .padding(4.dp)
                        .clip(CircleShape)
                        .background(if (on) c.brand else Color.Transparent),
                )
                Text(label, style = ChordType.body, color = if (on) c.brandInk else c.ink)
            }
        }
    }
}

/** Two or three tabs in one pill, like the Profile / Account control of the desktop. */
@Composable
internal fun <T> Segmented(
    options: List<Pair<T, String>>,
    selected: T,
    onSelect: (T) -> Unit,
    tag: String,
) {
    val c = Chord.colors
    Row(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(ChordRadius.md))
            .background(c.surface200)
            .padding(3.dp),
        horizontalArrangement = Arrangement.spacedBy(3.dp),
    ) {
        options.forEach { (value, label) ->
            val on = value == selected
            Box(
                Modifier
                    .weight(1f)
                    .heightIn(min = 40.dp)
                    .clip(RoundedCornerShape(ChordRadius.sm + 2.dp))
                    .background(if (on) c.surface300 else Color.Transparent)
                    .clickable(role = Role.Tab) { onSelect(value) }
                    .semantics { this.selected = on }
                    .testTag("${tag}_$label"),
                contentAlignment = Alignment.Center,
            ) {
                Text(label, style = ChordType.label, color = if (on) c.ink else c.inkMuted)
            }
        }
    }
}

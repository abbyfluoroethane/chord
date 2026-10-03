package space.foid.chord.ui.forms

import uniffi.chord_ffi.DataField
import uniffi.chord_ffi.DataFieldKind
import uniffi.chord_ffi.DataForm
import uniffi.chord_ffi.DataFormKind
import uniffi.chord_ffi.DataMedia
import uniffi.chord_ffi.DataOption

fun field(
    kind: DataFieldKind,
    name: String?,
    label: String? = null,
    values: List<String> = emptyList(),
    required: Boolean = false,
    desc: String? = null,
    options: List<DataOption> = emptyList(),
    media: List<DataMedia> = emptyList(),
) = DataField(name, kind, label, desc, required, values, options, media)

fun form(vararg fields: DataField, title: String? = null, instructions: String? = null) =
    DataForm(DataFormKind.FORM, title, instructions, fields.toList())

/** A 1x1 PNG, as the core would give a `cid:` image: a `data:` URI. */
const val TINY_PNG =
    "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg=="

/** A registration form like the one of ejabberd with a CAPTCHA. */
fun registrationFormWithCaptcha() = form(
    field(DataFieldKind.HIDDEN, "FORM_TYPE", values = listOf("jabber:iq:register")),
    field(DataFieldKind.TEXT_SINGLE, "username", "User", required = true),
    field(DataFieldKind.TEXT_PRIVATE, "password", "Password", required = true),
    field(
        DataFieldKind.TEXT_SINGLE, "ocr", "Enter the text you see", required = true,
        desc = "We have to know you are not a robot.",
        media = listOf(DataMedia(TINY_PNG, "image/png", 1u, 1u)),
    ),
    title = "Create your account",
    instructions = "Choose a username and a password.",
)

/** One field of each type, for the renderer tests. */
fun allKindsForm() = form(
    field(DataFieldKind.FIXED, null, values = listOf("Room settings")),
    field(DataFieldKind.TEXT_SINGLE, "name", "Room name", values = listOf("Launch Ops"), required = true),
    field(DataFieldKind.TEXT_MULTI, "desc", "Description", values = listOf("First line", "Second line")),
    field(DataFieldKind.TEXT_PRIVATE, "pw", "Password", values = listOf("secret")),
    field(DataFieldKind.BOOLEAN, "persist", "Keep the room", values = listOf("1"), desc = "The room stays when empty."),
    field(DataFieldKind.BOOLEAN, "moderated", "Moderated", values = listOf("0")),
    field(
        DataFieldKind.LIST_SINGLE, "level", "Who may talk",
        values = listOf("members"),
        options = listOf(DataOption("Everyone", "all"), DataOption("Members", "members")),
    ),
    field(
        DataFieldKind.LIST_MULTI, "roles", "Roles that can see addresses",
        values = listOf("moderator"),
        options = listOf(DataOption("Moderators", "moderator"), DataOption("Admins", "admin")),
    ),
    field(DataFieldKind.JID_MULTI, "owners", "Owners", values = listOf("rin@foid.space")),
    field(DataFieldKind.JID_SINGLE, "contact", "Contact", values = listOf("bay@foid.space")),
    title = "Configure the room",
    instructions = "Change what you need.",
)

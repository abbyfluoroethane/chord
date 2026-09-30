//! A generic data form (XEP-0004), for the forms of room configuration (XEP-0045),
//! ad-hoc commands (XEP-0050) and in-band registration (XEP-0077).
//!
//! `Form` is the plain model that a UI shows and fills in. `Form::from_element` reads it
//! from XML and `Form::submission` writes the answer. The reader is lenient: servers send
//! forms that the strict `xmpp_parsers::data_forms::DataForm` refuses (a `reported` table,
//! a field with no `var`). `From<DataForm>` converts the strict type, for code that has it
//! already.

use jid::Jid;
use xmpp_parsers::data_forms::{DataForm, DataFormType, Field, FieldType};
use xmpp_parsers::minidom::Element;
use xmpp_parsers::minidom::rxml::NcName;

pub(crate) const NS_DATA: &str = "jabber:x:data";
const NS_MEDIA: &str = "urn:xmpp:media-element";
const NS_BOB: &str = "urn:xmpp:bob";

/// The type of a field (XEP-0004, 3.3). The serde names are the names in the XEP.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "kebab-case")
)]
pub enum FieldKind {
    Boolean,
    Fixed,
    Hidden,
    JidMulti,
    JidSingle,
    ListMulti,
    ListSingle,
    TextMulti,
    TextPrivate,
    /// The default when a field has no `type`.
    #[default]
    TextSingle,
}

impl FieldKind {
    fn parse(s: &str) -> Self {
        match s {
            "boolean" => Self::Boolean,
            "fixed" => Self::Fixed,
            "hidden" => Self::Hidden,
            "jid-multi" => Self::JidMulti,
            "jid-single" => Self::JidSingle,
            "list-multi" => Self::ListMulti,
            "list-single" => Self::ListSingle,
            "text-multi" => Self::TextMulti,
            "text-private" => Self::TextPrivate,
            // An unknown type is a text field (XEP-0004, 3.3).
            _ => Self::TextSingle,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Boolean => "boolean",
            Self::Fixed => "fixed",
            Self::Hidden => "hidden",
            Self::JidMulti => "jid-multi",
            Self::JidSingle => "jid-single",
            Self::ListMulti => "list-multi",
            Self::ListSingle => "list-single",
            Self::TextMulti => "text-multi",
            Self::TextPrivate => "text-private",
            Self::TextSingle => "text-single",
        }
    }
}

impl From<FieldType> for FieldKind {
    fn from(t: FieldType) -> Self {
        match t {
            FieldType::Boolean => Self::Boolean,
            FieldType::Fixed => Self::Fixed,
            FieldType::Hidden => Self::Hidden,
            FieldType::JidMulti => Self::JidMulti,
            FieldType::JidSingle => Self::JidSingle,
            FieldType::ListMulti => Self::ListMulti,
            FieldType::ListSingle => Self::ListSingle,
            FieldType::TextMulti => Self::TextMulti,
            FieldType::TextPrivate => Self::TextPrivate,
            FieldType::TextSingle => Self::TextSingle,
        }
    }
}

/// The type of a whole form (XEP-0004, 3.1).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase")
)]
pub enum FormKind {
    /// A form to fill in.
    #[default]
    Form,
    Submit,
    Cancel,
    /// Data, not to fill in.
    Result,
}

impl FormKind {
    fn parse(s: Option<&str>) -> Self {
        match s {
            Some("submit") => Self::Submit,
            Some("cancel") => Self::Cancel,
            Some("result") => Self::Result,
            _ => Self::Form,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Form => "form",
            Self::Submit => "submit",
            Self::Cancel => "cancel",
            Self::Result => "result",
        }
    }
}

/// One choice of a list field.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase")
)]
pub struct FormOption {
    pub label: Option<String>,
    pub value: String,
}

/// A media element of a field (XEP-0221), for example a CAPTCHA image.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase")
)]
pub struct FormMedia {
    /// The URI. A `cid:` URI that the stanza has data for (XEP-0231) becomes a `data:` URI.
    pub uri: String,
    pub mime: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase")
)]
pub struct FormField {
    /// Only a `fixed` field may have no name.
    pub var: Option<String>,
    pub kind: FieldKind,
    pub label: Option<String>,
    pub desc: Option<String>,
    pub required: bool,
    /// The values. A boolean is "1" or "0" after `normalized`, and a list has the chosen
    /// option values.
    pub values: Vec<String>,
    pub options: Vec<FormOption>,
    pub media: Vec<FormMedia>,
}

impl FormField {
    /// The first value, or an empty string.
    pub fn value(&self) -> &str {
        self.values.first().map_or("", String::as_str)
    }

    /// True for a boolean field that is on ("1" or "true", XEP-0004, 3.3).
    pub fn is_on(&self) -> bool {
        matches!(self.value().trim(), "1" | "true")
    }
}

/// A data form.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase")
)]
pub struct Form {
    pub kind: FormKind,
    pub title: Option<String>,
    pub instructions: Option<String>,
    pub fields: Vec<FormField>,
}

fn nc(name: &'static str) -> NcName {
    NcName::try_from(name).expect("a valid attribute name")
}

fn text_of(element: &Element, name: &str) -> Option<String> {
    element
        .get_child(name, NS_DATA)
        .map(Element::text)
        .filter(|t| !t.is_empty())
}

impl Form {
    /// Read a form from its `<x xmlns='jabber:x:data'/>` element. Fails only when the
    /// element is not a data form.
    pub fn from_element(x: &Element) -> Result<Self, String> {
        if !x.is("x", NS_DATA) {
            return Err(format!("not a data form: <{} xmlns='{}'>", x.name(), x.ns()));
        }
        let instructions: Vec<String> = x
            .children()
            .filter(|c| c.is("instructions", NS_DATA))
            .map(Element::text)
            .filter(|t| !t.is_empty())
            .collect();
        let fields = x
            .children()
            .filter(|c| c.is("field", NS_DATA))
            .map(field_from_element)
            // A field with no `var` that is not `fixed` cannot be answered. Skip it.
            .filter(|f| f.var.is_some() || f.kind == FieldKind::Fixed)
            .collect();
        Ok(Self {
            kind: FormKind::parse(x.attr("type")),
            title: text_of(x, "title"),
            instructions: (!instructions.is_empty()).then(|| instructions.join("\n")),
            fields,
        })
    }

    /// The first data form in `parent`, or `None` when it has none or it is not valid.
    pub fn in_element(parent: &Element) -> Option<Self> {
        let mut form = parent
            .get_child("x", NS_DATA)
            .and_then(|x| Self::from_element(x).ok())?;
        form.resolve_bob(parent);
        Some(form)
    }

    /// The value of `FORM_TYPE`, if the form has it.
    pub fn form_type(&self) -> Option<&str> {
        self.get("FORM_TYPE").map(FormField::value)
    }

    pub fn get(&self, var: &str) -> Option<&FormField> {
        self.fields.iter().find(|f| f.var.as_deref() == Some(var))
    }

    pub fn get_mut(&mut self, var: &str) -> Option<&mut FormField> {
        self.fields
            .iter_mut()
            .find(|f| f.var.as_deref() == Some(var))
    }

    /// Set the values of a field. Returns false when the form has no such field.
    pub fn set(&mut self, var: &str, values: Vec<String>) -> bool {
        match self.get_mut(var) {
            Some(field) => {
                field.values = values;
                true
            }
            None => false,
        }
    }

    /// Replace each `cid:` media URI by a `data:` URI when `parent` has the BoB data
    /// (XEP-0231) for it. A CAPTCHA form (XEP-0158) sends its image this way.
    pub fn resolve_bob(&mut self, parent: &Element) {
        for field in &mut self.fields {
            for media in &mut field.media {
                let Some(cid) = media.uri.strip_prefix("cid:") else {
                    continue;
                };
                let data = parent
                    .children()
                    .find(|d| d.is("data", NS_BOB) && d.attr("cid") == Some(cid));
                if let Some(data) = data {
                    let mime = data.attr("type").unwrap_or("image/png");
                    let base64: String = data.text().split_whitespace().collect();
                    media.mime.get_or_insert_with(|| mime.to_owned());
                    media.uri = format!("data:{mime};base64,{base64}");
                }
            }
        }
    }

    /// The labels of the required fields that have no value (a required fixed or hidden
    /// field needs none), and the labels of the jid fields with an invalid JID.
    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        for field in &self.fields {
            let name = field
                .label
                .as_deref()
                .or(field.var.as_deref())
                .unwrap_or_default();
            if matches!(field.kind, FieldKind::Fixed | FieldKind::Hidden) {
                continue;
            }
            let filled = field.values.iter().any(|v| !v.trim().is_empty());
            if field.required && field.kind != FieldKind::Boolean && !filled {
                out.push(format!("{name} is required"));
                continue;
            }
            if matches!(field.kind, FieldKind::JidSingle | FieldKind::JidMulti) {
                for value in field.values.iter().filter(|v| !v.trim().is_empty()) {
                    if value.trim().parse::<Jid>().is_err() {
                        out.push(format!("{name}: {value} is not a valid address"));
                    }
                }
            }
        }
        out
    }

    /// The submit element `<x type='submit'/>` for this form. A fixed field has no answer
    /// and stays out. A boolean goes out as "1" or "0". A text field with no value sends
    /// one empty value, so that a cleared field clears the setting.
    pub fn submission(&self) -> Element {
        let mut x = Element::builder("x", NS_DATA).attr(nc("type"), "submit");
        for field in &self.fields {
            let Some(var) = field.var.as_deref() else {
                continue;
            };
            if field.kind == FieldKind::Fixed {
                continue;
            }
            let mut out = Element::builder("field", NS_DATA)
                .attr(nc("var"), var)
                .attr(nc("type"), field.kind.as_str());
            let values: Vec<String> = match field.kind {
                FieldKind::Boolean => vec![if field.is_on() { "1" } else { "0" }.to_owned()],
                FieldKind::TextSingle | FieldKind::TextPrivate | FieldKind::JidSingle
                    if field.values.is_empty() =>
                {
                    vec![String::new()]
                }
                _ => field.values.clone(),
            };
            for value in values {
                out = out.append(Element::builder("value", NS_DATA).append(value).build());
            }
            x = x.append(out.build());
        }
        x.build()
    }

    /// The `<x type='cancel'/>` element, for a form that the user leaves.
    pub fn cancellation() -> Element {
        Element::builder("x", NS_DATA)
            .attr(nc("type"), FormKind::Cancel.as_str())
            .build()
    }

    /// A form with only text values: the `var` and the first value of each field. This is
    /// the shape of a result form that a command returns.
    pub fn pairs(&self) -> Vec<(String, String)> {
        self.fields
            .iter()
            .filter_map(|f| Some((f.var.clone()?, f.value().to_owned())))
            .collect()
    }
}

fn field_from_element(field: &Element) -> FormField {
    FormField {
        var: field.attr("var").map(str::to_owned),
        kind: field.attr("type").map_or(FieldKind::TextSingle, FieldKind::parse),
        label: field.attr("label").map(str::to_owned),
        desc: text_of(field, "desc"),
        required: field.get_child("required", NS_DATA).is_some(),
        values: field
            .children()
            .filter(|c| c.is("value", NS_DATA))
            .map(Element::text)
            .collect(),
        options: field
            .children()
            .filter(|c| c.is("option", NS_DATA))
            .map(|o| FormOption {
                label: o.attr("label").map(str::to_owned),
                value: o
                    .get_child("value", NS_DATA)
                    .map(Element::text)
                    .unwrap_or_default(),
            })
            .collect(),
        media: field
            .children()
            .filter(|c| c.is("media", NS_MEDIA))
            .flat_map(|m| {
                let number = |name| m.attr(name).and_then(|v| v.parse::<u32>().ok());
                let (width, height) = (number("width"), number("height"));
                m.children()
                    .filter(|u| u.is("uri", NS_MEDIA))
                    .map(move |u| FormMedia {
                        uri: u.text().trim().to_owned(),
                        mime: u.attr("type").map(str::to_owned),
                        width,
                        height,
                    })
            })
            .collect(),
    }
}

impl From<&DataForm> for Form {
    fn from(form: &DataForm) -> Self {
        Self {
            kind: match form.type_ {
                DataFormType::Form => FormKind::Form,
                DataFormType::Submit => FormKind::Submit,
                DataFormType::Cancel => FormKind::Cancel,
                DataFormType::Result_ => FormKind::Result,
            },
            title: form.title.clone(),
            instructions: form.instructions.clone(),
            fields: form.fields.iter().map(FormField::from).collect(),
        }
    }
}

impl From<&Field> for FormField {
    fn from(field: &Field) -> Self {
        Self {
            var: field.var.clone(),
            kind: field.type_.clone().into(),
            label: field.label.clone(),
            desc: field.desc.clone(),
            required: field.required,
            values: field.values.clone(),
            options: field
                .options
                .iter()
                .map(|o| FormOption {
                    label: o.label.clone(),
                    value: o.value.clone(),
                })
                .collect(),
            media: field
                .media
                .iter()
                .flat_map(|m| {
                    m.uris.iter().map(|u| FormMedia {
                        uri: u.uri.clone(),
                        mime: Some(u.type_.clone()),
                        width: m.width.and_then(|w| u32::try_from(w).ok()),
                        height: m.height.and_then(|h| u32::try_from(h).ok()),
                    })
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn el(xml: &str) -> Element {
        xml.parse().unwrap()
    }

    const FORM: &str = "<x xmlns='jabber:x:data' type='form'>\
        <title>Configuration</title>\
        <instructions>Fill it in.</instructions><instructions>Then send it.</instructions>\
        <field var='FORM_TYPE' type='hidden'><value>http://jabber.org/protocol/muc#roomconfig</value></field>\
        <field type='fixed'><value>Section 1</value></field>\
        <field var='name' type='text-single' label='Name'><required/><desc>The room name</desc><value>Lobby</value></field>\
        <field var='secret' type='text-private' label='Password'/>\
        <field var='public' type='boolean' label='Public'><value>true</value></field>\
        <field var='who' type='list-single' label='Who sees JIDs'>\
        <value>moderators</value>\
        <option label='Moderators'><value>moderators</value></option>\
        <option label='Anyone'><value>anyone</value></option></field>\
        <field var='tags' type='list-multi'><option><value>a</value></option><option><value>b</value></option><value>a</value><value>b</value></field>\
        <field var='owners' type='jid-multi' label='Owners'><value>a@example.org</value></field>\
        <field var='bio' type='text-multi'><value>one</value><value>two</value></field>\
        <field var='odd' type='weird'/>\
        <field label='no var'/>\
        <reported><field var='x'/></reported>\
        </x>";

    #[test]
    fn reads_every_field_type() {
        let form = Form::from_element(&el(FORM)).unwrap();
        assert_eq!(form.kind, FormKind::Form);
        assert_eq!(form.title.as_deref(), Some("Configuration"));
        assert_eq!(form.instructions.as_deref(), Some("Fill it in.\nThen send it."));
        assert_eq!(form.form_type(), Some("http://jabber.org/protocol/muc#roomconfig"));
        let kinds: Vec<_> = form.fields.iter().map(|f| f.kind).collect();
        assert_eq!(
            kinds,
            vec![
                FieldKind::Hidden,
                FieldKind::Fixed,
                FieldKind::TextSingle,
                FieldKind::TextPrivate,
                FieldKind::Boolean,
                FieldKind::ListSingle,
                FieldKind::ListMulti,
                FieldKind::JidMulti,
                FieldKind::TextMulti,
                // An unknown type reads as text-single. The field with no var is gone.
                FieldKind::TextSingle,
            ]
        );
        let name = form.get("name").unwrap();
        assert!(name.required);
        assert_eq!(name.desc.as_deref(), Some("The room name"));
        assert_eq!(name.value(), "Lobby");
        let who = form.get("who").unwrap();
        assert_eq!(who.options.len(), 2);
        assert_eq!(who.options[1].label.as_deref(), Some("Anyone"));
        assert_eq!(form.get("bio").unwrap().values, vec!["one", "two"]);
        assert!(form.get("public").unwrap().is_on());
    }

    #[test]
    fn the_strict_type_converts_to_the_same_model() {
        let x = el(FORM);
        // The strict parser refuses this form (`reported`, a field with no var).
        assert!(DataForm::try_from(x.clone()).is_err());
        let strict = el("<x xmlns='jabber:x:data' type='form'>\
            <field var='a' type='list-single' label='A'><option label='One'><value>1</value></option><value>1</value></field>\
            <field var='m' type='text-single'><media xmlns='urn:xmpp:media-element' width='10'><uri type='image/png'>http://e/x.png</uri></media></field></x>");
        let data = DataForm::try_from(strict.clone()).unwrap();
        assert_eq!(Form::from(&data), Form::from_element(&strict).unwrap());
    }

    #[test]
    fn submission_writes_the_answers() {
        let mut form = Form::from_element(&el(FORM)).unwrap();
        form.set("name", vec!["New".into()]);
        form.set("public", vec!["false".into()]);
        form.set("secret", vec![]);
        let x = form.submission();
        assert_eq!(x.attr("type"), Some("submit"));
        let by_var = |var: &str| {
            x.children()
                .find(|f| f.attr("var") == Some(var))
                .unwrap_or_else(|| panic!("no field {var}"))
        };
        let values = |var: &str| -> Vec<String> { by_var(var).children().map(Element::text).collect() };
        assert_eq!(values("FORM_TYPE"), vec!["http://jabber.org/protocol/muc#roomconfig"]);
        assert_eq!(values("name"), vec!["New"]);
        assert_eq!(values("public"), vec!["0"]);
        // A cleared text field sends an empty value.
        assert_eq!(values("secret"), vec![""]);
        assert_eq!(values("tags"), vec!["a", "b"]);
        assert_eq!(values("bio"), vec!["one", "two"]);
        assert_eq!(by_var("public").attr("type"), Some("boolean"));
        // The fixed field has no var, so it is not in the answer.
        assert_eq!(x.children().count(), 9);
        // The answer parses as a strict submit form.
        assert!(DataForm::try_from(x).is_ok());
    }

    #[test]
    fn problems_name_the_missing_and_invalid_fields() {
        let mut form = Form::from_element(&el(FORM)).unwrap();
        assert!(form.problems().is_empty());
        form.set("name", vec!["  ".into()]);
        form.set("owners", vec!["a@example.org".into(), "not a jid@@".into()]);
        let problems = form.problems();
        assert_eq!(problems.len(), 2, "{problems:?}");
        assert!(problems[0].contains("Name is required"));
        assert!(problems[1].contains("not a valid address"));
    }

    #[test]
    fn a_cid_image_becomes_a_data_uri() {
        let reply = el("<query xmlns='jabber:iq:register'>\
            <x xmlns='jabber:x:data' type='form'>\
            <field var='ocr' type='text-single' label='Enter the text'><required/>\
            <media xmlns='urn:xmpp:media-element' height='60' width='150'>\
            <uri type='image/png'>cid:sha1+abc@bob.xmpp.org</uri></media></field></x>\
            <data xmlns='urn:xmpp:bob' cid='sha1+abc@bob.xmpp.org' type='image/png' max-age='0'>iVBO\nRw0K</data></query>");
        let form = Form::in_element(&reply).unwrap();
        let media = &form.fields[0].media[0];
        assert_eq!(media.uri, "data:image/png;base64,iVBORw0K");
        assert_eq!(media.mime.as_deref(), Some("image/png"));
        assert_eq!((media.width, media.height), (Some(150), Some(60)));
    }

    #[test]
    fn a_form_that_is_not_a_form_is_an_error() {
        assert!(Form::from_element(&el("<x xmlns='other'/>")).is_err());
        assert!(Form::in_element(&el("<query xmlns='q'/>")).is_none());
    }

    #[test]
    fn cancellation_is_a_cancel_form() {
        assert_eq!(Form::cancellation().attr("type"), Some("cancel"));
    }
}

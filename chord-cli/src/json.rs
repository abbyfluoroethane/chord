//! A small JSON writer for `--json` output. The CLI needs only objects of strings,
//! numbers, booleans, nulls, and nested objects, so it does not need a JSON crate.

use std::fmt::Write;

/// A JSON object under construction.
pub struct Obj(String);

impl Obj {
    pub fn new() -> Self {
        Self(String::from("{"))
    }

    fn key(&mut self, key: &str) {
        if self.0.len() > 1 {
            self.0.push(',');
        }
        push_str(&mut self.0, key);
        self.0.push(':');
    }

    pub fn str(mut self, key: &str, value: &str) -> Self {
        self.key(key);
        push_str(&mut self.0, value);
        self
    }

    pub fn opt_str(mut self, key: &str, value: Option<&str>) -> Self {
        self.key(key);
        match value {
            Some(v) => push_str(&mut self.0, v),
            None => self.0.push_str("null"),
        }
        self
    }

    pub fn num(mut self, key: &str, value: i64) -> Self {
        self.key(key);
        let _ = write!(self.0, "{value}");
        self
    }

    pub fn opt_num(mut self, key: &str, value: Option<i64>) -> Self {
        self.key(key);
        match value {
            Some(v) => {
                let _ = write!(self.0, "{v}");
            }
            None => self.0.push_str("null"),
        }
        self
    }

    pub fn bool(mut self, key: &str, value: bool) -> Self {
        self.key(key);
        self.0.push_str(if value { "true" } else { "false" });
        self
    }

    /// A value that is JSON already (an object or an array).
    pub fn raw(mut self, key: &str, json: &str) -> Self {
        self.key(key);
        self.0.push_str(json);
        self
    }

    pub fn finish(mut self) -> String {
        self.0.push('}');
        self.0
    }
}

/// A JSON array of values that are JSON already.
pub fn array(items: impl IntoIterator<Item = String>) -> String {
    let items: Vec<String> = items.into_iter().collect();
    format!("[{}]", items.join(","))
}

fn push_str(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_and_nests() {
        let inner = Obj::new().num("n", -3).finish();
        let json = Obj::new()
            .str("s", "a\"b\\c\nd\u{1}")
            .opt_str("none", None)
            .bool("t", true)
            .raw("inner", &inner)
            .raw("list", &array(vec!["1".into(), "2".into()]))
            .finish();
        assert_eq!(
            json,
            r#"{"s":"a\"b\\c\nd\u0001","none":null,"t":true,"inner":{"n":-3},"list":[1,2]}"#
        );
    }
}

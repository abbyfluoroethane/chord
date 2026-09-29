//! JSON arrays of strings, for list columns (reaction emojis, roster groups). The store
//! needs only this shape, so it does not need a JSON crate.

/// Write a JSON array of strings.
pub fn to_array(items: &[String]) -> String {
    let mut out = String::from("[");
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push('"');
        for c in item.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
                c => out.push(c),
            }
        }
        out.push('"');
    }
    out.push(']');
    out
}

/// Read a JSON array of strings. Returns an empty list for anything else.
pub fn from_array(json: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = json.trim().chars().peekable();
    if chars.next() != Some('[') {
        return out;
    }
    loop {
        match chars.find(|c| !c.is_whitespace()) {
            Some('"') => {}
            Some(']') | None => return out,
            Some(',') => continue,
            Some(_) => return Vec::new(),
        }
        let mut item = String::new();
        while let Some(c) = chars.next() {
            match c {
                '"' => break,
                '\\' => match chars.next() {
                    Some('n') => item.push('\n'),
                    Some('r') => item.push('\r'),
                    Some('t') => item.push('\t'),
                    Some('u') => {
                        let hex: String = chars.by_ref().take(4).collect();
                        if let Some(c) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32)
                        {
                            item.push(c);
                        }
                    }
                    Some(c) => item.push(c),
                    None => return Vec::new(),
                },
                c => item.push(c),
            }
        }
        out.push(item);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let items = vec![
            "👍".to_owned(),
            "a\"b\\c".to_owned(),
            "line\nbreak".to_owned(),
            String::new(),
        ];
        assert_eq!(from_array(&to_array(&items)), items);
        assert_eq!(from_array("[]"), Vec::<String>::new());
        assert_eq!(from_array(" [ \"x\" , \"y\" ] "), vec!["x", "y"]);
        assert_eq!(from_array("{}"), Vec::<String>::new());
    }
}

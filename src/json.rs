//! Minimal JSON parser (read-only).

#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

static NULL: Json = Json::Null;

impl Json {
    pub fn parse(s: &str) -> Option<Json> {
        let mut p = Parser { b: s.as_bytes(), i: 0, depth: 0 };
        let v = p.value()?;
        p.ws();
        (p.i == p.b.len()).then_some(v)
    }

    /// Value under the key; `Null` if missing or not an object.
    pub fn get(&self, key: &str) -> &Json {
        match self {
            Json::Obj(items) => items.iter().find(|(k, _)| k == key).map(|(_, v)| v).unwrap_or(&NULL),
            _ => &NULL,
        }
    }

    pub fn str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn f64(&self) -> Option<f64> {
        match self {
            Json::Num(n) => Some(*n),
            _ => None,
        }
    }

    pub fn bool(&self) -> Option<bool> {
        match self {
            Json::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn arr(&self) -> &[Json] {
        match self {
            Json::Arr(a) => a,
            _ => &[],
        }
    }

    pub fn obj(&self) -> &[(String, Json)] {
        match self {
            Json::Obj(o) => o,
            _ => &[],
        }
    }

    pub fn is_obj(&self) -> bool {
        matches!(self, Json::Obj(_))
    }

    /// Array of strings (skips non-string elements).
    pub fn strs(&self) -> impl Iterator<Item = &str> {
        self.arr().iter().filter_map(|v| v.str())
    }
}

struct Parser<'a> {
    b: &'a [u8],
    i: usize,
    depth: u32,
}

impl Parser<'_> {
    fn ws(&mut self) {
        while self.i < self.b.len() && matches!(self.b[self.i], b' ' | b'\t' | b'\r' | b'\n') {
            self.i += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.b.get(self.i).copied()
    }

    fn lit(&mut self, word: &[u8], v: Json) -> Option<Json> {
        if self.b[self.i..].starts_with(word) {
            self.i += word.len();
            Some(v)
        } else {
            None
        }
    }

    fn value(&mut self) -> Option<Json> {
        self.ws();
        match self.peek()? {
            b'{' => self.object(),
            b'[' => self.array(),
            b'"' => self.string().map(Json::Str),
            b't' => self.lit(b"true", Json::Bool(true)),
            b'f' => self.lit(b"false", Json::Bool(false)),
            b'n' => self.lit(b"null", Json::Null),
            b'N' => self.lit(b"NaN", Json::Null),
            b'-' | b'0'..=b'9' | b'I' => self.number(),
            _ => None,
        }
    }

    fn enter(&mut self) -> Option<()> {
        self.depth += 1;
        (self.depth <= 64).then_some(())
    }

    fn object(&mut self) -> Option<Json> {
        self.enter()?;
        self.i += 1;
        let mut items = Vec::new();
        self.ws();
        if self.peek()? == b'}' {
            self.i += 1;
            self.depth -= 1;
            return Some(Json::Obj(items));
        }
        loop {
            self.ws();
            if self.peek()? != b'"' {
                return None;
            }
            let k = self.string()?;
            self.ws();
            if self.peek()? != b':' {
                return None;
            }
            self.i += 1;
            let v = self.value()?;
            items.push((k, v));
            self.ws();
            match self.peek()? {
                b',' => self.i += 1,
                b'}' => {
                    self.i += 1;
                    self.depth -= 1;
                    return Some(Json::Obj(items));
                }
                _ => return None,
            }
        }
    }

    fn array(&mut self) -> Option<Json> {
        self.enter()?;
        self.i += 1;
        let mut items = Vec::new();
        self.ws();
        if self.peek()? == b']' {
            self.i += 1;
            self.depth -= 1;
            return Some(Json::Arr(items));
        }
        loop {
            items.push(self.value()?);
            self.ws();
            match self.peek()? {
                b',' => self.i += 1,
                b']' => {
                    self.i += 1;
                    self.depth -= 1;
                    return Some(Json::Arr(items));
                }
                _ => return None,
            }
        }
    }

    fn hex4(&mut self) -> Option<u32> {
        let s = std::str::from_utf8(self.b.get(self.i..self.i + 4)?).ok()?;
        self.i += 4;
        u32::from_str_radix(s, 16).ok()
    }

    fn string(&mut self) -> Option<String> {
        self.i += 1;
        let mut out = Vec::new();
        loop {
            let c = self.peek()?;
            self.i += 1;
            match c {
                b'"' => return String::from_utf8(out).ok(),
                b'\\' => {
                    let e = self.peek()?;
                    self.i += 1;
                    match e {
                        b'"' => out.push(b'"'),
                        b'\\' => out.push(b'\\'),
                        b'/' => out.push(b'/'),
                        b'b' => out.push(8),
                        b'f' => out.push(12),
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'u' => {
                            let mut cp = self.hex4()?;
                            if (0xD800..0xDC00).contains(&cp) && self.b[self.i..].starts_with(b"\\u") {
                                self.i += 2;
                                let lo = self.hex4()?;
                                if (0xDC00..0xE000).contains(&lo) {
                                    cp = 0x10000 + ((cp - 0xD800) << 10) + (lo - 0xDC00);
                                }
                            }
                            let ch = char::from_u32(cp).unwrap_or('\u{FFFD}');
                            let mut buf = [0u8; 4];
                            out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                        }
                        _ => return None,
                    }
                }
                _ => out.push(c),
            }
        }
    }

    fn number(&mut self) -> Option<Json> {
        let start = self.i;
        if self.b[self.i..].starts_with(b"-Infinity") || self.b[self.i..].starts_with(b"Infinity") {
            self.i += if self.b[self.i] == b'-' { 9 } else { 8 };
            return Some(Json::Null);
        }
        while self.i < self.b.len() && matches!(self.b[self.i], b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9') {
            self.i += 1;
        }
        let s = std::str::from_utf8(&self.b[start..self.i]).ok()?;
        s.parse::<f64>().ok().map(Json::Num)
    }
}

/// Quickly extracts the `"topic"` field value from the start of a Frigate
/// WebSocket message without parsing all of it (e.g. large `stats`).
pub fn peek_topic(msg: &str) -> Option<&str> {
    let b = msg.as_bytes();
    let head = &b[..b.len().min(96)];
    let pos = head.windows(7).position(|w| w == b"\"topic\"")? + 7;
    let mut i = pos;
    while i < b.len() && matches!(b[i], b' ' | b':' | b'\t') {
        i += 1;
    }
    if b.get(i) != Some(&b'"') {
        return None;
    }
    let start = i + 1;
    let end = start + b[start..].iter().position(|&c| c == b'"' || c == b'\\')?;
    msg.get(start..end)
}

/// A JSON string literal (with quotes) for `s`.
pub fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
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
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_strings() {
        let s = "a\"b\\c\nd\u{1}ż";
        assert_eq!(quote(s), "\"a\\\"b\\\\c\\nd\\u0001ż\"");
        assert_eq!(Json::parse(&quote(s)).and_then(|j| j.str().map(str::to_string)).as_deref(), Some(s));
    }

    #[test]
    fn parses_scalars_and_nesting() {
        let j = Json::parse(r#" {"a": 1.5, "b": [true, false, null], "c": {"d": "x"}, "e": -2e3} "#).unwrap();
        assert_eq!(j.get("a").f64(), Some(1.5));
        assert_eq!(j.get("b").arr().len(), 3);
        assert_eq!(j.get("b").arr()[0].bool(), Some(true));
        assert_eq!(j.get("c").get("d").str(), Some("x"));
        assert_eq!(j.get("e").f64(), Some(-2000.0));
        assert!(j.get("missing").get("deeper").str().is_none());
    }

    #[test]
    fn parses_string_escapes() {
        let j = Json::parse(r#""a\"b\\c\/\ną😀""#).unwrap();
        assert_eq!(j.str(), Some("a\"b\\c/\ną😀"));
        let raw = Json::parse("\"zażółć\"").unwrap();
        assert_eq!(raw.str(), Some("zażółć"));
    }

    #[test]
    fn rejects_garbage() {
        assert!(Json::parse("{").is_none());
        assert!(Json::parse("[1,]").is_none());
        assert!(Json::parse("{\"a\" 1}").is_none());
        assert!(Json::parse("1 2").is_none());
        assert!(Json::parse(&"[".repeat(100)).is_none());
    }

    #[test]
    fn python_specials_become_null() {
        let j = Json::parse(r#"{"a": NaN, "b": Infinity, "c": -Infinity}"#).unwrap();
        assert!(j.get("a").f64().is_none());
        assert!(j.get("c").f64().is_none());
    }

    #[test]
    fn peeks_topic_without_full_parse() {
        assert_eq!(peek_topic(r#"{"topic": "events", "payload": "{\"type\": \"new\"}"}"#), Some("events"));
        assert_eq!(peek_topic(r#"{"topic":"stats","payload":"..."#), Some("stats"));
        assert_eq!(peek_topic(r#"{"payload": 1}"#), None);
    }

    #[test]
    fn parses_double_encoded_frigate_message() {
        let msg = r#"{"topic": "events", "payload": "{\"type\": \"new\", \"after\": {\"camera\": \"brama\", \"label\": \"person\", \"current_zones\": []}}"}"#;
        let outer = Json::parse(msg).unwrap();
        let inner = Json::parse(outer.get("payload").str().unwrap()).unwrap();
        assert_eq!(inner.get("type").str(), Some("new"));
        assert_eq!(inner.get("after").get("camera").str(), Some("brama"));
    }
}

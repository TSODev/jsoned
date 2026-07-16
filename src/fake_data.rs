//! `fake` plugin — a small DSL for generating fake/random JSON structures during editing.
//! Grammar: expr := object | array | type_atom
//!          object := "{" field ("," field)* "}"        field := ident (":" expr)?
//!          array  := "[" count "]" expr
//!          type_atom := ident ("(" number ("," number)* ")")? ("@" locale)?
//! Two phases, kept separate like the rest of the codebase separates syntax from semantics:
//! `parse` is purely syntactic (never touches the `fake` crate), `generate` is the catalog lookup.
//!
//! Locale support (`@fr`) is intentionally narrow: the `fake` crate compiles in seven locales
//! unconditionally (no Cargo feature gates them), but most only override a handful of fields —
//! `fr_fr` itself only has real French data for names and phone numbers, everything else (city,
//! company, job, lorem text) silently falls back to English/Latin defaults in the underlying
//! crate. Rather than expose `@fr` everywhere and let it silently do nothing useful on
//! `city@fr`, `generate_leaf` only honors it on `name`/`first_name`/`last_name`/`phone`/`date`/
//! `datetime` and rejects any other `@fr` combination with an explicit error instead of silently
//! falling back to English. `date`/`datetime` are a different flavor of "@fr" than the other
//! four: the date *value* is never locale-specific, only its rendered *format* is (`%Y-%m-%d` vs
//! `%d/%m/%Y`) — `fake`'s own `faker::chrono` locales don't actually localize the format string
//! either (no locale overrides `CHRONO_DEFAULT_DATE_FORMAT`), so date/time generation is
//! hand-rolled here against `chrono` directly rather than routed through `fake` at all.

use anyhow::{anyhow, bail, Result};
use serde_json::Value;

use crate::plugin::Plugin;
use crate::tree::JNode;

#[derive(Debug, Clone, PartialEq)]
enum FakeSpec {
    Object(Vec<(String, FakeSpec)>),
    Array(usize, Box<FakeSpec>),
    Leaf(String, Vec<f64>, Option<String>),
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    LParen,
    RParen,
    Colon,
    Comma,
    At,
    Ident(String),
    Num(f64),
}

fn tokenize(input: &str) -> Result<Vec<Token>> {
    let chars: Vec<char> = input.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            c if c.is_whitespace() => i += 1,
            '{' => { tokens.push(Token::LBrace); i += 1; }
            '}' => { tokens.push(Token::RBrace); i += 1; }
            '[' => { tokens.push(Token::LBracket); i += 1; }
            ']' => { tokens.push(Token::RBracket); i += 1; }
            '(' => { tokens.push(Token::LParen); i += 1; }
            ')' => { tokens.push(Token::RParen); i += 1; }
            ':' => { tokens.push(Token::Colon); i += 1; }
            ',' => { tokens.push(Token::Comma); i += 1; }
            '@' => { tokens.push(Token::At); i += 1; }
            c if c.is_ascii_digit() => {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    i += 1;
                }
                let text: String = chars[start..i].iter().collect();
                let n: f64 = text
                    .parse()
                    .map_err(|_| anyhow!("fake parse error: invalid number '{text}'"))?;
                tokens.push(Token::Num(n));
            }
            c if c.is_alphabetic() || c == '_' => {
                let start = i;
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                tokens.push(Token::Ident(chars[start..i].iter().collect()));
            }
            other => bail!("fake parse error: unexpected character '{other}'"),
        }
    }
    Ok(tokens)
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn next(&mut self) -> Option<Token> {
        let t = self.tokens.get(self.pos).cloned();
        self.pos += 1;
        t
    }

    fn expect(&mut self, expected: Token) -> Result<()> {
        match self.next() {
            Some(ref t) if *t == expected => Ok(()),
            Some(t) => Err(anyhow!("fake parse error: expected {expected:?}, found {t:?}")),
            None => Err(anyhow!("fake parse error: expected {expected:?}, found end of input")),
        }
    }

    fn parse_expr(&mut self) -> Result<FakeSpec> {
        match self.peek() {
            Some(Token::LBrace) => self.parse_object(),
            Some(Token::LBracket) => self.parse_array(),
            Some(Token::Ident(_)) => self.parse_type_atom(),
            Some(t) => Err(anyhow!("fake parse error: unexpected token {t:?}")),
            None => Err(anyhow!("fake parse error: unexpected end of input")),
        }
    }

    fn parse_object(&mut self) -> Result<FakeSpec> {
        self.expect(Token::LBrace)?;
        let mut fields = Vec::new();
        if self.peek() != Some(&Token::RBrace) {
            loop {
                let key = match self.next() {
                    Some(Token::Ident(name)) => name,
                    Some(t) => bail!("fake parse error: expected field name, found {t:?}"),
                    None => bail!("fake parse error: unmatched '{{'"),
                };
                let value = if self.peek() == Some(&Token::Colon) {
                    self.next();
                    self.parse_expr()?
                } else {
                    FakeSpec::Leaf(key.clone(), Vec::new(), None)
                };
                fields.push((key, value));
                match self.peek() {
                    Some(Token::Comma) => { self.next(); }
                    Some(Token::RBrace) => break,
                    Some(t) => bail!("fake parse error: expected ',' or '}}', found {t:?}"),
                    None => bail!("fake parse error: unmatched '{{'"),
                }
            }
        }
        self.expect(Token::RBrace)?;
        Ok(FakeSpec::Object(fields))
    }

    fn parse_array(&mut self) -> Result<FakeSpec> {
        self.expect(Token::LBracket)?;
        let count = match self.next() {
            Some(Token::Num(n)) if n >= 0.0 && n.fract() == 0.0 => n as usize,
            Some(Token::Num(n)) => {
                bail!("fake parse error: array count must be a non-negative integer, got {n}")
            }
            Some(t) => bail!("fake parse error: expected array count, found {t:?}"),
            None => bail!("fake parse error: expected array count, found end of input"),
        };
        self.expect(Token::RBracket)?;
        let inner = self.parse_expr()?;
        Ok(FakeSpec::Array(count, Box::new(inner)))
    }

    fn parse_type_atom(&mut self) -> Result<FakeSpec> {
        let name = match self.next() {
            Some(Token::Ident(name)) => name,
            _ => unreachable!("parse_type_atom only called when peek() is Ident"),
        };
        let mut args = Vec::new();
        if self.peek() == Some(&Token::LParen) {
            self.next();
            if self.peek() != Some(&Token::RParen) {
                loop {
                    match self.next() {
                        Some(Token::Num(n)) => args.push(n),
                        Some(t) => bail!("fake parse error: expected numeric argument, found {t:?}"),
                        None => bail!("fake parse error: unmatched '('"),
                    }
                    match self.peek() {
                        Some(Token::Comma) => { self.next(); }
                        Some(Token::RParen) => break,
                        Some(t) => bail!("fake parse error: expected ',' or ')', found {t:?}"),
                        None => bail!("fake parse error: unmatched '('"),
                    }
                }
            }
            self.expect(Token::RParen)?;
        }
        let locale = if self.peek() == Some(&Token::At) {
            self.next();
            match self.next() {
                Some(Token::Ident(loc)) => Some(loc),
                Some(t) => bail!("fake parse error: expected locale name after '@', found {t:?}"),
                None => bail!("fake parse error: expected locale name after '@', found end of input"),
            }
        } else {
            None
        };
        Ok(FakeSpec::Leaf(name, args, locale))
    }
}

fn parse(input: &str) -> Result<FakeSpec> {
    let tokens = tokenize(input)?;
    if tokens.is_empty() {
        bail!("fake parse error: empty expression");
    }
    let mut parser = Parser { tokens, pos: 0 };
    let spec = parser.parse_expr()?;
    if parser.pos != parser.tokens.len() {
        bail!("fake parse error: trailing input after expression");
    }
    Ok(spec)
}

fn no_args(name: &str, args: &[f64]) -> Result<()> {
    if args.is_empty() {
        Ok(())
    } else {
        bail!("fake error: '{name}' does not take arguments")
    }
}

fn optional_count_arg(name: &str, args: &[f64], default: usize) -> Result<usize> {
    match args {
        [] => Ok(default),
        [n] if *n >= 0.0 && n.fract() == 0.0 => Ok(*n as usize),
        [n] => bail!("fake error: '{name}' argument must be a non-negative integer, got {n}"),
        _ => bail!("fake error: '{name}' takes 0 or 1 argument, got {}", args.len()),
    }
}

fn optional_pct_arg(name: &str, args: &[f64], default: u8) -> Result<u8> {
    match args {
        [] => Ok(default),
        [n] if (0.0..=100.0).contains(n) => Ok(*n as u8),
        [n] => bail!("fake error: '{name}' percentage must be between 0 and 100, got {n}"),
        _ => bail!("fake error: '{name}' takes 0 or 1 argument, got {}", args.len()),
    }
}

fn range_args(name: &str, args: &[f64], default: (f64, f64)) -> Result<(f64, f64)> {
    match args {
        [] => Ok(default),
        [min, max] if min <= max => Ok((*min, *max)),
        [min, max] => bail!("fake error: '{name}' min must be <= max, got ({min}, {max})"),
        _ => bail!("fake error: '{name}' takes 0 or 2 arguments (min,max), got {}", args.len()),
    }
}

fn random_date(min_year: i64, max_year: i64) -> chrono::NaiveDate {
    use chrono::NaiveDate;
    use fake::Fake;

    let year = (min_year..max_year + 1).fake::<i64>() as i32;
    let month = (1..=12).fake::<u32>();
    let mut day = (1..=31).fake::<u32>();
    while NaiveDate::from_ymd_opt(year, month, day).is_none() {
        day -= 1;
    }
    NaiveDate::from_ymd_opt(year, month, day).unwrap()
}

fn random_time() -> chrono::NaiveTime {
    use chrono::NaiveTime;
    use fake::Fake;

    let hour = (0..24).fake::<u32>();
    let min = (0..60).fake::<u32>();
    let sec = (0..60).fake::<u32>();
    NaiveTime::from_hms_opt(hour, min, sec).unwrap()
}

fn fr_unsupported(name: &str, fr: bool) -> Result<()> {
    if fr {
        bail!("fake error: 'fr' has no localized data for '{name}' — omit @fr");
    }
    Ok(())
}

fn generate_leaf(name: &str, args: &[f64], locale: Option<&str>) -> Result<Value> {
    use fake::faker::address::en::{BuildingNumber, CityName, CountryName, StreetName, ZipCode};
    use fake::faker::boolean::en::Boolean;
    use fake::faker::company::en::CompanyName;
    use fake::faker::internet::en::{DomainSuffix, SafeEmail, Username};
    use fake::faker::job::en::Title as JobTitle;
    use fake::faker::lorem::en::{Paragraph, Sentence, Word, Words};
    use fake::faker::name::en::{FirstName, LastName, Name};
    use fake::faker::name::fr_fr::{
        FirstName as FrFirstName, LastName as FrLastName, Name as FrName,
    };
    use fake::faker::phone_number::en::PhoneNumber;
    use fake::faker::phone_number::fr_fr::PhoneNumber as FrPhoneNumber;
    use fake::uuid::UUIDv4;
    use fake::Fake;

    if let Some(loc) = locale {
        if loc != "fr" {
            bail!("fake error: unknown locale '{loc}' (only 'fr' is supported)");
        }
    }
    let fr = locale.is_some();

    match name {
        "name" => {
            no_args(name, args)?;
            Ok(Value::String(if fr { FrName().fake() } else { Name().fake() }))
        }
        "first_name" => {
            no_args(name, args)?;
            Ok(Value::String(if fr { FrFirstName().fake() } else { FirstName().fake() }))
        }
        "last_name" => {
            no_args(name, args)?;
            Ok(Value::String(if fr { FrLastName().fake() } else { LastName().fake() }))
        }
        "phone" => {
            no_args(name, args)?;
            Ok(Value::String(if fr { FrPhoneNumber().fake() } else { PhoneNumber().fake() }))
        }
        "date" => {
            use chrono::Datelike;
            let now_year = chrono::Utc::now().year() as f64;
            let (min_year, max_year) = range_args(name, args, (now_year - 20.0, now_year))?;
            let date = random_date(min_year as i64, max_year as i64);
            let fmt = if fr { "%d/%m/%Y" } else { "%Y-%m-%d" };
            Ok(Value::String(date.format(fmt).to_string()))
        }
        "datetime" => {
            use chrono::Datelike;
            let now_year = chrono::Utc::now().year() as f64;
            let (min_year, max_year) = range_args(name, args, (now_year - 20.0, now_year))?;
            let dt = chrono::NaiveDateTime::new(random_date(min_year as i64, max_year as i64), random_time());
            let fmt = if fr { "%d/%m/%Y %H:%M:%S" } else { "%Y-%m-%dT%H:%M:%SZ" };
            Ok(Value::String(dt.format(fmt).to_string()))
        }
        "username" => { no_args(name, args)?; fr_unsupported(name, fr)?; Ok(Value::String(Username().fake())) }
        "email" => { no_args(name, args)?; fr_unsupported(name, fr)?; Ok(Value::String(SafeEmail().fake())) }
        "address" => {
            no_args(name, args)?;
            fr_unsupported(name, fr)?;
            let building: String = BuildingNumber().fake();
            let street: String = StreetName().fake();
            let city: String = CityName().fake();
            let zip: String = ZipCode().fake();
            Ok(Value::String(format!("{building} {street}, {city} {zip}")))
        }
        "city" => { no_args(name, args)?; fr_unsupported(name, fr)?; Ok(Value::String(CityName().fake())) }
        "country" => { no_args(name, args)?; fr_unsupported(name, fr)?; Ok(Value::String(CountryName().fake())) }
        "zipcode" => { no_args(name, args)?; fr_unsupported(name, fr)?; Ok(Value::String(ZipCode().fake())) }
        "url" => {
            no_args(name, args)?;
            fr_unsupported(name, fr)?;
            let user: String = Username().fake();
            let domain: String = DomainSuffix().fake();
            Ok(Value::String(format!("https://{user}.{domain}")))
        }
        "job" => { no_args(name, args)?; fr_unsupported(name, fr)?; Ok(Value::String(JobTitle().fake())) }
        "company" => { no_args(name, args)?; fr_unsupported(name, fr)?; Ok(Value::String(CompanyName().fake())) }
        "word" => { no_args(name, args)?; fr_unsupported(name, fr)?; Ok(Value::String(Word().fake())) }
        "words" => {
            fr_unsupported(name, fr)?;
            let n = optional_count_arg(name, args, 3)?;
            let words: Vec<String> = Words(n..n + 1).fake();
            Ok(Value::String(words.join(" ")))
        }
        "sentence" => {
            fr_unsupported(name, fr)?;
            let n = optional_count_arg(name, args, 6)?;
            Ok(Value::String(Sentence(n..n + 1).fake()))
        }
        "paragraph" => {
            fr_unsupported(name, fr)?;
            let n = optional_count_arg(name, args, 3)?;
            Ok(Value::String(Paragraph(n..n + 1).fake()))
        }
        "number" => {
            fr_unsupported(name, fr)?;
            let (min, max) = range_args(name, args, (0.0, 1000.0))?;
            let (min, max) = (min as i64, max as i64);
            Ok(Value::from((min..max + 1).fake::<i64>()))
        }
        "float" => {
            fr_unsupported(name, fr)?;
            let (min, max) = range_args(name, args, (0.0, 1.0))?;
            let v: f64 = (min..max).fake();
            Ok(serde_json::json!(v))
        }
        "bool" => {
            fr_unsupported(name, fr)?;
            let pct = optional_pct_arg(name, args, 50)?;
            Ok(Value::Bool(Boolean(pct).fake()))
        }
        "uuid" => { no_args(name, args)?; fr_unsupported(name, fr)?; Ok(Value::String(UUIDv4.fake())) }
        other => bail!("fake error: unknown type '{other}'"),
    }
}

fn generate(spec: &FakeSpec) -> Result<Value> {
    match spec {
        FakeSpec::Object(fields) => {
            // serde_json's `preserve_order` feature (enabled crate-wide) keeps this insertion
            // order intact all the way through `JNode::from_value`'s `IndexMap`.
            let mut map = serde_json::Map::new();
            for (key, sub) in fields {
                map.insert(key.clone(), generate(sub)?);
            }
            Ok(Value::Object(map))
        }
        FakeSpec::Array(count, sub) => {
            let mut items = Vec::with_capacity(*count);
            for _ in 0..*count {
                items.push(generate(sub)?);
            }
            Ok(Value::Array(items))
        }
        FakeSpec::Leaf(name, args, locale) => generate_leaf(name, args, locale.as_deref()),
    }
}

pub struct FakePlugin;

impl Plugin for FakePlugin {
    fn name(&self) -> &'static str {
        "fake"
    }

    fn prompt(&self) -> &'static str {
        "fake spec (e.g. {name, email}, [5]name, name@fr):"
    }

    fn run(&self, _input: &JNode, arg: &str) -> Result<JNode> {
        let value = generate(&parse(arg)?)?;
        Ok(JNode::from_value(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalar_leaf() {
        let out = generate(&parse("email").unwrap()).unwrap();
        assert!(out.as_str().unwrap().contains('@'));
    }

    #[test]
    fn array_of_objects_has_shape_and_count() {
        let out = generate(&parse("[3] { name, email }").unwrap()).unwrap();
        let arr = out.as_array().unwrap();
        assert_eq!(arr.len(), 3);
        for item in arr {
            let obj = item.as_object().unwrap();
            assert!(!obj["name"].as_str().unwrap().is_empty());
            assert!(obj["email"].as_str().unwrap().contains('@'));
        }
    }

    #[test]
    fn nested_object_derives_recursively() {
        let out = generate(&parse("{ user: { name, email }, tags: [2] word }").unwrap()).unwrap();
        let obj = out.as_object().unwrap();
        assert!(obj["user"].as_object().unwrap().contains_key("name"));
        assert_eq!(obj["tags"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn object_preserves_field_order() {
        let out = generate(&parse("{ b: word, a: word, c: word }").unwrap()).unwrap();
        let keys: Vec<&String> = out.as_object().unwrap().keys().collect();
        assert_eq!(keys, vec!["b", "a", "c"]);
    }

    #[test]
    fn number_respects_inclusive_bounds() {
        let spec = parse("number(18,65)").unwrap();
        for _ in 0..100 {
            let n = generate(&spec).unwrap().as_i64().unwrap();
            assert!((18..=65).contains(&n), "{n} out of bounds");
        }
    }

    #[test]
    fn bool_extreme_percentages_are_deterministic() {
        let always_true = parse("bool(100)").unwrap();
        let always_false = parse("bool(0)").unwrap();
        for _ in 0..50 {
            assert_eq!(generate(&always_true).unwrap(), serde_json::json!(true));
            assert_eq!(generate(&always_false).unwrap(), serde_json::json!(false));
        }
    }

    #[test]
    fn zero_count_array_is_empty() {
        let out = generate(&parse("[0] name").unwrap()).unwrap();
        assert_eq!(out, serde_json::json!([]));
    }

    #[test]
    fn nested_array_of_array_works() {
        let out = generate(&parse("[2] [3] word").unwrap()).unwrap();
        let outer = out.as_array().unwrap();
        assert_eq!(outer.len(), 2);
        assert_eq!(outer[0].as_array().unwrap().len(), 3);
    }

    #[test]
    fn unmatched_brace_is_a_parse_error() {
        assert!(parse("{ name, email").is_err());
    }

    #[test]
    fn negative_array_count_is_a_parse_error() {
        assert!(parse("[-1] name").is_err());
    }

    #[test]
    fn trailing_input_is_a_parse_error() {
        assert!(parse("name extra").is_err());
    }

    #[test]
    fn unknown_type_is_a_generate_error() {
        let spec = parse("bogus_type").unwrap();
        assert!(generate(&spec).is_err());
    }

    #[test]
    fn wrong_arg_count_is_a_generate_error() {
        let spec = parse("number(1,2,3)").unwrap();
        assert!(generate(&spec).is_err());
    }

    #[test]
    fn fr_locale_applies_to_whitelisted_leaves() {
        let out = generate(&parse("{ name: name@fr, phone: phone@fr }").unwrap()).unwrap();
        let obj = out.as_object().unwrap();
        assert!(!obj["name"].as_str().unwrap().is_empty());
        assert!(!obj["phone"].as_str().unwrap().is_empty());
    }

    #[test]
    fn fr_locale_on_unsupported_leaf_is_a_generate_error() {
        let spec = parse("city@fr").unwrap();
        let err = generate(&spec).unwrap_err().to_string();
        assert!(err.contains("no localized data"), "unexpected error: {err}");
    }

    #[test]
    fn unknown_locale_is_a_generate_error() {
        let spec = parse("name@es").unwrap();
        let err = generate(&spec).unwrap_err().to_string();
        assert!(err.contains("unknown locale"), "unexpected error: {err}");
    }

    #[test]
    fn date_default_format_is_iso() {
        let out = generate(&parse("date").unwrap()).unwrap();
        let s = out.as_str().unwrap().to_string();
        assert_eq!(s.len(), 10, "expected YYYY-MM-DD, got '{s}'");
        assert_eq!(s.as_bytes()[4], b'-');
        assert_eq!(s.as_bytes()[7], b'-');
    }

    #[test]
    fn date_fr_format_is_ddmmyyyy() {
        let out = generate(&parse("date@fr").unwrap()).unwrap();
        let s = out.as_str().unwrap().to_string();
        assert_eq!(s.len(), 10, "expected DD/MM/YYYY, got '{s}'");
        assert_eq!(s.as_bytes()[2], b'/');
        assert_eq!(s.as_bytes()[5], b'/');
    }

    #[test]
    fn date_respects_year_range() {
        let spec = parse("date(2000,2001)").unwrap();
        for _ in 0..50 {
            let s = generate(&spec).unwrap();
            let year: i32 = s.as_str().unwrap()[0..4].parse().unwrap();
            assert!((2000..=2001).contains(&year), "{s} out of range");
        }
    }

    #[test]
    fn datetime_fr_format_has_slashes_and_time() {
        let out = generate(&parse("datetime@fr").unwrap()).unwrap();
        let s = out.as_str().unwrap().to_string();
        assert!(s.contains('/'), "expected DD/MM/YYYY prefix in '{s}'");
        assert!(s.contains(':'), "expected HH:MM:SS suffix in '{s}'");
    }

    #[test]
    fn run_fake_ignores_input_node() {
        let plugin = FakePlugin;
        let input = JNode::from_value(serde_json::json!({"unrelated": true}));
        let out = plugin.run(&input, "email").unwrap();
        match out {
            JNode::Scalar(crate::tree::JScalar::String(s)) => assert!(s.contains('@')),
            _ => panic!("expected a string scalar"),
        }
    }
}

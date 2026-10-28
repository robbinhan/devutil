use crate::diagnostics::{Diagnostic, SourceRange};
use crate::json_value::{JSONDocument, JSONMember, JSONValue};

pub struct StrictJSONParser;

impl StrictJSONParser {
    pub fn new() -> Self {
        Self
    }

    pub fn parse(&self, source: &str) -> Result<JSONValue, Diagnostic> {
        Ok(self.parse_document(source)?.value)
    }

    pub fn parse_document(&self, source: &str) -> Result<JSONDocument, Diagnostic> {
        let mut parser = Parser::new(source);
        parser.parse_document()
    }
}

impl Default for StrictJSONParser {
    fn default() -> Self {
        Self::new()
    }
}

struct Parser {
    characters: Vec<char>,
    /// UTF-16 offset of `characters[i]`, with one extra entry past the end.
    ///
    /// Built once. `make_range` used to copy `characters[..index]` into a
    /// `String` on every object member (three times). On a multi-megabyte
    /// document that is quadratic: each call copies a longer prefix, the
    /// process climbs for minutes, and the allocator does not settle.
    utf16_offsets: Vec<usize>,
    index: usize,
}

impl Parser {
    fn new(source: &str) -> Self {
        let characters: Vec<char> = source.chars().collect();
        let mut utf16_offsets = Vec::with_capacity(characters.len() + 1);
        let mut units = 0usize;
        utf16_offsets.push(0);
        for ch in &characters {
            units += ch.len_utf16();
            utf16_offsets.push(units);
        }
        Self {
            characters,
            utf16_offsets,
            index: 0,
        }
    }

    fn parse_document(&mut self) -> Result<JSONDocument, Diagnostic> {
        self.skip_whitespace();
        let start = self.index;
        let value = self.parse_value()?;
        let value_end = self.index;
        self.skip_whitespace();

        if !self.is_at_end() {
            return Err(self.diagnostic("Unexpected content after the JSON value", self.index));
        }

        Ok(JSONDocument::new(value, self.make_range(start, value_end)))
    }

    fn parse_value(&mut self) -> Result<JSONValue, Diagnostic> {
        self.skip_whitespace();
        let Some(character) = self.current() else {
            return Err(self.diagnostic("Expected a JSON value", self.index));
        };

        match character {
            '{' => self.parse_object(),
            '[' => self.parse_array(),
            '"' => Ok(JSONValue::String(self.parse_string()?)),
            't' => {
                self.parse_literal("true")?;
                Ok(JSONValue::Bool(true))
            }
            'f' => {
                self.parse_literal("false")?;
                Ok(JSONValue::Bool(false))
            }
            'n' => {
                self.parse_literal("null")?;
                Ok(JSONValue::Null)
            }
            '-' | '0'..='9' => Ok(JSONValue::Number {
                raw: self.parse_number()?,
            }),
            _ => {
                // Swift also accepts Unicode numeric starts via Character.isNumber
                // after the ASCII digit match. Keep the unexpected-char path here
                // unless the character is a Unicode number (rare, but match Swift).
                if character.is_numeric() {
                    return Ok(JSONValue::Number {
                        raw: self.parse_number()?,
                    });
                }
                Err(self.diagnostic(
                    &format!("Unexpected character '{character}'"),
                    self.index,
                ))
            }
        }
    }

    fn parse_object(&mut self) -> Result<JSONValue, Diagnostic> {
        self.consume_expected('{');
        self.skip_whitespace();

        let mut members: Vec<JSONMember> = Vec::new();
        if self.consume_if_present('}') {
            return Ok(JSONValue::Object(members));
        }

        loop {
            self.skip_whitespace();
            let member_start = self.index;
            if self.current() != Some('"') {
                return Err(self.diagnostic("Object keys must be double-quoted strings", self.index));
            }

            let key_start = self.index;
            let key = self.parse_string()?;
            let key_end = self.index;
            self.skip_whitespace();
            if !self.consume_if_present(':') {
                return Err(self.diagnostic("Expected ':' after object key", self.index));
            }

            let value_start = self.index;
            let value = self.parse_value()?;
            let value_end = self.index;
            members.push(JSONMember::with_ranges(
                key,
                value,
                Some(self.make_range(key_start, key_end)),
                Some(self.make_range(value_start, value_end)),
                Some(self.make_range(member_start, value_end)),
            ));

            self.skip_whitespace();
            if self.consume_if_present('}') {
                return Ok(JSONValue::Object(members));
            }

            if !self.consume_if_present(',') {
                return Err(self.diagnostic(
                    "Expected ',' or '}' after object member",
                    self.index,
                ));
            }

            self.skip_whitespace();
            if self.current() == Some('}') {
                return Err(self.diagnostic(
                    "Trailing commas are not valid in strict JSON",
                    self.index,
                ));
            }
        }
    }

    fn parse_array(&mut self) -> Result<JSONValue, Diagnostic> {
        self.consume_expected('[');
        self.skip_whitespace();

        let mut values: Vec<JSONValue> = Vec::new();
        if self.consume_if_present(']') {
            return Ok(JSONValue::Array(values));
        }

        loop {
            values.push(self.parse_value()?);
            self.skip_whitespace();
            if self.consume_if_present(']') {
                return Ok(JSONValue::Array(values));
            }

            if !self.consume_if_present(',') {
                return Err(self.diagnostic(
                    "Expected ',' or ']' after array value",
                    self.index,
                ));
            }

            self.skip_whitespace();
            if self.current() == Some(']') {
                return Err(self.diagnostic(
                    "Trailing commas are not valid in strict JSON",
                    self.index,
                ));
            }
        }
    }

    fn parse_string(&mut self) -> Result<String, Diagnostic> {
        self.consume_expected('"');
        let mut result = String::new();

        while let Some(character) = self.current() {
            if character == '"' {
                self.advance();
                return Ok(result);
            }

            if character == '\\' {
                self.advance();
                result.push(self.parse_escape()?);
                continue;
            }

            if is_control_character(character) {
                return Err(self.diagnostic("Unescaped control character in string", self.index));
            }

            result.push(character);
            self.advance();
        }

        Err(self.diagnostic("Unterminated string", self.index))
    }

    fn parse_escape(&mut self) -> Result<char, Diagnostic> {
        let Some(character) = self.current() else {
            return Err(self.diagnostic("Unterminated escape sequence", self.index));
        };

        match character {
            '"' => {
                self.advance();
                Ok('"')
            }
            '\\' => {
                self.advance();
                Ok('\\')
            }
            '/' => {
                self.advance();
                Ok('/')
            }
            'b' => {
                self.advance();
                Ok('\u{8}')
            }
            'f' => {
                self.advance();
                Ok('\u{c}')
            }
            'n' => {
                self.advance();
                Ok('\n')
            }
            'r' => {
                self.advance();
                Ok('\r')
            }
            't' => {
                self.advance();
                Ok('\t')
            }
            'u' => self.parse_unicode_escape(),
            _ => Err(self.diagnostic(
                &format!("Invalid escape sequence '\\{character}'"),
                self.index,
            )),
        }
    }

    fn parse_unicode_escape(&mut self) -> Result<char, Diagnostic> {
        self.consume_expected('u');
        let first = self.parse_hexadecimal_code_unit()?;

        if (0xD800..=0xDBFF).contains(&first) {
            if !(self.consume_if_present('\\') && self.consume_if_present('u')) {
                return Err(self.diagnostic(
                    "High surrogate must be followed by a low surrogate",
                    self.index,
                ));
            }
            let second = self.parse_hexadecimal_code_unit()?;
            if !(0xDC00..=0xDFFF).contains(&second) {
                return Err(self.diagnostic("Invalid low surrogate", self.index));
            }
            let scalar_value = 0x10000 + ((first - 0xD800) << 10) + (second - 0xDC00);
            return char::from_u32(scalar_value)
                .ok_or_else(|| self.diagnostic("Invalid Unicode scalar", self.index));
        }

        if (0xDC00..=0xDFFF).contains(&first) {
            return Err(self.diagnostic("Invalid Unicode surrogate", self.index));
        }
        char::from_u32(first).ok_or_else(|| self.diagnostic("Invalid Unicode surrogate", self.index))
    }

    fn parse_hexadecimal_code_unit(&mut self) -> Result<u32, Diagnostic> {
        let mut value: u32 = 0;
        for _ in 0..4 {
            let Some(character) = self.current() else {
                return Err(self.diagnostic(
                    "Expected four hexadecimal digits after '\\u'",
                    self.index,
                ));
            };
            let Some(digit) = hexadecimal_value(character) else {
                return Err(self.diagnostic(
                    "Expected four hexadecimal digits after '\\u'",
                    self.index,
                ));
            };
            value = value * 16 + digit;
            self.advance();
        }
        Ok(value)
    }

    fn parse_number(&mut self) -> Result<String, Diagnostic> {
        let start = self.index;

        let _ = self.consume_if_present('-');
        let Some(first) = self.current() else {
            return Err(self.diagnostic("Incomplete number", self.index));
        };

        if first == '0' {
            self.advance();
            if let Some(next) = self.current() {
                if next.is_numeric() {
                    return Err(self.diagnostic(
                        "Leading zeroes are not valid in JSON numbers",
                        self.index,
                    ));
                }
            }
        } else if first.is_numeric() && first != '0' {
            loop {
                self.advance();
                if self.current().map(|c| c.is_numeric()) != Some(true) {
                    break;
                }
            }
        } else {
            return Err(self.diagnostic("Expected a digit in number", self.index));
        }

        if self.consume_if_present('.') {
            if self.current().map(|c| c.is_numeric()) != Some(true) {
                return Err(self.diagnostic("Expected digits after decimal point", self.index));
            }
            loop {
                self.advance();
                if self.current().map(|c| c.is_numeric()) != Some(true) {
                    break;
                }
            }
        }

        if self.current() == Some('e') || self.current() == Some('E') {
            self.advance();
            let _ = self.consume_if_present('+') || self.consume_if_present('-');
            if self.current().map(|c| c.is_numeric()) != Some(true) {
                return Err(self.diagnostic("Expected digits in exponent", self.index));
            }
            loop {
                self.advance();
                if self.current().map(|c| c.is_numeric()) != Some(true) {
                    break;
                }
            }
        }

        Ok(self.characters[start..self.index].iter().collect())
    }

    fn parse_literal(&mut self, literal: &str) -> Result<(), Diagnostic> {
        for expected in literal.chars() {
            if self.current() != Some(expected) {
                return Err(self.diagnostic(&format!("Expected '{literal}'"), self.index));
            }
            self.advance();
        }
        Ok(())
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.current(), Some(' ' | '\n' | '\r' | '\t')) {
            self.advance();
        }
    }

    fn consume_if_present(&mut self, expected: char) -> bool {
        if self.current() == Some(expected) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn consume_expected(&mut self, expected: char) {
        assert_eq!(self.current(), Some(expected), "parser invariant violated");
        self.advance();
    }

    fn advance(&mut self) {
        self.index += 1;
    }

    fn current(&self) -> Option<char> {
        self.characters.get(self.index).copied()
    }

    fn is_at_end(&self) -> bool {
        self.index >= self.characters.len()
    }

    fn make_range(&self, start: usize, end: usize) -> SourceRange {
        let last = self.utf16_offsets.len() - 1;
        SourceRange::new(
            self.utf16_offsets[start.min(last)],
            self.utf16_offsets[end.min(last)],
        )
    }

    fn diagnostic(&self, message: &str, offset: usize) -> Diagnostic {
        let clipped = offset.min(self.characters.len());
        let mut line = 1usize;
        let mut column = 1usize;
        for ch in &self.characters[..clipped] {
            if *ch == '\n' {
                line += 1;
                column = 1;
            } else {
                column += 1;
            }
        }

        Diagnostic::new(message)
            .with_range(Some(
                self.make_range(offset, (offset + 1).min(self.characters.len())),
            ))
            .with_location(line, column)
    }
}

fn is_control_character(character: char) -> bool {
    (character as u32) < 0x20
}

fn hexadecimal_value(character: char) -> Option<u32> {
    let scalar = character as u32;
    match scalar {
        48..=57 => Some(scalar - 48),
        65..=70 => Some(scalar - 55),
        97..=102 => Some(scalar - 87),
        _ => None,
    }
}



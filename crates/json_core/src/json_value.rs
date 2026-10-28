use crate::diagnostics::SourceRange;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JSONValue {
    Object(Vec<JSONMember>),
    Array(Vec<JSONValue>),
    String(String),
    Number { raw: String },
    Bool(bool),
    Null,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JSONMember {
    pub key: String,
    pub value: JSONValue,
    pub key_range: Option<SourceRange>,
    pub value_range: Option<SourceRange>,
    pub source_range: Option<SourceRange>,
}

impl JSONMember {
    pub fn new(key: impl Into<String>, value: JSONValue) -> Self {
        Self {
            key: key.into(),
            value,
            key_range: None,
            value_range: None,
            source_range: None,
        }
    }

    pub fn with_ranges(
        key: impl Into<String>,
        value: JSONValue,
        key_range: Option<SourceRange>,
        value_range: Option<SourceRange>,
        source_range: Option<SourceRange>,
    ) -> Self {
        Self {
            key: key.into(),
            value,
            key_range,
            value_range,
            source_range,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JSONDocument {
    pub value: JSONValue,
    pub source_range: SourceRange,
}

impl JSONDocument {
    pub fn new(value: JSONValue, source_range: SourceRange) -> Self {
        Self {
            value,
            source_range,
        }
    }
}

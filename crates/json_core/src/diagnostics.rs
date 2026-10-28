use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceRange {
    pub utf16_start: usize,
    pub utf16_end: usize,
}

impl SourceRange {
    pub fn new(utf16_start: usize, utf16_end: usize) -> Self {
        Self {
            utf16_start,
            utf16_end,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub message: String,
    pub severity: DiagnosticSeverity,
    pub range: Option<SourceRange>,
    pub line: usize,
    pub column: usize,
}

impl Diagnostic {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            severity: DiagnosticSeverity::Error,
            range: None,
            line: 1,
            column: 1,
        }
    }

    pub fn with_location(mut self, line: usize, column: usize) -> Self {
        self.line = line;
        self.column = column;
        self
    }

    pub fn with_range(mut self, range: Option<SourceRange>) -> Self {
        self.range = range;
        self
    }

    pub fn with_severity(mut self, severity: DiagnosticSeverity) -> Self {
        self.severity = severity;
        self
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for Diagnostic {}

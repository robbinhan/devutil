//! Ordered JSON engine ported from the Swift `JSONCore` package.
//!
//! Public surface matches the Swift workbench contract:
//! `AnalysisEngine::analyze(source) -> AnalysisResult`.

mod analysis;
mod candidate;
mod canonical;
mod classifier;
mod diagnostics;
mod json_value;
mod normalization;
mod recovery;
mod strict_parser;

pub use analysis::{AnalysisEngine, AnalysisPolicy, AnalysisResult, Disposition};
pub use candidate::CandidateNormalizer;
pub use canonical::CanonicalSerializer;
pub use classifier::InputClassifier;
pub use diagnostics::{Diagnostic, DiagnosticSeverity, SourceRange};
pub use json_value::{JSONDocument, JSONMember, JSONValue};
pub use normalization::{InputKind, NormalizationCandidate, NormalizationStep};
pub use recovery::{RecoveryParser, RecoveryResult};
pub use strict_parser::StrictJSONParser;


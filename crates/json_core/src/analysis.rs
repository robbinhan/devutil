use crate::candidate::CandidateNormalizer;
use crate::canonical::CanonicalSerializer;
use crate::diagnostics::{Diagnostic, DiagnosticSeverity};
use crate::json_value::JSONValue;
use crate::normalization::{NormalizationCandidate, NormalizationStep};
use crate::recovery::RecoveryParser;
use crate::strict_parser::StrictJSONParser;

#[derive(Clone, Debug, PartialEq)]
pub enum Disposition {
    Accepted,
    Recovered,
    Inspect(Vec<NormalizationCandidate>),
    Rejected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnalysisPolicy {
    pub allow_automatic_recovery: bool,
}

impl AnalysisPolicy {
    pub fn new(allow_automatic_recovery: bool) -> Self {
        Self {
            allow_automatic_recovery,
        }
    }

    pub fn default_policy() -> Self {
        Self {
            allow_automatic_recovery: true,
        }
    }
}

impl Default for AnalysisPolicy {
    fn default() -> Self {
        Self::default_policy()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AnalysisResult {
    pub source: String,
    pub normalized: Option<String>,
    pub value: Option<JSONValue>,
    pub steps: Vec<NormalizationStep>,
    pub diagnostics: Vec<Diagnostic>,
    pub disposition: Disposition,
}

impl AnalysisResult {
    pub fn new(
        source: impl Into<String>,
        normalized: Option<String>,
        value: Option<JSONValue>,
        steps: Vec<NormalizationStep>,
        diagnostics: Vec<Diagnostic>,
        disposition: Disposition,
    ) -> Self {
        Self {
            source: source.into(),
            normalized,
            value,
            steps,
            diagnostics,
            disposition,
        }
    }
}

pub struct AnalysisEngine {
    parser: StrictJSONParser,
    normalizer: CandidateNormalizer,
    recovery: RecoveryParser,
}

impl AnalysisEngine {
    pub fn new() -> Self {
        Self {
            parser: StrictJSONParser::new(),
            normalizer: CandidateNormalizer::new(),
            recovery: RecoveryParser::new(),
        }
    }

    pub fn analyze(&self, source: &str) -> AnalysisResult {
        self.analyze_with_policy(source, AnalysisPolicy::default())
    }

    pub fn analyze_with_policy(&self, source: &str, policy: AnalysisPolicy) -> AnalysisResult {
        match self.parser.parse(source) {
            Ok(value) => {
                let candidates = self.normalizer.generate_candidates(source);
                if let Some(candidate) = candidates.iter().find(|c| !c.requires_user_action) {
                    if let Ok(recovered_value) = self.parser.parse(&candidate.text) {
                        return self.result(
                            source,
                            recovered_value,
                            candidate.steps.clone(),
                            Disposition::Recovered,
                        );
                    }
                }
                self.result(source, value, Vec::new(), Disposition::Accepted)
            }
            Err(strict_diagnostic) => {
                if policy.allow_automatic_recovery {
                    let candidates = self.normalizer.generate_candidates(source);
                    if let Some(candidate) = candidates.iter().find(|c| !c.requires_user_action) {
                        if let Ok(value) = self.parser.parse(&candidate.text) {
                            return self.result(
                                source,
                                value,
                                candidate.steps.clone(),
                                Disposition::Recovered,
                            );
                        }
                    }
                }

                if policy.allow_automatic_recovery {
                    if let Ok(recovered) = self.recovery.repair(source) {
                        if let Ok(value) = self.parser.parse(&recovered.repaired_text) {
                            return self.result(
                                source,
                                value,
                                recovered.steps,
                                Disposition::Recovered,
                            );
                        }
                    }
                }

                let candidates: Vec<NormalizationCandidate> = self
                    .normalizer
                    .generate_candidates(source)
                    .into_iter()
                    .filter(|c| c.requires_user_action)
                    .collect();
                if !candidates.is_empty() {
                    return AnalysisResult::new(
                        source.to_string(),
                        None,
                        None,
                        Vec::new(),
                        vec![strict_diagnostic],
                        Disposition::Inspect(candidates),
                    );
                }

                AnalysisResult::new(
                    source.to_string(),
                    None,
                    None,
                    Vec::new(),
                    vec![strict_diagnostic],
                    Disposition::Rejected,
                )
            }
        }
    }

    fn result(
        &self,
        source: &str,
        value: JSONValue,
        steps: Vec<NormalizationStep>,
        disposition: Disposition,
    ) -> AnalysisResult {
        AnalysisResult::new(
            source.to_string(),
            Some(CanonicalSerializer::compact(&value)),
            Some(value.clone()),
            steps,
            duplicate_key_diagnostics(&value),
            disposition,
        )
    }
}

impl Default for AnalysisEngine {
    fn default() -> Self {
        Self::new()
    }
}

fn duplicate_key_diagnostics(value: &JSONValue) -> Vec<Diagnostic> {
    match value {
        JSONValue::Object(members) => {
            let mut seen = std::collections::HashSet::new();
            let mut diagnostics = Vec::new();
            for member in members {
                if !seen.insert(member.key.clone()) {
                    diagnostics.push(
                        Diagnostic::new(format!(
                            "Duplicate object key '{}' was preserved",
                            member.key
                        ))
                        .with_severity(DiagnosticSeverity::Warning)
                        .with_range(member.key_range),
                    );
                }
                diagnostics.extend(duplicate_key_diagnostics(&member.value));
            }
            diagnostics
        }
        JSONValue::Array(values) => values.iter().flat_map(duplicate_key_diagnostics).collect(),
        JSONValue::String(_) | JSONValue::Number { .. } | JSONValue::Bool(_) | JSONValue::Null => {
            Vec::new()
        }
    }
}

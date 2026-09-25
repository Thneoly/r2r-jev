use serde::{Deserialize, Serialize};

pub const ONE_PPM: u32 = 1_000_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JudgmentObserved {
    pub provider: String,
    pub subject: String,
    pub scope: String,
    pub task: String,
    pub tool: String,
    pub intent: String,
    pub beyond_scope_ppm: u32,
    pub destructive_ppm: u32,
}

impl JudgmentObserved {
    /// Adapter-boundary constructor: all judgment identity/context fields remain
    /// explicit so floating-point probabilities are normalized exactly once.
    #[allow(clippy::too_many_arguments)]
    pub fn from_probabilities(
        provider: impl Into<String>,
        subject: impl Into<String>,
        scope: impl Into<String>,
        task: impl Into<String>,
        tool: impl Into<String>,
        intent: impl Into<String>,
        beyond_scope: f64,
        destructive: f64,
    ) -> Self {
        Self {
            provider: provider.into(),
            subject: subject.into(),
            scope: scope.into(),
            task: task.into(),
            tool: tool.into(),
            intent: intent.into(),
            beyond_scope_ppm: probability_to_ppm(beyond_scope),
            destructive_ppm: probability_to_ppm(destructive),
        }
    }
}

pub fn probability_to_ppm(value: f64) -> u32 {
    let clamped = value.clamp(0.0, 1.0);
    (clamped * ONE_PPM as f64).round() as u32
}

pub fn ppm_to_display(value: u32) -> String {
    format!("{:.6}", value as f64 / ONE_PPM as f64)
}

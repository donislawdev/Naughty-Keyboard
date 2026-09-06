//! Pure logic of Naughty Keyboard.
//!
//! # What this layer may not do
//!
//! No files, no clock, no randomness, no network, no text meant for a human.
//! Everything here is testable without starting an operating system, and this
//! crate depends on nothing - which is what makes the rule enforceable rather
//! than merely stated: reaching upwards from here is a compile error.
//!
//! # Why there are no messages here
//!
//! This layer returns facts: codes, counts and shapes. The words shown to a
//! person are assembled one layer out, from translation keys. A core that
//! returns a finished sentence drags the text layer downwards and quietly
//! removes the ability to translate the interface.

#![forbid(unsafe_code)]

pub mod lint;
pub mod metrics;
pub mod source_text;
pub mod text;
pub mod value;

pub use lint::{
    LintProblem, LintReport, LintRule, RULES, RuleCode, RuleCoverage, RuleStatus, Severity,
    rule_for,
};
pub use metrics::TextMetrics;
pub use source_text::line_of;
pub use text::{EscapedText, LiteralText};
pub use value::{MAX_REPEAT_COUNT, MAX_VALUE_CODE_POINTS, ValueBody, ValueProblem};

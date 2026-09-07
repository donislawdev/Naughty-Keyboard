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

pub mod description;
pub mod identity;
pub mod lint;
pub mod metrics;
pub mod pack;
pub mod schema;
pub mod source_text;
pub mod text;
pub mod value;
pub mod written_form;

pub use description::{BreaksFault, MIN_BREAKS_CODE_POINTS, check_breaks, looks_english};
pub use identity::{is_pack_id, is_value_id};
pub use lint::{
    LintProblem, LintReport, LintRule, MAX_VALUES_PER_PACK, RULES, RuleCode, RuleCoverage,
    RuleStatus, Severity, SkipReason, SkippedRule, rule_for,
};
pub use metrics::TextMetrics;
pub use pack::{Pack, PackPair, PackValue, Risk};
pub use schema::{FieldKind, kind_of};
pub use source_text::line_of;
pub use text::{EscapedText, LiteralText};
pub use value::{MAX_REPEAT_COUNT, MAX_VALUE_CODE_POINTS, ValueBody, ValueProblem};
pub use written_form::{LITERAL_LENGTH_HINT, Quoting};

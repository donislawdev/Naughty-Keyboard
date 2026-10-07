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
pub mod graphemes;
pub mod hotkeys;
pub mod identity;
pub mod ignorable;
pub mod keys;
pub mod lint;
pub mod metrics;
pub mod normalization;
pub mod pack;
pub mod preview;
pub mod report;
pub mod schema;
pub mod screens;
pub mod sequence;
pub mod source_text;
pub mod text;
pub mod typeface;
pub mod value;
pub mod written_form;

pub use description::{BreaksFault, MIN_BREAKS_CODE_POINTS, check_breaks, looks_english};
pub use graphemes::UNICODE_VERSION;
pub use hotkeys::{
    Bindings, ChordError, ChordProblem, Convention, DEFAULT_BINDINGS, HotkeyAction, HotkeyChord,
    HotkeyKey, MACOS_DEFAULT_BINDINGS, Refusal, Refused, default_bindings, default_chord,
};
pub use identity::{ValueKey, is_pack_id, is_value_id};
pub use lint::{
    LintProblem, LintReport, LintRule, MAX_VALUES_PER_PACK, RULES, RuleCode, RuleCoverage,
    RuleStatus, Severity, SkipReason, SkippedRule, rule_for,
};
pub use metrics::TextMetrics;
pub use pack::{Pack, PackPair, PackValue, Risk};
pub use report::{Arrival, ControlKind, ReportBlock, SpelledOut, Target, Typed};
pub use schema::{FieldKind, kind_of};
pub use sequence::{Delivery, Effect, Event, Position, Sequence, Step};
pub use source_text::SourceText;
pub use text::{EscapedText, LiteralText};
pub use typeface::{TypefaceGuarantee, outside_guarantee};
pub use value::{MAX_REPEAT_COUNT, MAX_VALUE_CODE_POINTS, ValueBody, ValueProblem};
pub use written_form::{LITERAL_LENGTH_HINT, Quoting};

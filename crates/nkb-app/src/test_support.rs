//! Fakes shared by the tests of the use cases that drive the sequence.
//!
//! They started life inside `advance_sequence`'s tests and moved here on
//! 2026-09-22 when `drive_sequence` needed the same delivery, keys, source and
//! format doubles - one copy, so a fake that grows a behaviour grows it for
//! every test that leans on it. Test-only: the module exists under `cfg(test)`
//! and nothing in the product can reach it.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]

use crate::advance_sequence::AdvanceSequence;
use crate::ports::{
    Availability, Date, Delivered, DeliveryError, KeystrokeError, KeystrokeSender, PackFormat,
    PackSource, SourceError, TargetRef, TranslationCheck, TranslationTarget, ValueDelivery,
};
use nkb_core::keys::KeyChord;
use nkb_core::lint::{LintProblem, RuleCode};
use nkb_core::pack::{Pack, PackValue, Risk};
use nkb_core::text::LiteralText;
use nkb_core::value::ValueBody;

/// A delivery whose target presence and outcome a test dictates.
pub(crate) struct FakeDelivery {
    target: Option<TargetRef>,
    fail: Option<DeliveryError>,
}

impl FakeDelivery {
    pub(crate) fn ready() -> Self {
        Self {
            target: Some(TargetRef(1)),
            fail: None,
        }
    }
    pub(crate) fn without_target() -> Self {
        Self {
            target: None,
            fail: None,
        }
    }
    pub(crate) fn failing(error: DeliveryError) -> Self {
        Self {
            target: Some(TargetRef(1)),
            fail: Some(error),
        }
    }
}

impl ValueDelivery for FakeDelivery {
    fn availability(&self) -> Availability {
        Availability::Ready
    }
    fn target(&self) -> Option<TargetRef> {
        self.target
    }
    fn deliver(&self, text: &str) -> Result<Delivered, DeliveryError> {
        match &self.fail {
            None => Ok(Delivered {
                utf16_units: text.encode_utf16().count(),
            }),
            Some(error) => Err(error.clone()),
        }
    }
}

pub(crate) struct FakeKeys {
    fail: Option<KeystrokeError>,
}

impl FakeKeys {
    pub(crate) fn working() -> Self {
        Self { fail: None }
    }
    pub(crate) fn failing(error: KeystrokeError) -> Self {
        Self { fail: Some(error) }
    }
}

impl KeystrokeSender for FakeKeys {
    fn send_keystrokes(&self, _chords: &[KeyChord]) -> Result<(), KeystrokeError> {
        match &self.fail {
            None => Ok(()),
            Some(error) => Err(error.clone()),
        }
    }
}

pub(crate) struct FakeSource;

impl PackSource for FakeSource {
    fn read(&self, _id: &str) -> Result<String, SourceError> {
        Ok("the format is scripted, this text is ignored".to_owned())
    }
}

pub(crate) struct MissingSource;

impl PackSource for MissingSource {
    fn read(&self, _id: &str) -> Result<String, SourceError> {
        Err(SourceError::NotFound)
    }
}

pub(crate) fn a_value(id: &str, text: &str, risk: Option<Risk>) -> PackValue {
    PackValue {
        id: id.to_owned(),
        name: format!("Value {id}"),
        body: ValueBody::Literal(LiteralText::new(text.to_owned())),
        breaks: Some("It breaks something worth a whole sentence about it.".to_owned()),
        expect: None,
        risk,
        fields: Vec::new(),
        tags: Vec::new(),
        source: None,
        since: Some("1.0".to_owned()),
        shape: None,
        deprecated: false,
        replaced_by: None,
    }
}

pub(crate) fn a_pack(values: Vec<PackValue>, risk: Risk) -> Pack {
    Pack {
        id: "sample".to_owned(),
        name: "Sample pack".to_owned(),
        description: "A pack for the tests in this module.".to_owned(),
        version: "1.0".to_owned(),
        updated: "2026-09-15".to_owned(),
        license: "CC-BY-4.0".to_owned(),
        authors: vec!["Naughty Keyboard".to_owned()],
        language: "en".to_owned(),
        risk,
        tags: Vec::new(),
        fields: vec!["any".to_owned()],
        source: None,
        values,
        pairs: Vec::new(),
    }
}

pub(crate) struct FakeFormat {
    errors: usize,
    pack: Option<Pack>,
}

impl FakeFormat {
    pub(crate) fn of(pack: Pack) -> Self {
        Self {
            errors: 0,
            pack: Some(pack),
        }
    }
    pub(crate) fn with_errors(n: usize) -> Self {
        Self {
            errors: n,
            pack: None,
        }
    }
}

impl PackFormat for FakeFormat {
    fn check(&self, _text: &str, _expected_id: &str) -> Vec<LintProblem> {
        (0..self.errors)
            .map(|_| LintProblem::new(RuleCode::PackWithoutValues))
            .collect()
    }
    fn parse(&self, _text: &str) -> Option<Pack> {
        self.pack.clone()
    }
    fn skeleton(&self, _id: &str, _today: Date) -> String {
        String::new()
    }
    fn canonical(&self, _text: &str) -> Option<String> {
        None
    }
    fn translated_pack(&self, _text: &str) -> TranslationTarget {
        TranslationTarget::NotATranslation
    }
    fn check_translation(&self, _t: &str, _o: &str) -> TranslationCheck {
        TranslationCheck::Compared(Vec::new())
    }
    fn same_insertions(&self, _before: &str, _after: &str) -> bool {
        true
    }
}

/// Chooses a three-value pack and hands back the ready sequence machine.
pub(crate) fn chosen(risk: Risk) -> AdvanceSequence {
    let mut advance = AdvanceSequence::new();
    let pack = a_pack(
        vec![
            a_value("one", "alpha", None),
            a_value("two", "beta", None),
            a_value("three", "gamma", None),
        ],
        risk,
    );
    advance
        .choose_pack(&FakeSource, &FakeFormat::of(pack), "sample")
        .expect("a scripted pack with no errors loads");
    advance
}

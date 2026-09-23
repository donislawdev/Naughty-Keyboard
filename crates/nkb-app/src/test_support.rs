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

use std::cell::RefCell;

use crate::advance_sequence::{AdvanceSequence, Ports};
use crate::ports::{
    Availability, Clipboard, ClipboardError, Date, Delivered, DeliveryError, History,
    KeystrokeError, KeystrokeSender, PackFormat, PackSource, ReportText, SourceError, TargetRef,
    TranslationCheck, TranslationTarget, ValueDelivery,
};
use nkb_core::keys::KeyChord;
use nkb_core::lint::{LintProblem, RuleCode};
use nkb_core::pack::{Pack, PackValue, Risk};
use nkb_core::report::ReportBlock;
use nkb_core::text::LiteralText;
use nkb_core::value::ValueBody;

/// A delivery whose availability, target and outcome a test dictates, and
/// which keeps every text it was handed - so a test can say which ROUTE a value
/// went by, which is the whole question once there are two.
pub(crate) struct FakeDelivery {
    availability: Availability,
    target: Option<TargetRef>,
    fail: Option<DeliveryError>,
    pub(crate) handed: RefCell<Vec<String>>,
}

impl FakeDelivery {
    pub(crate) fn ready() -> Self {
        Self {
            availability: Availability::Ready,
            target: Some(TargetRef(1)),
            fail: None,
            handed: RefCell::new(Vec::new()),
        }
    }
    pub(crate) fn without_target() -> Self {
        Self {
            target: None,
            ..Self::ready()
        }
    }
    pub(crate) fn failing(error: DeliveryError) -> Self {
        Self {
            fail: Some(error),
            ..Self::ready()
        }
    }
    /// No route on this system, known before any send - what `DirectInjection`
    /// answers on macOS and Linux today.
    pub(crate) fn unavailable(system: &str) -> Self {
        Self {
            availability: Availability::Unavailable {
                reason: system.to_owned(),
            },
            fail: Some(DeliveryError::Unsupported {
                system: system.to_owned(),
            }),
            ..Self::ready()
        }
    }
}

impl ValueDelivery for FakeDelivery {
    fn availability(&self) -> Availability {
        self.availability.clone()
    }
    fn target(&self) -> Option<TargetRef> {
        self.target
    }
    fn deliver(&self, text: &str) -> Result<Delivered, DeliveryError> {
        self.handed.borrow_mut().push(text.to_owned());
        match &self.fail {
            None => Ok(Delivered {
                utf16_units: text.encode_utf16().count(),
            }),
            Some(error) => Err(error.clone()),
        }
    }
}

/// Keys that go through or refuse as a test says, and count the requests - so
/// a test can prove that a route pressed NOTHING.
pub(crate) struct FakeKeys {
    fail: Option<KeystrokeError>,
    pub(crate) requests: RefCell<usize>,
}

impl FakeKeys {
    pub(crate) fn working() -> Self {
        Self {
            fail: None,
            requests: RefCell::new(0),
        }
    }
    pub(crate) fn failing(error: KeystrokeError) -> Self {
        Self {
            fail: Some(error),
            requests: RefCell::new(0),
        }
    }
}

impl KeystrokeSender for FakeKeys {
    fn send_keystrokes(&self, _chords: &[KeyChord]) -> Result<(), KeystrokeError> {
        *self.requests.borrow_mut() += 1;
        match &self.fail {
            None => Ok(()),
            Some(error) => Err(error.clone()),
        }
    }
}

/// A clipboard that records what it was handed and under which history rule,
/// or refuses as a test says.
pub(crate) struct FakeClipboard {
    fail: Option<ClipboardError>,
    pub(crate) puts: RefCell<Vec<String>>,
    pub(crate) histories: RefCell<Vec<History>>,
}

impl FakeClipboard {
    pub(crate) fn working() -> Self {
        Self {
            fail: None,
            puts: RefCell::new(Vec::new()),
            histories: RefCell::new(Vec::new()),
        }
    }
    pub(crate) fn failing(error: ClipboardError) -> Self {
        Self {
            fail: Some(error),
            ..Self::working()
        }
    }
}

impl Clipboard for FakeClipboard {
    fn put_text(&self, text: &str, history: History) -> Result<(), ClipboardError> {
        match &self.fail {
            None => {
                self.puts.borrow_mut().push(text.to_owned());
                self.histories.borrow_mut().push(history);
                Ok(())
            }
            Some(error) => Err(error.clone()),
        }
    }
}

/// A report text that keeps every block it was asked to write, so a test can
/// look at the FACTS rather than parse a sentence back out of the text.
pub(crate) struct FakeReportText {
    pub(crate) blocks: RefCell<Vec<ReportBlock>>,
}

impl ReportText for FakeReportText {
    fn report_text(&self, block: &ReportBlock) -> String {
        self.blocks.borrow_mut().push(block.clone());
        format!("report of {}", block.reference)
    }
}

/// One of every fake port, owned together so a test can lend them all at once.
///
/// Two deliveries, as the product has: `direct` stands for `DirectInjection`
/// and `by_clipboard` for `ClipboardDelivery`. The fake clipboard is the one
/// the report block writes to, and `by_clipboard` keeps its own record of the
/// values - so a test can tell the two doors apart.
pub(crate) struct Kit {
    pub(crate) direct: FakeDelivery,
    pub(crate) by_clipboard: FakeDelivery,
    pub(crate) keys: FakeKeys,
    pub(crate) clipboard: FakeClipboard,
    pub(crate) text: FakeReportText,
}

impl Kit {
    pub(crate) fn ready() -> Self {
        Self {
            direct: FakeDelivery::ready(),
            by_clipboard: FakeDelivery::ready(),
            keys: FakeKeys::working(),
            clipboard: FakeClipboard::working(),
            text: FakeReportText {
                blocks: RefCell::new(Vec::new()),
            },
        }
    }
    pub(crate) fn with_delivery(direct: FakeDelivery) -> Self {
        Self {
            direct,
            ..Self::ready()
        }
    }
    pub(crate) fn with_by_clipboard(by_clipboard: FakeDelivery) -> Self {
        Self {
            by_clipboard,
            ..Self::ready()
        }
    }
    pub(crate) fn with_keys(keys: FakeKeys) -> Self {
        Self {
            keys,
            ..Self::ready()
        }
    }
    pub(crate) fn with_clipboard(clipboard: FakeClipboard) -> Self {
        Self {
            clipboard,
            ..Self::ready()
        }
    }
    pub(crate) fn ports(&self) -> Ports<'_> {
        Ports {
            direct: &self.direct,
            by_clipboard: &self.by_clipboard,
            keys: &self.keys,
            clipboard: &self.clipboard,
            report_text: &self.text,
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

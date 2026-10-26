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

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::advance_sequence::{AdvanceSequence, InFlight, Ports};
use crate::ports::{
    Availability, Clipboard, ClipboardError, Date, Delivered, DeliveryError, Following, History,
    KeystrokeError, KeystrokeSender, PackFormat, PackSource, Progress, ReportText, SourceError,
    TargetInspector, TargetRef, TranslationCheck, TranslationTarget, ValueDelivery, Waiting,
};
use nkb_core::calendar::Day;
use nkb_core::keys::KeyChord;
use nkb_core::lint::{LintProblem, RuleCode};
use nkb_core::pack::{Pack, PackValue, Risk};
use nkb_core::report::{ControlKind, ReportBlock, Target};
use nkb_core::substitutions::Substitutions;
use nkb_core::text::LiteralText;
use nkb_core::value::ValueBody;

/// A delivery whose availability, target and outcome a test dictates, and
/// which keeps every text it was handed - so a test can say which ROUTE a value
/// went by, which is the whole question once there are two.
pub(crate) struct FakeDelivery {
    availability: Availability,
    /// A `Cell`, so a test can bring another window to the front between two
    /// presses - or between two idle ticks - without rebuilding the ports.
    target: Cell<Option<TargetRef>>,
    fail: Option<DeliveryError>,
    /// What a successful delivery says about following the application
    /// (`D95`, `D145`): the application unless a test asks for another route.
    following: Following,
    /// The text services the route says could have changed what it typed
    /// (`D139`): none unless a test asks.
    altered: Substitutions,
    /// What the route says on the way, in order, before it answers - a long
    /// send as the real route reports it (`OBS-160`). Empty: a send too short
    /// to report.
    reports: Vec<Progress>,
    pub(crate) handed: RefCell<Vec<String>>,
    /// The target each text was aimed at, in the order `handed` keeps them.
    pub(crate) aimed: RefCell<Vec<TargetRef>>,
    /// How many times the target was asked for (`D148`: once per send).
    pub(crate) targets_asked: Cell<usize>,
}

impl FakeDelivery {
    pub(crate) fn ready() -> Self {
        Self {
            availability: Availability::Ready,
            target: Cell::new(Some(TargetRef(1))),
            fail: None,
            following: Following::Application,
            altered: Substitutions::NONE,
            reports: Vec::new(),
            handed: RefCell::new(Vec::new()),
            aimed: RefCell::new(Vec::new()),
            targets_asked: Cell::new(0),
        }
    }
    /// A route on a system whose text services are on and could act on what it
    /// types.
    pub(crate) fn altering(altered: Substitutions) -> Self {
        Self {
            altered,
            ..Self::ready()
        }
    }
    /// A route that says how far it got, `reports` times, before answering.
    pub(crate) fn reporting(self, reports: &[Progress]) -> Self {
        Self {
            reports: reports.to_vec(),
            ..self
        }
    }
    /// A route that delivers but could not follow the application's queue.
    pub(crate) fn unpaced() -> Self {
        Self {
            following: Following::System,
            ..Self::ready()
        }
    }
    /// A route that hands the value to the desktop's input method.
    pub(crate) fn handing_over() -> Self {
        Self {
            following: Following::HandedOver,
            ..Self::ready()
        }
    }
    pub(crate) fn without_target() -> Self {
        Self {
            target: Cell::new(None),
            ..Self::ready()
        }
    }
    /// Another window comes to the front.
    pub(crate) fn set_target(&self, target: Option<TargetRef>) {
        self.target.set(target);
    }
    pub(crate) fn failing(error: DeliveryError) -> Self {
        Self {
            fail: Some(error),
            ..Self::ready()
        }
    }
    /// A route the system holds shut until the person allows it - known before
    /// any send, and found again at the send if it was taken back: the
    /// accessibility permission on macOS (`D138`).
    pub(crate) fn needs_permission() -> Self {
        Self {
            availability: Availability::NeedsPermission,
            fail: Some(DeliveryError::NeedsPermission),
            ..Self::ready()
        }
    }
    /// No route on this system, known before any send - what `DirectInjection`
    /// answers on Linux today.
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
        self.targets_asked.set(self.targets_asked.get() + 1);
        self.target.get()
    }
    fn deliver(
        &self,
        text: &str,
        target: TargetRef,
        progress: &mut dyn FnMut(Progress),
    ) -> Result<Delivered, DeliveryError> {
        self.handed.borrow_mut().push(text.to_owned());
        self.aimed.borrow_mut().push(target);
        for report in &self.reports {
            progress(*report);
        }
        match &self.fail {
            None => Ok(Delivered {
                utf16_units: text.encode_utf16().count(),
                following: self.following,
            }),
            Some(error) => Err(error.clone()),
        }
    }
    fn may_alter(&self, _text: &str) -> Substitutions {
        self.altered
    }
}

/// Keys that go through or refuse as a test says, and count the requests - so
/// a test can prove that a route pressed NOTHING.
pub(crate) struct FakeKeys {
    fail: Option<KeystrokeError>,
    /// What a press says it waits for before its first key.
    says: Vec<Waiting>,
    pub(crate) requests: RefCell<usize>,
    /// The target each request was aimed at.
    pub(crate) aimed: RefCell<Vec<TargetRef>>,
}

impl FakeKeys {
    pub(crate) fn working() -> Self {
        Self {
            fail: None,
            says: Vec::new(),
            requests: RefCell::new(0),
            aimed: RefCell::new(Vec::new()),
        }
    }
    pub(crate) fn failing(error: KeystrokeError) -> Self {
        Self {
            fail: Some(error),
            says: Vec::new(),
            requests: RefCell::new(0),
            aimed: RefCell::new(Vec::new()),
        }
    }
    /// A press that stands on a window of the system, and says so (`D145` point 76).
    pub(crate) fn asking_the_person() -> Self {
        Self {
            says: vec![Waiting::Asking, Waiting::ForConsent, Waiting::Answered],
            ..Self::working()
        }
    }
}

impl KeystrokeSender for FakeKeys {
    fn send_keystrokes(
        &self,
        _chords: &[KeyChord],
        target: TargetRef,
        waiting: &mut dyn FnMut(Waiting),
    ) -> Result<(), KeystrokeError> {
        *self.requests.borrow_mut() += 1;
        self.aimed.borrow_mut().push(target);
        for said in &self.says {
            waiting(*said);
        }
        match &self.fail {
            None => Ok(()),
            Some(error) => Err(error.clone()),
        }
    }
}

/// An inspector that answers what a test dictates and counts the questions -
/// so a test can prove the clipboard route asked nothing (`D104`).
pub(crate) struct FakeInspector {
    pub(crate) answer: Target,
    pub(crate) asked: RefCell<Vec<TargetRef>>,
}

impl FakeInspector {
    pub(crate) fn answering(answer: Target) -> Self {
        Self {
            answer,
            asked: RefCell::new(Vec::new()),
        }
    }
}

impl TargetInspector for FakeInspector {
    fn inspect(&self, target: TargetRef) -> Target {
        self.asked.borrow_mut().push(target);
        self.answer.clone()
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
    pub(crate) inspector: FakeInspector,
    pub(crate) clipboard: FakeClipboard,
    pub(crate) text: FakeReportText,
    /// Every value heard on its way into the field, in order (`OBS-160`).
    pub(crate) heard: Rc<RefCell<Vec<InFlight>>>,
    /// What the ports lend as `progress`: it writes into `heard`. Boxed and
    /// sharing `heard` through an `Rc`, so the kit does not borrow itself.
    pub(crate) ear: Box<dyn Fn(InFlight)>,
    /// Every wait for the person heard before the first key, in order (`D145` point 76).
    pub(crate) waited: Rc<RefCell<Vec<Waiting>>>,
    /// What the ports lend as `waiting`: it writes into `waited`.
    pub(crate) listening: Box<dyn Fn(Waiting)>,
}

impl Kit {
    pub(crate) fn ready() -> Self {
        let heard = Rc::new(RefCell::new(Vec::new()));
        let into = Rc::clone(&heard);
        let waited = Rc::new(RefCell::new(Vec::new()));
        let into_waited = Rc::clone(&waited);
        Self {
            direct: FakeDelivery::ready(),
            by_clipboard: FakeDelivery::ready(),
            keys: FakeKeys::working(),
            inspector: FakeInspector::answering(Target {
                program: Some(String::from("notepad.exe")),
                field: ControlKind::TextField,
            }),
            clipboard: FakeClipboard::working(),
            text: FakeReportText {
                blocks: RefCell::new(Vec::new()),
            },
            heard,
            ear: Box::new(move |in_flight| into.borrow_mut().push(in_flight)),
            waited,
            listening: Box::new(move |waiting| into_waited.borrow_mut().push(waiting)),
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
    pub(crate) fn with_inspector(answer: Target) -> Self {
        Self {
            inspector: FakeInspector::answering(answer),
            ..Self::ready()
        }
    }
    pub(crate) fn ports(&self) -> Ports<'_> {
        Ports {
            direct: &self.direct,
            by_clipboard: &self.by_clipboard,
            keys: &self.keys,
            inspector: &self.inspector,
            clipboard: &self.clipboard,
            report_text: &self.text,
            progress: &*self.ear,
            waiting: &*self.listening,
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
        definition: ValueBody::Literal(LiteralText::new(text.to_owned())).into(),
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
    let mut advance = AdvanceSequence::new(Box::new(StoppedClock::default()));
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

/// 2026-10-15, the day `next-date` was written: the autumn clock change of 2026
/// is still ahead, the spring one of 2027 is next.
pub(crate) const WRITTEN_ON: Day = Day::from_days_since_1970(20_741);

/// A clock stopped on one day, so that a value of type `next-date` names the
/// same date in every run.
#[derive(Debug, Clone, Copy)]
pub(crate) struct StoppedClock(pub(crate) Day);

impl Default for StoppedClock {
    fn default() -> Self {
        Self(WRITTEN_ON)
    }
}

impl crate::ports::Clock for StoppedClock {
    fn today(&self) -> Day {
        self.0
    }
}

/// A clock a test moves between two presses: a palette stays open across
/// midnight, and a value of type `next-date` has to notice.
#[derive(Debug, Clone)]
pub(crate) struct MovingClock(std::sync::Arc<std::sync::atomic::AtomicI64>);

impl MovingClock {
    pub(crate) fn on(day: Day) -> Self {
        Self(std::sync::Arc::new(std::sync::atomic::AtomicI64::new(
            day.days_since_1970(),
        )))
    }

    pub(crate) fn set(&self, day: Day) {
        self.0
            .store(day.days_since_1970(), std::sync::atomic::Ordering::SeqCst);
    }
}

impl crate::ports::Clock for MovingClock {
    fn today(&self) -> Day {
        Day::from_days_since_1970(self.0.load(std::sync::atomic::Ordering::SeqCst))
    }
}

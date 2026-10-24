//! Whether the palette's shortcuts are held by this process now, and the
//! registrar that takes them.
//!
//! # A set in which nothing took is not held (`OBS-176`)
//!
//! A second palette finds every shortcut held by the first. Until 2026-10-14
//! that set was held anyway and the sequence waited on it - and the Windows
//! listener, which does not pump for a set where nothing registered, had
//! already ended its thread. The wait answered `Wait::Gone`, and the palette
//! wrote "the shortcut listener has stopped, restart the palette" over the
//! sentences saying the shortcuts were taken. A restart changed nothing while
//! the first palette ran.
//!
//! Nothing here knows about that listener: a set in which no shortcut took can
//! never be pressed, whatever stands behind it
//! ([`LiveShortcuts::holds_any`]), and that alone is reason not to wait on it.
//! It is [`Holding::Lost`], the state that already meant "nothing could be
//! registered", and a resume tries again as it does there.
//!
//! # A desktop that gives the shortcuts answers when a person has chosen
//!
//! On a Wayland session the desktop holds the keys, and the first time it shows
//! a window of its own that waits for the person, up to five minutes
//! (`PortalShortcuts`). The worker is the one thread that carries out the
//! tester's clicks and sees the palette close, so it cannot stand in that call:
//! the palette would show nothing, answer nothing, and a close would wait for the
//! window of the system. The call goes to a thread of its own, the hold is
//! [`Holding::Asking`] meanwhile, and the worker reads the answer between its
//! other waits ([`Hold::arrived`]). Only the actions that do something in this
//! build are asked for, because the window of the system lists every one of them.

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, TryRecvError};

use nkb_adapters::i18n::{self, PaletteLabel};
use nkb_adapters::{PortalEntry, PortalShortcuts};
use nkb_app::ports::{
    DesktopRefusal, EntryNews, HotkeyRegistrar, LiveShortcuts, ProgramEntry, ShortcutRegistration,
    ShortcutsUnavailable,
};
use nkb_core::hotkeys::{Bindings, HotkeyAction, HotkeyChord};

use super::{Apart, DesktopNow, ShortcutLink};

/// What a registrar answered.
type Answer = Result<Box<dyn LiveShortcuts + Send>, ShortcutsUnavailable>;

/// A registrar that may be asked from a thread of its own.
type Desktop = Arc<dyn HotkeyRegistrar + Send + Sync>;

/// The entry that tells that desktop which program is asking.
type Entry = Arc<dyn ProgramEntry + Send + Sync>;

/// Whether the palette's shortcuts are held by this process now.
enum Holding {
    /// Registered, and driving the sequence.
    Held(Box<dyn LiveShortcuts + Send>),
    /// Asked of a desktop that has not answered: the person is choosing. Nothing
    /// is held and nothing can be pressed yet.
    Asking {
        answers: Receiver<Answer>,
        /// The table asked for, for the sentences about the answer.
        bindings: Bindings,
    },
    /// Given back to the system while the shortcuts window is open.
    Paused,
    /// Not held, and not because of the window: nothing could be registered,
    /// none of the shortcuts took, or the thread behind them ended. Said when
    /// it happened. A [`Command::Resume`](super::Command::Resume) tries again.
    Lost,
}

/// The palette's shortcuts as this thread holds them, and the registrar that
/// takes them.
pub(super) struct Hold<'a> {
    /// The registrar of the shortcuts window's trial, and of every registration
    /// when no desktop holds the keys.
    pub(super) registrar: &'a dyn HotkeyRegistrar,
    /// The desktop to ask instead, when it holds the keys (see the module).
    desktop: Option<Desktop>,
    /// The entry that same desktop wants before it gives a program shortcuts, when
    /// it is one that does (`D145` point 68).
    entry: Option<Entry>,
    /// The desktop has said it does not know the program, and nothing has been asked
    /// of it since. The offer to make the entry stands for as long as this is true.
    needs_entry: bool,
    /// What was said about the entry while the desktop was being asked again, to say
    /// once more in front of what the desktop answers: an answer that comes at once
    /// would otherwise take the line off the screen before it was read.
    after_answer: Vec<String>,
    /// Why the last attempt to take the shortcuts gave none, as the sentence the
    /// palette said once. The shortcuts window says it for as long as it holds, because
    /// a person who opens it later is asking why the shortcuts do not answer.
    refusal: Option<String>,
    holding: Holding,
    /// What the last registration said, chord by chord - see
    /// [`ShortcutsNow`](super::ShortcutsNow).
    pub(super) registered: Vec<(HotkeyAction, HotkeyChord, ShortcutRegistration)>,
}

impl<'a> Hold<'a> {
    pub(super) const fn new(registrar: &'a dyn HotkeyRegistrar) -> Self {
        Self {
            registrar,
            desktop: None,
            entry: None,
            needs_entry: false,
            after_answer: Vec::new(),
            refusal: None,
            holding: Holding::Lost,
            registered: Vec::new(),
        }
    }

    /// The hold for the session this runs in: `registrar` where a program may grab
    /// a key, and the portal of the desktop where only the desktop may.
    pub(super) fn here(registrar: &'a dyn HotkeyRegistrar) -> Self {
        let hold = Self::new(registrar);
        if PortalShortcuts::is_the_route() {
            hold.asking(Arc::new(PortalShortcuts))
                .with_entry(Arc::new(PortalEntry))
        } else {
            hold
        }
    }

    /// This hold asks `desktop` for its shortcuts, on a thread of its own.
    pub(super) fn asking(mut self, desktop: Desktop) -> Self {
        self.desktop = Some(desktop);
        self
    }

    /// This hold offers to make `entry` when the desktop it asks does not know the
    /// program.
    pub(super) fn with_entry(mut self, entry: Entry) -> Self {
        self.entry = Some(entry);
        self
    }

    pub(super) fn live(&self) -> Option<&dyn LiveShortcuts> {
        match &self.holding {
            Holding::Held(live) => Some(live.as_ref()),
            Holding::Asking { .. } | Holding::Paused | Holding::Lost => None,
        }
    }

    pub(super) const fn paused(&self) -> bool {
        matches!(self.holding, Holding::Paused)
    }

    /// Whether a desktop has been asked and has not answered.
    pub(super) const fn asking_now(&self) -> bool {
        matches!(self.holding, Holding::Asking { .. })
    }

    /// Whether a desktop holds the shortcuts here, and the person chooses them there.
    pub(super) const fn on_the_desktop(&self) -> bool {
        self.desktop.is_some()
    }

    /// What the desktop holds for the palette now, for the shortcuts window - and
    /// `None` where no desktop does.
    ///
    /// The sentence that stands is the one every view says while the shortcuts are
    /// held back, and failing that the reason the last attempt gave none.
    pub(super) fn desktop_now(&self) -> Option<DesktopNow> {
        self.desktop.as_ref()?;
        Some(DesktopNow {
            given: self
                .live()
                .map_or_else(Vec::new, |live| live.given().to_vec()),
            waiting: self.asking_now(),
            standing: self.held_back().or_else(|| self.refusal.clone()),
        })
    }

    /// The sentence every view says first while the shortcuts are not in hand for
    /// a reason that will pass: given back to the system, not yet given, or not
    /// given because the desktop does not know the program and the person can
    /// change that. A line said once would last one view - the band is rebuilt
    /// each time.
    pub(super) fn held_back(&self) -> Option<String> {
        let label = match self.holding {
            Holding::Paused => PaletteLabel::ShortcutsPaused,
            Holding::Asking { .. } => PaletteLabel::WaitingForTheDesktop,
            Holding::Lost if self.needs_entry => return Some(self.unknown_to_the_desktop()),
            Holding::Held(_) | Holding::Lost => return None,
        };
        Some(i18n::label(label).to_owned())
    }

    /// What is said while the desktop does not know the program: the offer to make
    /// the entry, or - when the entry is in place and the desktop still does not
    /// know it - that there is nothing left to offer.
    fn unknown_to_the_desktop(&self) -> String {
        if self.entry.as_ref().is_some_and(|entry| entry.ours()) {
            i18n::label(PaletteLabel::EntryInPlace).to_owned()
        } else {
            i18n::shortcuts_unavailable(&ShortcutsUnavailable::Desktop(DesktopRefusal::NeedsEntry))
        }
    }

    /// The links under the switches: the way into the shortcuts window always, the
    /// entry the desktop wants while it does not know the program and none is
    /// there, and the way to take back an entry this program made - not while the
    /// person is choosing in the window of the system, which that would outlive.
    pub(super) fn links(&self) -> Vec<ShortcutLink> {
        let mut links = vec![ShortcutLink::Window];
        if let Some(entry) = &self.entry {
            let ours = entry.ours();
            if self.needs_entry && !ours {
                links.push(ShortcutLink::EnableEntry);
            }
            if ours && !self.asking_now() {
                links.push(ShortcutLink::RemoveEntry);
            }
        }
        links
    }

    /// Makes the entry, when the offer stands: the desktop said it does not know the
    /// program and no entry of ours is there. `None` for a click that comes late,
    /// after the offer is gone.
    pub(super) fn make_entry(&mut self) -> Option<EntryNews> {
        let entry = self.entry.clone()?;
        (self.needs_entry && !entry.ours()).then(|| entry.make())
    }

    /// Takes away the entry this program made. Shortcuts that were given are let go
    /// with it, so the palette does not claim a route it has taken the ground from,
    /// and the offer to make it again stands. `None` for an entry that is not ours,
    /// and while the person is choosing in the window of the system.
    pub(super) fn remove_entry(&mut self) -> Option<EntryNews> {
        let entry = self.entry.clone()?;
        if self.asking_now() || !entry.ours() {
            return None;
        }
        let news = entry.take_away();
        if news == EntryNews::TakenAway && matches!(self.holding, Holding::Held(_)) {
            self.holding = Holding::Lost;
            self.registered.clear();
            self.needs_entry = true;
        }
        Some(news)
    }

    /// The table the words of the palette follow, and the shortcuts they cannot
    /// name: what a desktop that chooses the chords gave, on top of `asked`.
    ///
    /// The chord it gave replaces the chord asked for wherever the palette reads
    /// it, so a chord the person changed in the system's own window is the one
    /// the hint bar and the sentences show. A shortcut it gave in words that are
    /// no chord of ours, and one it gave none of, are [`Apart`]. Where no desktop
    /// is asked there is nothing given, and the table is `asked` as it was.
    ///
    /// The shortcuts that are [`Apart`] are marked in the table as well
    /// ([`Bindings::unnamed`]), so a sentence that reads the table cannot name a key
    /// for them whatever it is (`OBS-225`).
    pub(super) fn as_given(&self, asked: Bindings) -> (Bindings, Vec<(HotkeyAction, Apart)>) {
        let given = self.live().map_or(&[][..], |live| live.given());
        let chords: Vec<(HotkeyAction, HotkeyChord)> = given
            .iter()
            .filter_map(|(action, one)| one.chord.map(|chord| (*action, chord)))
            .collect();
        let worded = given
            .iter()
            .filter(|(_, one)| one.chord.is_none())
            .map(|(action, one)| (*action, Apart::Worded(one.words.clone())));
        let left = self
            .registered
            .iter()
            .filter(|(_, _, outcome)| *outcome == ShortcutRegistration::NotGiven)
            .map(|(action, _, _)| (*action, Apart::Left));
        let apart: Vec<(HotkeyAction, Apart)> = worded.chain(left).collect();
        let unnamed: Vec<HotkeyAction> = apart.iter().map(|(action, _)| *action).collect();
        (asked.as_given(&chords).unnamed(&unnamed), apart)
    }

    /// Gives the shortcuts back. Dropping the handle is the release, and it
    /// returns only once the system has let them go.
    pub(super) fn pause(&mut self) {
        self.holding = Holding::Paused;
        self.after_answer.clear();
    }

    /// Says `lines` once more when the desktop answers what it is being asked now.
    pub(super) fn say_on_answer(&mut self, lines: Vec<String>) {
        self.after_answer = lines;
    }

    /// The thread behind the shortcuts has ended.
    pub(super) fn lose(&mut self) {
        self.holding = Holding::Lost;
    }

    /// Registers `bindings` in place of whatever is held, and the lines worth
    /// saying about it.
    ///
    /// 🔴 What is held is released FIRST. The old set holds the very chords the
    /// new one asks for, so the other order would come back `Taken`, against
    /// ourselves, for every chord that did not move.
    ///
    /// A registration that worked says nothing, and every shortcut taken is
    /// one sentence - both are `i18n::registrations` deciding, not this
    /// module. A set in which nothing took is let go here and stays
    /// [`Holding::Lost`]: see the module.
    ///
    /// Of a desktop the answer is not here yet: it says nothing and the hold is
    /// [`Holding::Asking`], until [`Hold::arrived`] has it.
    pub(super) fn take(&mut self, bindings: &Bindings) -> Vec<String> {
        self.holding = Holding::Lost;
        self.needs_entry = false;
        self.refusal = None;
        self.registered.clear();
        match self.desktop.clone() {
            Some(desktop) => self.ask(desktop, *bindings),
            None => {
                let answer = self.registrar.register(bindings.as_slice());
                self.settle(answer, bindings)
            }
        }
    }

    /// Asks `desktop` for the shortcuts that do something, on a thread of its own.
    /// A thread that cannot start is the same refusal as a listener that cannot.
    fn ask(&mut self, desktop: Desktop, bindings: Bindings) -> Vec<String> {
        let wanted: Vec<(HotkeyAction, HotkeyChord)> = bindings
            .as_slice()
            .iter()
            .copied()
            .filter(|(action, _)| super::wired(*action))
            .collect();
        let (sender, answers) = mpsc::channel();
        // Not joined: it ends when the desktop has answered or given up, which can
        // be after the palette is gone. The answer then has no one to go to and is
        // dropped, which closes its session.
        let started = std::thread::Builder::new()
            .name(String::from("nkb-desktop-shortcuts"))
            .spawn(move || {
                let _ = sender.send(desktop.register(&wanted));
            });
        if started.is_err() {
            return self.refused(&ShortcutsUnavailable::CouldNotStart);
        }
        self.holding = Holding::Asking { answers, bindings };
        Vec::new()
    }

    /// The answer of the desktop and the lines worth saying about it, once the
    /// person has given it - and `None` while they have not. The hold is what the
    /// answer makes of it, as after [`Hold::take`].
    pub(super) fn arrived(&mut self) -> Option<Vec<String>> {
        let Holding::Asking { answers, bindings } = &self.holding else {
            return None;
        };
        let answer = match answers.try_recv() {
            Ok(answer) => answer,
            Err(TryRecvError::Empty) => return None,
            // The thread ended without answering, which only a panic does.
            Err(TryRecvError::Disconnected) => Err(ShortcutsUnavailable::CouldNotStart),
        };
        let bindings = *bindings;
        self.holding = Holding::Lost;
        let mut said = std::mem::take(&mut self.after_answer);
        said.extend(self.settle(answer, &bindings));
        Some(said)
    }

    /// What a registrar answered, made the hold and the lines about it.
    fn settle(&mut self, answer: Answer, bindings: &Bindings) -> Vec<String> {
        match answer {
            Ok(live) => {
                self.registered = live
                    .outcomes()
                    .iter()
                    .map(|(action, outcome)| (*action, bindings.chord(*action), *outcome))
                    .collect();
                if live.holds_any() {
                    self.holding = Holding::Held(live);
                }
                i18n::registrations(&self.registered)
            }
            // The offer to make the entry is said by `held_back` for as long as it
            // stands, not once: it is the sentence the link under the switches needs.
            Err(ShortcutsUnavailable::Desktop(DesktopRefusal::NeedsEntry))
                if self.entry.is_some() =>
            {
                self.registered.clear();
                self.needs_entry = true;
                Vec::new()
            }
            Err(error) => {
                self.registered.clear();
                self.refused(&error)
            }
        }
    }

    /// The sentence for a refusal, kept to be said again by the window.
    fn refused(&mut self, error: &ShortcutsUnavailable) -> Vec<String> {
        let said = i18n::shortcuts_unavailable(error);
        self.refusal = Some(said.clone());
        vec![said]
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::mpsc::{self, Receiver, Sender};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use nkb_adapters::i18n::{self, PaletteLabel};
    use nkb_app::ports::{
        DesktopRefusal, EntryNews, EntryRefusal, GivenShortcut, HotkeyRegistrar, LiveShortcuts,
        ProgramEntry, ShortcutRegistration, ShortcutsUnavailable, Wait,
    };
    use nkb_core::hotkeys::{HotkeyAction, HotkeyChord};

    use super::{Answer, Hold};
    use crate::live::tests::{Registrar, a_store, paused_line, with_worker};
    use crate::live::{
        Command, DesktopNow, Next, ShortcutLink, ShortcutsNow, Told, legend, next_command, wired,
    };

    /// What a person at the window of the desktop decides.
    enum Verdict {
        Give,
        /// Gives what was asked for less `left_out`, and holds `held`, which
        /// the person may have changed in the window of the system.
        Keep {
            left_out: Vec<HotkeyAction>,
            held: Vec<(HotkeyAction, GivenShortcut)>,
        },
        Refuse(ShortcutsUnavailable),
    }

    /// A desktop that answers when the test lets it, as a person does: the call
    /// to register stays standing until a verdict is sent - or four seconds go,
    /// so a call made where it should not be fails a test and does not hang it. It keeps what it was
    /// asked, and counts the handles it gave that are alive.
    struct Person {
        verdicts: Mutex<Receiver<Verdict>>,
        asked: Mutex<Vec<Vec<(HotkeyAction, HotkeyChord)>>>,
        /// How many handles were ever given, and how many are alive.
        gave: AtomicUsize,
        alive: Arc<AtomicUsize>,
    }

    struct Given {
        outcomes: Vec<(HotkeyAction, ShortcutRegistration)>,
        /// What the desktop says it holds, in its own words.
        held: Vec<(HotkeyAction, GivenShortcut)>,
        alive: Arc<AtomicUsize>,
    }

    impl Drop for Given {
        fn drop(&mut self) {
            self.alive.fetch_sub(1, Ordering::SeqCst);
        }
    }

    impl LiveShortcuts for Given {
        fn outcomes(&self) -> &[(HotkeyAction, ShortcutRegistration)] {
            &self.outcomes
        }
        fn next(&self, _: Duration) -> Wait {
            Wait::Nothing
        }
        fn given(&self) -> &[(HotkeyAction, GivenShortcut)] {
            &self.held
        }
    }

    impl Person {
        /// The handle of a desktop that gave what was asked for, less
        /// `left_out`, and says in `held` what it holds.
        fn gives(
            &self,
            asked: &[(HotkeyAction, HotkeyChord)],
            left_out: &[HotkeyAction],
            held: Vec<(HotkeyAction, GivenShortcut)>,
        ) -> Answer {
            self.alive.fetch_add(1, Ordering::SeqCst);
            self.gave.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(Given {
                outcomes: asked
                    .iter()
                    .map(|(action, _)| {
                        let outcome = if left_out.contains(action) {
                            ShortcutRegistration::NotGiven
                        } else {
                            ShortcutRegistration::Registered
                        };
                        (*action, outcome)
                    })
                    .collect(),
                held,
                alive: Arc::clone(&self.alive),
            }))
        }
    }

    impl HotkeyRegistrar for Person {
        fn register(&self, bindings: &[(HotkeyAction, HotkeyChord)]) -> Answer {
            self.asked
                .lock()
                .expect("the test holds the lock briefly")
                .push(bindings.to_vec());
            let verdict = self
                .verdicts
                .lock()
                .expect("the test holds the lock briefly")
                .recv_timeout(Duration::from_secs(4));
            match verdict {
                Ok(Verdict::Give) => self.gives(bindings, &[], Vec::new()),
                Ok(Verdict::Keep { left_out, held }) => self.gives(bindings, &left_out, held),
                Ok(Verdict::Refuse(error)) => Err(error),
                Err(_) => Err(ShortcutsUnavailable::CouldNotStart),
            }
        }
    }

    fn a_person() -> (Arc<Person>, Sender<Verdict>) {
        let (decide, verdicts) = mpsc::channel();
        let person = Arc::new(Person {
            verdicts: Mutex::new(verdicts),
            asked: Mutex::default(),
            gave: AtomicUsize::default(),
            alive: Arc::default(),
        });
        (person, decide)
    }

    impl Person {
        fn times_asked(&self) -> usize {
            self.asked.lock().expect("brief lock").len()
        }
        fn alive(&self) -> usize {
            self.alive.load(Ordering::SeqCst)
        }
    }

    /// Reads the answer the way the worker does, until it comes or a few seconds
    /// have gone - the call runs on a thread of its own.
    fn answer_of(hold: &mut Hold<'_>) -> Vec<String> {
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(lines) = hold.arrived() {
                return lines;
            }
            assert!(Instant::now() < until, "the desktop never answered");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    /// Waits until the desktop has been asked `times` times: the question runs on
    /// a thread that may not have started when the call that began it returns.
    fn wait_asked(person: &Person, times: usize) {
        let until = Instant::now() + Duration::from_secs(5);
        while person.times_asked() < times {
            assert!(Instant::now() < until, "the desktop was not asked");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn waiting_line() -> String {
        i18n::label(PaletteLabel::WaitingForTheDesktop).to_owned()
    }

    /// Not held because nothing could be registered is a state the worker
    /// stays in, saying why - and the next resume tries again. Until K5.3 the
    /// worker ended there.
    #[test]
    fn with_nothing_registrable_the_worker_says_why_and_a_resume_tries_again() {
        let store = a_store(false);
        let registrar = Registrar::default();
        registrar.unavailable.set(true);
        with_worker(&store, &registrar, false, |worker, memory| {
            let lines = worker.take(memory.bindings.get());
            assert_eq!(
                lines,
                vec![i18n::shortcuts_unavailable(
                    &ShortcutsUnavailable::CouldNotStart
                )]
            );
            assert!(worker.hold.live().is_none());
            assert!(
                !worker.hold.paused(),
                "lost is not paused, and must not say it is"
            );

            let chosen = worker
                .carry_out(Command::Choose(String::from("unicode-text")))
                .view
                .expect("a pack is still chosen");
            assert_eq!(chosen.pack, "Unicode and text");

            registrar.unavailable.set(false);
            worker.carry_out(Command::Resume);
            assert!(worker.hold.live().is_some(), "the resume did not try again");
        });
    }

    /// `OBS-176`: a second palette, every shortcut held by the first. Nothing
    /// is held, so the sequence never waits on a set that cannot be pressed,
    /// and one sentence says why. The window still learns which chords are
    /// taken, and once they come free a resume holds them.
    #[test]
    fn with_every_shortcut_taken_nothing_is_held_and_one_sentence_says_why() {
        let store = a_store(false);
        let registrar = Registrar::default();
        let chords = nkb_adapters::default_bindings();
        registrar
            .taken
            .borrow_mut()
            .extend(chords.as_slice().iter().map(|(_, chord)| *chord));
        with_worker(&store, &registrar, false, |worker, memory| {
            let lines = worker.take(memory.bindings.get());
            assert_eq!(lines.len(), 1, "{lines:?}");
            assert!(
                lines[0].starts_with("Every shortcut is already taken"),
                "{lines:?}"
            );
            assert!(worker.hold.live().is_none(), "a set where nothing took");
            assert!(!worker.hold.paused());
            assert_eq!(worker.hold.registered.len(), chords.as_slice().len());

            registrar.taken.borrow_mut().clear();
            worker.carry_out(Command::Resume);
            assert!(worker.hold.live().is_some(), "the resume did not try again");
        });
    }

    /// A desktop answers when a person has chosen, which may be minutes. The
    /// worker must not stand in that call: it returns at once, nothing is held,
    /// every view says what it waits for, and the commands of the tester are
    /// carried out meanwhile. The answer then makes the hold what any
    /// registration does.
    #[test]
    fn a_desktop_is_asked_without_waiting_and_every_view_says_it_is_waited_on() {
        let store = a_store(false);
        let registrar = Registrar::default();
        let (person, decide) = a_person();
        with_worker(&store, &registrar, false, |worker, memory| {
            worker.hold.desktop = Some(person.clone());

            let lines = worker.take(memory.bindings.get());

            assert!(lines.is_empty(), "{lines:?}");
            assert!(worker.hold.asking_now());
            assert!(worker.hold.live().is_none(), "nothing is held yet");
            assert_eq!(worker.hold.held_back(), Some(waiting_line()));
            let chosen = worker
                .carry_out(Command::Choose(String::from("unicode-text")))
                .view
                .expect("a pack is chosen while the desktop is waited on");
            assert_eq!(chosen.pack, "Unicode and text");
            assert_eq!(chosen.messages.first(), Some(&waiting_line()));
            assert!(worker.hold.arrived().is_none(), "no one has chosen yet");

            decide
                .send(Verdict::Give)
                .expect("the desktop is listening");
            let lines = answer_of(&mut worker.hold);

            assert!(lines.is_empty(), "everything given says nothing: {lines:?}");
            assert!(worker.hold.live().is_some());
            assert!(!worker.hold.asking_now());
            assert_eq!(worker.hold.held_back(), None);
            let view = worker.answered(lines);
            assert!(
                !view.messages.contains(&waiting_line()),
                "{:?}",
                view.messages
            );
            assert!(
                view.legend.is_some(),
                "the hint bar is drawn from the table"
            );
        });
    }

    /// The window of the system lists every shortcut it is asked for, so a
    /// desktop is asked for those that do something in this build and no others.
    #[test]
    fn a_desktop_is_asked_only_for_the_shortcuts_that_work() {
        let store = a_store(false);
        let registrar = Registrar::default();
        let (person, decide) = a_person();
        with_worker(&store, &registrar, false, |worker, memory| {
            worker.hold.desktop = Some(person.clone());
            worker.take(memory.bindings.get());
            decide
                .send(Verdict::Give)
                .expect("the desktop is listening");
            answer_of(&mut worker.hold);

            let asked = person.asked.lock().expect("brief lock").clone();
            assert_eq!(asked.len(), 1);
            let actions: Vec<HotkeyAction> = asked[0].iter().map(|(action, _)| *action).collect();
            let working: Vec<HotkeyAction> = HotkeyAction::ALL
                .iter()
                .copied()
                .filter(|action| wired(*action))
                .collect();
            assert_eq!(actions, working);
            assert!(
                working.len() < HotkeyAction::ALL.len(),
                "some shortcut does nothing yet, or this proves nothing"
            );
            assert_eq!(worker.hold.registered.len(), working.len());
        });
    }

    /// A desktop that refuses is said once and the hold is lost, not waited on.
    /// A resume does not ask it again: the person was asked, the sentence says
    /// how to be asked once more, and a window closing must not put the question
    /// to them behind their back (`D145` point 70).
    #[test]
    fn a_desktop_that_refuses_is_said_once_and_a_resume_does_not_ask_it_again() {
        let store = a_store(false);
        let registrar = Registrar::default();
        let (person, decide) = a_person();
        with_worker(&store, &registrar, false, |worker, memory| {
            worker.hold.desktop = Some(person.clone());
            worker.take(memory.bindings.get());
            let refusal = ShortcutsUnavailable::Desktop(DesktopRefusal::Closed);
            decide
                .send(Verdict::Refuse(refusal.clone()))
                .expect("the desktop is listening");

            let lines = answer_of(&mut worker.hold);

            assert_eq!(lines, vec![i18n::shortcuts_unavailable(&refusal)]);
            assert!(worker.hold.live().is_none());
            assert!(!worker.hold.asking_now(), "a refusal is not a wait");
            assert_eq!(worker.hold.held_back(), None);

            let carried = worker.carry_out(Command::Resume);
            assert!(carried.view.is_none() && carried.told.is_none());
            assert!(!worker.hold.asking_now(), "the resume asked again");
            std::thread::sleep(Duration::from_millis(30));
            assert_eq!(person.times_asked(), 1);
        });
    }

    /// An entry the test keeps in memory: made when asked, and ours once made.
    #[derive(Default)]
    struct Desk {
        ours: AtomicBool,
        made: AtomicUsize,
        taken: AtomicUsize,
        refuses: Mutex<Option<EntryRefusal>>,
    }

    impl ProgramEntry for Desk {
        fn ours(&self) -> bool {
            self.ours.load(Ordering::SeqCst)
        }
        fn make(&self) -> EntryNews {
            self.made.fetch_add(1, Ordering::SeqCst);
            if let Some(why) = *self.refuses.lock().expect("brief lock") {
                return EntryNews::Refused(why);
            }
            if self.ours.swap(true, Ordering::SeqCst) {
                EntryNews::AlreadyThere
            } else {
                EntryNews::Made(String::from("/home/a/applications/entry.desktop"))
            }
        }
        fn take_away(&self) -> EntryNews {
            self.taken.fetch_add(1, Ordering::SeqCst);
            if self.ours.swap(false, Ordering::SeqCst) {
                EntryNews::TakenAway
            } else {
                EntryNews::NotThere
            }
        }
    }

    fn made_line() -> String {
        i18n::entry_news(&EntryNews::Made(String::from(
            "/home/a/applications/entry.desktop",
        )))
    }

    fn the_offer() -> String {
        i18n::shortcuts_unavailable(&ShortcutsUnavailable::Desktop(DesktopRefusal::NeedsEntry))
    }

    /// Asks a desktop that does not know the program, with `desk` to make the entry.
    fn asked_in_vain(
        worker: &mut crate::live::Worker<'_>,
        memory: &crate::live::Memory<'_>,
        person: &Arc<Person>,
        decide: &Sender<Verdict>,
        desk: &Arc<Desk>,
    ) {
        worker.hold.desktop = Some(person.clone());
        worker.hold.entry = Some(desk.clone());
        worker.take(memory.bindings.get());
        decide
            .send(Verdict::Refuse(ShortcutsUnavailable::Desktop(
                DesktopRefusal::NeedsEntry,
            )))
            .expect("the desktop is listening");
        let lines = answer_of(&mut worker.hold);
        assert!(
            lines.is_empty(),
            "the offer is standing, not said once: {lines:?}"
        );
    }

    /// A desktop that does not know the program is not a refusal said once: the offer
    /// to make the entry stands in every view, with the link that makes it, and a click
    /// makes the entry and asks the desktop again. Once it answers, the link is the one
    /// that takes the entry away.
    #[test]
    fn a_desktop_that_does_not_know_the_program_is_offered_the_entry_and_asked_again() {
        let store = a_store(false);
        let registrar = Registrar::default();
        let (person, decide) = a_person();
        let desk = Arc::new(Desk::default());
        with_worker(&store, &registrar, false, |worker, memory| {
            asked_in_vain(worker, memory, &person, &decide, &desk);

            assert!(worker.hold.live().is_none());
            assert_eq!(worker.hold.held_back(), Some(the_offer()));
            assert_eq!(
                worker.hold.links(),
                vec![ShortcutLink::Window, ShortcutLink::EnableEntry]
            );
            let drawn = worker.answered(Vec::new()).legend;
            assert_eq!(
                drawn.expect("the words are drawn").links,
                worker.hold.links(),
                "the links go to the palette with the legend"
            );
            let chosen = worker
                .carry_out(Command::Choose(String::from("unicode-text")))
                .view
                .expect("a pack is chosen while the offer stands");
            assert_eq!(chosen.messages.first(), Some(&the_offer()));

            let view = worker
                .carry_out(Command::EnableEntry)
                .view
                .expect("a click on the link is answered");

            assert_eq!(desk.made.load(Ordering::SeqCst), 1);
            assert!(worker.hold.asking_now(), "the desktop is asked again");
            wait_asked(&person, 2);
            assert_eq!(view.messages, vec![waiting_line(), made_line()]);
            assert_eq!(
                worker.hold.links(),
                vec![ShortcutLink::Window],
                "nothing is taken away while the person is choosing in the window of the system"
            );
            assert!(worker.carry_out(Command::RemoveEntry).view.is_none());
            assert_eq!(desk.taken.load(Ordering::SeqCst), 0);

            decide
                .send(Verdict::Give)
                .expect("the desktop is listening");
            let lines = answer_of(&mut worker.hold);
            assert_eq!(
                lines,
                vec![made_line()],
                "said again, in front of the answer"
            );
            assert!(worker.hold.live().is_some());
            assert_eq!(worker.hold.held_back(), None);
            assert_eq!(
                worker.hold.links(),
                vec![ShortcutLink::Window, ShortcutLink::RemoveEntry]
            );
        });
    }

    /// A click that comes after the offer is gone changes nothing and draws nothing,
    /// and so does a click to take away an entry that is not ours.
    #[test]
    fn a_click_that_comes_late_changes_nothing() {
        let store = a_store(false);
        let registrar = Registrar::default();
        let desk = Arc::new(Desk::default());
        with_worker(&store, &registrar, false, |worker, _| {
            worker.hold.entry = Some(desk.clone());

            assert!(worker.carry_out(Command::EnableEntry).view.is_none());
            assert!(worker.carry_out(Command::RemoveEntry).view.is_none());
            assert_eq!(desk.made.load(Ordering::SeqCst), 0);
            assert_eq!(desk.taken.load(Ordering::SeqCst), 0);
        });
    }

    /// Taking the entry away lets the shortcuts that were given go, because the
    /// palette does not claim a route it has taken the ground from, and the offer to make
    /// the entry again stands in their place.
    #[test]
    fn taking_the_entry_away_lets_the_shortcuts_go_and_offers_it_again() {
        let store = a_store(false);
        let registrar = Registrar::default();
        let (person, decide) = a_person();
        let desk = Arc::new(Desk::default());
        desk.ours.store(true, Ordering::SeqCst);
        with_worker(&store, &registrar, false, |worker, memory| {
            worker.hold.desktop = Some(person.clone());
            worker.hold.entry = Some(desk.clone());
            worker.take(memory.bindings.get());
            decide
                .send(Verdict::Give)
                .expect("the desktop is listening");
            answer_of(&mut worker.hold);
            assert!(worker.hold.live().is_some());
            assert_eq!(person.alive(), 1);

            let view = worker
                .carry_out(Command::RemoveEntry)
                .view
                .expect("a click on the link is answered");

            assert_eq!(desk.taken.load(Ordering::SeqCst), 1);
            assert!(worker.hold.live().is_none(), "the shortcuts are let go");
            assert_eq!(person.alive(), 0, "and the session is closed");
            assert_eq!(
                view.messages,
                vec![the_offer(), i18n::entry_news(&EntryNews::TakenAway)]
            );
            assert_eq!(
                worker.hold.links(),
                vec![ShortcutLink::Window, ShortcutLink::EnableEntry]
            );
        });
    }

    /// An entry that is in place, and a desktop that still does not know the program,
    /// have nothing left to offer: the sentence says so and the way back is the only link.
    #[test]
    fn an_entry_in_place_that_the_desktop_does_not_know_is_not_offered_again() {
        let store = a_store(false);
        let registrar = Registrar::default();
        let (person, decide) = a_person();
        let desk = Arc::new(Desk::default());
        desk.ours.store(true, Ordering::SeqCst);
        with_worker(&store, &registrar, false, |worker, memory| {
            asked_in_vain(worker, memory, &person, &decide, &desk);

            assert_eq!(
                worker.hold.held_back(),
                Some(i18n::label(PaletteLabel::EntryInPlace).to_owned())
            );
            assert_eq!(
                worker.hold.links(),
                vec![ShortcutLink::Window, ShortcutLink::RemoveEntry]
            );
            assert!(worker.carry_out(Command::EnableEntry).view.is_none());
            assert_eq!(desk.made.load(Ordering::SeqCst), 0);
        });
    }

    /// An entry that could not be made is said, and the offer still stands: nothing was
    /// done, and nothing is asked of the desktop again.
    #[test]
    fn an_entry_that_could_not_be_made_is_said_and_the_offer_stands() {
        let store = a_store(false);
        let registrar = Registrar::default();
        let (person, decide) = a_person();
        let desk = Arc::new(Desk::default());
        *desk.refuses.lock().expect("brief lock") = Some(EntryRefusal::Io);
        with_worker(&store, &registrar, false, |worker, memory| {
            asked_in_vain(worker, memory, &person, &decide, &desk);

            let view = worker
                .carry_out(Command::EnableEntry)
                .view
                .expect("a refusal is said");

            assert_eq!(
                view.messages,
                vec![
                    the_offer(),
                    i18n::entry_news(&EntryNews::Refused(EntryRefusal::Io))
                ]
            );
            assert!(!worker.hold.asking_now());
            assert_eq!(person.times_asked(), 1);
            assert_eq!(
                worker.hold.links(),
                vec![ShortcutLink::Window, ShortcutLink::EnableEntry]
            );
        });
    }

    /// Where no desktop chooses the chords there is no entry to offer: a refusal that
    /// says the desktop does not know the program is said once, as any refusal is, and
    /// the only link is the way into the shortcuts window.
    #[test]
    fn without_an_entry_to_make_the_refusal_is_said_once_and_no_link_is_offered() {
        let store = a_store(false);
        let registrar = Registrar::default();
        let (person, decide) = a_person();
        with_worker(&store, &registrar, false, |worker, memory| {
            worker.hold.desktop = Some(person.clone());
            worker.take(memory.bindings.get());
            decide
                .send(Verdict::Refuse(ShortcutsUnavailable::Desktop(
                    DesktopRefusal::NeedsEntry,
                )))
                .expect("the desktop is listening");

            let lines = answer_of(&mut worker.hold);

            assert_eq!(lines, vec![the_offer()]);
            assert_eq!(worker.hold.held_back(), None);
            assert_eq!(worker.hold.links(), vec![ShortcutLink::Window]);
            assert!(worker.carry_out(Command::EnableEntry).view.is_none());
        });
    }

    /// The window that closes twice, or a resume with nothing paused, must not
    /// put the question to the person a second time.
    #[test]
    fn a_resume_while_the_desktop_is_waited_on_asks_nothing() {
        let store = a_store(false);
        let registrar = Registrar::default();
        let (person, _decide) = a_person();
        with_worker(&store, &registrar, false, |worker, memory| {
            worker.hold.desktop = Some(person.clone());
            worker.take(memory.bindings.get());
            wait_asked(&person, 1);

            let carried = worker.carry_out(Command::Resume);

            assert!(carried.view.is_none());
            assert!(worker.hold.asking_now());
            std::thread::sleep(Duration::from_millis(30));
            assert_eq!(person.times_asked(), 1);
        });
    }

    /// 🔴 The shortcuts window opens while the person is still choosing in the
    /// window of the system: nothing is given back and the question stands, so the
    /// answer that comes later is held and told to the window, which waits for it.
    /// Dropping the question would have asked the person again when the window closed.
    #[test]
    fn a_pause_while_the_desktop_is_waited_on_leaves_the_question_standing() {
        let store = a_store(false);
        let registrar = Registrar::default();
        let (person, decide) = a_person();
        with_worker(&store, &registrar, false, |worker, memory| {
            worker.hold.desktop = Some(person.clone());
            worker.take(memory.bindings.get());

            let carried = worker.carry_out(Command::Pause);

            assert!(!worker.hold.paused());
            assert!(worker.hold.asking_now(), "the question was dropped");
            assert!(carried.view.is_none(), "nothing is paused to be said");
            let Some(Told::Paused(now)) = carried.told else {
                panic!("the window was not told: {:?}", carried.told);
            };
            assert_eq!(
                now.desktop,
                Some(DesktopNow {
                    given: Vec::new(),
                    waiting: true,
                    standing: Some(waiting_line()),
                })
            );

            decide
                .send(Verdict::Give)
                .expect("the desktop is listening");
            answer_of(&mut worker.hold);

            assert!(worker.hold.live().is_some());
            assert_eq!(person.alive(), 1);
            assert_eq!(
                worker.given_news(),
                Some(Told::Given(DesktopNow {
                    given: Vec::new(),
                    waiting: false,
                    standing: None,
                }))
            );
        });
    }

    /// 🔴 The palette closes while the person has not chosen: the wait between
    /// commands ends on the stop at once, not when the window of the system is
    /// answered - or the process would stand for five minutes.
    #[test]
    fn a_palette_that_closes_while_the_desktop_is_waited_on_does_not_wait_for_it() {
        let store = a_store(false);
        let registrar = Registrar::default();
        let (person, _decide) = a_person();
        with_worker(&store, &registrar, false, |worker, memory| {
            worker.hold.desktop = Some(person.clone());
            worker.take(memory.bindings.get());
            let (_send, commands) = mpsc::channel();
            let stop = AtomicBool::new(true);

            let next = next_command(&stop, &commands, &mut || worker.hold.arrived());

            assert!(matches!(next, Next::Stop));
        });
    }

    /// The answer reaches the loop that waits between commands, which hands it
    /// on as a step of its own.
    #[test]
    fn the_wait_between_commands_hands_on_the_answer_of_the_desktop() {
        let store = a_store(false);
        let registrar = Registrar::default();
        let (person, decide) = a_person();
        with_worker(&store, &registrar, false, |worker, memory| {
            worker.hold.desktop = Some(person.clone());
            worker.take(memory.bindings.get());
            decide
                .send(Verdict::Give)
                .expect("the desktop is listening");
            let (_send, commands) = mpsc::channel();
            let stop = AtomicBool::new(false);
            let done = AtomicBool::new(false);

            // An answer that is not handed on would be waited for for ever, so a
            // stop comes after a few seconds and the test fails instead of hanging.
            let next = std::thread::scope(|scope| {
                scope.spawn(|| {
                    let until = Instant::now() + Duration::from_secs(3);
                    while !done.load(Ordering::Relaxed) && Instant::now() < until {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    stop.store(true, Ordering::Relaxed);
                });
                let next = next_command(&stop, &commands, &mut || worker.hold.arrived());
                done.store(true, Ordering::Relaxed);
                next
            });

            match next {
                Next::Answered(lines) => assert!(lines.is_empty(), "{lines:?}"),
                _ => panic!("the answer was not handed on"),
            }
            assert!(worker.hold.live().is_some());
        });
    }

    /// A desktop that answers nothing at all - its thread ended - is a refusal
    /// with a sentence, not a wait that never ends.
    #[test]
    fn a_desktop_whose_question_ended_without_an_answer_is_a_refusal_and_not_a_wait() {
        struct Vanishing;
        impl HotkeyRegistrar for Vanishing {
            fn register(&self, _: &[(HotkeyAction, HotkeyChord)]) -> Answer {
                std::panic::resume_unwind(Box::new("the desktop's thread ended"))
            }
        }
        let store = a_store(false);
        let registrar = Registrar::default();
        with_worker(&store, &registrar, false, |worker, memory| {
            worker.hold.desktop = Some(Arc::new(Vanishing));
            worker.take(memory.bindings.get());

            let lines = answer_of(&mut worker.hold);

            assert_eq!(
                lines,
                vec![i18n::shortcuts_unavailable(
                    &ShortcutsUnavailable::CouldNotStart
                )]
            );
            assert!(!worker.hold.asking_now());
            assert!(worker.hold.live().is_none());
        });
    }

    /// Asking a desktop is a choice of route and nothing else: with no desktop
    /// to ask, the registrar answers on the spot, as it always did.
    #[test]
    fn with_no_desktop_the_registrar_answers_on_the_spot() {
        let store = a_store(false);
        let registrar = Registrar::default();
        with_worker(&store, &registrar, false, |worker, memory| {
            let lines = worker.take(memory.bindings.get());

            assert!(lines.is_empty(), "{lines:?}");
            assert!(!worker.hold.asking_now());
            assert!(worker.hold.live().is_some());
            assert_eq!(worker.hold.held_back(), None);
        });
    }

    /// 🔴 The window opens on a palette that has let its shortcuts go - a
    /// press there must reach the window, not send a value into it - and
    /// hears the table it edits in the answer.
    #[test]
    fn a_pause_gives_the_shortcuts_back_says_so_and_tells_the_window_the_table() {
        let store = a_store(false);
        let registrar = Registrar::default();
        with_worker(&store, &registrar, true, |worker, _| {
            assert_eq!(registrar.alive(), 1);

            let carried = worker.carry_out(Command::Pause);

            assert_eq!(registrar.alive(), 0, "the shortcuts were not given back");
            assert!(worker.hold.paused());
            let view = carried.view.expect("the palette says it is paused");
            assert_eq!(view.messages, vec![paused_line()]);
            let defaults = nkb_adapters::default_bindings();
            assert_eq!(
                carried.told,
                Some(Told::Paused(ShortcutsNow {
                    bindings: defaults,
                    registered: defaults
                        .as_slice()
                        .iter()
                        .map(|(action, chord)| (*action, *chord, ShortcutRegistration::Registered))
                        .collect(),
                    desktop: None,
                }))
            );
        });
    }

    /// The pause is said in every view while it lasts - a pack chosen in the
    /// pack window meanwhile must not take the sentence away - and not after.
    #[test]
    fn while_paused_every_view_says_so_and_after_a_resume_none_does() {
        let store = a_store(false);
        let registrar = Registrar::default();
        with_worker(&store, &registrar, true, |worker, _| {
            worker.carry_out(Command::Pause);

            let chosen = worker
                .carry_out(Command::Choose(String::from("unicode-text")))
                .view
                .expect("another pack is drawn");
            assert_eq!(chosen.messages.first(), Some(&paused_line()));

            let resumed = worker
                .carry_out(Command::Resume)
                .view
                .expect("the palette says what came of taking them again");
            assert!(
                !resumed.messages.contains(&paused_line()),
                "{:?}",
                resumed.messages
            );
            assert!(!worker.hold.paused());
            assert_eq!(registrar.alive(), 1);
        });
    }

    /// Nothing is registered twice: a resume while held changes nothing and
    /// says nothing.
    #[test]
    fn a_resume_while_the_shortcuts_are_held_does_nothing() {
        let store = a_store(false);
        let registrar = Registrar::default();
        with_worker(&store, &registrar, true, |worker, _| {
            let asked = registrar.asked.borrow().len();
            let carried = worker.carry_out(Command::Resume);
            assert!(carried.view.is_none() && carried.told.is_none());
            assert_eq!(registrar.asked.borrow().len(), asked);
            assert_eq!(registrar.alive(), 1);
        });
    }

    /// 🔴 Where a desktop holds the shortcuts the window opens over what it gave,
    /// and nothing is given back: giving them back would close the session, and
    /// taking them again would put the window of the system before the person once
    /// more. The window is told what the desktop holds, in the desktop's own words,
    /// and a close that follows changes nothing.
    #[test]
    fn a_pause_over_a_desktop_gives_nothing_back_and_tells_the_window_what_it_gave() {
        let store = a_store(false);
        let registrar = Registrar::default();
        let (person, decide) = a_person();
        with_worker(&store, &registrar, false, |worker, memory| {
            worker.hold.desktop = Some(person.clone());
            worker.take(memory.bindings.get());
            let chosen = HotkeyChord::parse("Ctrl+Shift+K").expect("a chord of the palette");
            let held = vec![
                (
                    HotkeyAction::NextValue,
                    GivenShortcut {
                        words: String::from("Press <Control><Shift>k"),
                        chord: Some(chosen),
                    },
                ),
                (
                    HotkeyAction::PreviousValue,
                    GivenShortcut {
                        words: String::from("Press <Control><Alt>Return"),
                        chord: None,
                    },
                ),
            ];
            decide
                .send(Verdict::Keep {
                    left_out: vec![HotkeyAction::RestartPack],
                    held: held.clone(),
                })
                .expect("the desktop is listening");
            let lines = answer_of(&mut worker.hold);
            worker.answered(lines);
            assert_eq!(person.alive(), 1);

            let carried = worker.carry_out(Command::Pause);

            assert_eq!(person.alive(), 1, "the desktop's shortcuts were given back");
            assert!(!worker.hold.paused());
            assert!(worker.hold.live().is_some());
            assert!(carried.view.is_none(), "nothing is paused to be said");
            let Some(Told::Paused(now)) = carried.told else {
                panic!("the window was not told: {:?}", carried.told);
            };
            assert_eq!(
                now.desktop,
                Some(DesktopNow {
                    given: held,
                    waiting: false,
                    standing: None,
                })
            );

            let closed = worker.carry_out(Command::Resume);
            assert!(closed.view.is_none() && closed.told.is_none());
            assert_eq!(person.alive(), 1);
            assert_eq!(person.times_asked(), 1);
        });
    }

    /// The window says what the palette says while the shortcuts are held back -
    /// the wait, the offer to make the entry - and, for as long as it is true, why
    /// the desktop gave none. A person who opens it later is asking exactly that.
    /// Asked again, the desktop is waited on and the old reason is gone.
    #[test]
    fn the_window_hears_why_the_desktop_gave_nothing_until_it_is_asked_again() {
        let store = a_store(false);
        let registrar = Registrar::default();
        let (person, decide) = a_person();
        let desk = Arc::new(Desk::default());
        with_worker(&store, &registrar, false, |worker, memory| {
            assert_eq!(worker.hold.desktop_now(), None, "no desktop holds them");
            worker.hold.desktop = Some(person.clone());
            worker.hold.entry = Some(desk.clone());
            worker.take(memory.bindings.get());
            let standing = |worker: &crate::live::Worker<'_>| {
                worker.hold.desktop_now().expect("a desktop holds them")
            };
            assert_eq!(standing(worker).standing, Some(waiting_line()));
            assert!(standing(worker).waiting);

            let closed = DesktopRefusal::Closed;
            decide
                .send(Verdict::Refuse(ShortcutsUnavailable::Desktop(closed)))
                .expect("the desktop is listening");
            answer_of(&mut worker.hold);

            let now = standing(worker);
            assert!(!now.waiting);
            assert_eq!(
                now.standing,
                Some(i18n::shortcuts_unavailable(&ShortcutsUnavailable::Desktop(
                    closed
                )))
            );
            assert_eq!(worker.hold.held_back(), None, "the palette said it once");

            worker.take(memory.bindings.get());
            assert_eq!(standing(worker).standing, Some(waiting_line()));
            decide
                .send(Verdict::Refuse(ShortcutsUnavailable::Desktop(
                    DesktopRefusal::NeedsEntry,
                )))
                .expect("the desktop is listening");
            answer_of(&mut worker.hold);
            assert_eq!(standing(worker).standing, Some(the_offer()));

            worker.take(memory.bindings.get());
            decide
                .send(Verdict::Give)
                .expect("the desktop is listening");
            answer_of(&mut worker.hold);
            assert_eq!(
                standing(worker).standing,
                None,
                "a reason from before stands over shortcuts that were given"
            );
        });
    }

    /// A click on a link of the palette changes what the desktop holds while the
    /// window may be open, so the window is told what came of it: the wait, and the
    /// answer that follows. A click that changed nothing tells nothing.
    #[test]
    fn a_click_on_an_entry_link_tells_the_open_window_what_the_desktop_holds() {
        let store = a_store(false);
        let registrar = Registrar::default();
        let (person, decide) = a_person();
        let desk = Arc::new(Desk::default());
        with_worker(&store, &registrar, false, |worker, memory| {
            asked_in_vain(worker, memory, &person, &decide, &desk);

            let carried = worker.carry_out(Command::EnableEntry);

            assert_eq!(
                carried.told,
                Some(Told::Given(DesktopNow {
                    given: Vec::new(),
                    waiting: true,
                    standing: Some(waiting_line()),
                }))
            );
            let late = worker.carry_out(Command::RemoveEntry);
            assert!(late.view.is_none() && late.told.is_none());
        });
    }

    /// The person may change a chord in the window of the system, leave one out,
    /// or choose one the palette has no name for. What the palette says follows
    /// what the desktop holds: the hint bar and the bands, and every sentence
    /// that names a shortcut, from the table the answer leaves behind.
    #[test]
    fn what_the_desktop_gave_is_what_the_words_of_the_palette_name() {
        let store = a_store(false);
        let registrar = Registrar::default();
        let (person, decide) = a_person();
        with_worker(&store, &registrar, false, |worker, memory| {
            worker.hold.desktop = Some(person.clone());
            let asked = memory.bindings.get();
            worker.take(asked);
            let chosen = HotkeyChord::parse("Ctrl+Shift+K").expect("a chord of the palette");
            let worded = String::from("Press <Control><Alt>Return");
            decide
                .send(Verdict::Keep {
                    left_out: vec![HotkeyAction::RestartPack],
                    held: vec![
                        (
                            HotkeyAction::NextValue,
                            GivenShortcut {
                                words: String::from("Press <Control><Shift>k"),
                                chord: Some(chosen),
                            },
                        ),
                        (
                            HotkeyAction::PreviousValue,
                            GivenShortcut {
                                words: worded.clone(),
                                chord: None,
                            },
                        ),
                    ],
                })
                .expect("the desktop is listening");
            let lines = answer_of(&mut worker.hold);
            assert_eq!(lines.len(), 1, "the one left out is said: {lines:?}");

            let legend = worker.answered(lines).legend.expect("the words are drawn");

            let key_of = |action: HotkeyAction| {
                let name = i18n::action_name(action);
                legend
                    .hints
                    .iter()
                    .find(|(_, hint)| hint == name)
                    .map(|(key, _)| key.clone())
            };
            assert_eq!(key_of(HotkeyAction::NextValue), Some(i18n::chord(chosen)));
            assert_eq!(key_of(HotkeyAction::PreviousValue), Some(worded));
            assert_eq!(key_of(HotkeyAction::RestartPack), None, "left out");
            assert_eq!(
                key_of(HotkeyAction::OpenPacks),
                Some(i18n::chord(asked.chord(HotkeyAction::OpenPacks))),
                "a chord the desktop said nothing about stays as asked"
            );
            assert_eq!(legend.next, Some(chosen));
            assert_eq!(legend.previous, None, "worded, not a chord to name");
            assert_eq!(legend.no_value, i18n::no_value_yet(Some(chosen)));

            let now = memory.bindings.get();
            assert_eq!(now.chord(HotkeyAction::NextValue), chosen);
            assert_eq!(
                now.chord(HotkeyAction::OpenPacks),
                asked.chord(HotkeyAction::OpenPacks)
            );

            // `OBS-225`: what the desktop left out or gave in other words is no key a
            // sentence may name, and the restart row of the value window follows it.
            assert_eq!(now.named(HotkeyAction::RestartPack), None, "left out");
            assert_eq!(now.named(HotkeyAction::PreviousValue), None, "worded");
            assert_eq!(now.named(HotkeyAction::NextValue), Some(chosen));
            assert_eq!(
                now.named(HotkeyAction::OpenPacks),
                Some(asked.chord(HotkeyAction::OpenPacks))
            );
            let kept = nkb_app::advance_sequence::Message::CounterKept { done: 1, total: 2 };
            let said = i18n::message(&kept, "any-pack", &now);
            assert!(
                said.contains(i18n::action_name(HotkeyAction::RestartPack)),
                "{said}"
            );
            assert!(!said.contains(&i18n::chord(now.chord(HotkeyAction::RestartPack))));
        });
    }

    /// Where no desktop is asked nothing is given, so the words are the table
    /// asked for, as they were before a desktop could give anything.
    #[test]
    fn without_a_desktop_the_words_follow_the_table_asked_for() {
        let store = a_store(false);
        let registrar = Registrar::default();
        with_worker(&store, &registrar, true, |worker, memory| {
            let asked = memory.bindings.get();
            assert_eq!(worker.naming(asked), legend(&asked));
            assert_eq!(memory.bindings.get(), asked);
        });
    }
}

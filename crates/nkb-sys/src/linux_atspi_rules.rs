//! What the accessibility bus of a Linux session says about the keyboard focus, and what is
//! decided from it without a bus (`D145`, L4). `linux/atspi.rs` asks the bus, this file
//! decides, and everything here is tested on every system.
//!
//! # What was measured before this was written
//!
//! 2026-10-24, Ubuntu 26.04 on VirtualBox, GNOME 50 on Wayland, the focus put where it was
//! wanted with a keyboard of the kernel and the tree read through `Atspi` (role and states
//! only, `tools/sondy/linux/`):
//!
//! - the application in front shows its top-level window with the state `ACTIVE`, and the
//!   one control that holds the keyboard focus shows `FOCUSED`. Neither is shown by a
//!   container of it, so the focus is found by walking down from the windows in front,
//! - a GTK4 text view answers `TEXT` + `EDITABLE`, a Slint field `ENTRY` + `EDITABLE`, and
//!   Firefox on a page: `<input>` `ENTRY` + `EDITABLE` + `SINGLE_LINE`, `<textarea>` `ENTRY` +
//!   `EDITABLE` + `MULTI_LINE`, `contenteditable` `SECTION` + `EDITABLE`, a password input
//!   `PASSWORD_TEXT` + `EDITABLE`, a read-only input `ENTRY` WITHOUT `EDITABLE`, a button
//!   `PUSH_BUTTON`, a link `LINK`, a `<select>` `COMBO_BOX`,
//! - the number of questions to the bus is two per element visited (its children, its
//!   states). Firefox needed 70 to 80 elements before the one with the focus, a GTK4 or
//!   Slint window 7 to 31. 2026-11-13, Firefox 158 on the page of the field receiver: the
//!   same look asked one question after the other took 231 ms in the median (8 looks), and
//!   with the questions of one level of the tree asked together 46 ms with the whole level
//!   waiting, 62 ms with at most 32 waiting and 81 ms with 4 (76 elements over six levels,
//!   71 one after the other, `tools/sondy/linux/atspi_rownolegle.py`),
//! - with the switch of the bus (`org.a11y.Status.IsEnabled`) off, GTK applications still
//!   show their trees, and Slint, Qt, Firefox and Chromium do not show a window at all.
//!   Chromium, switch on, showed a window with no children in the run made.
//!
//! 2026-11-03, the same machine, `tools/sondy/linux/atspi_zatrzymana.py` (`OBS-238`): an
//! application stopped with `SIGSTOP` stays on the bus and answers nothing, and the look
//! went on to the others. A GTK window with the focus on a button, in front on a second X
//! server on the same bus, made `nkb send` refuse the stopped field as "not a text field",
//! and the same window with the focus in an entry made it press the clearing keys into the
//! stopped application, confirmed as a field by another program.
//!
//! # What is decided here
//!
//! The kind of control is claimed only from the role AND the states of the one element that
//! holds the focus ([`kind_of`]): a text field needs a role from the closed list of the
//! ones that take text and the state `EDITABLE`, and a control that takes no text by
//! definition needs a role from the other closed list. Everything else, a table cell
//! included (the cell of a spreadsheet takes the clearing keys to the row, `D76`), is
//! `Unknown`, and `Unknown` presses as it did before this existed.
//!
//! The answer is about the window in front only while every application answered. One that
//! said nothing in time may be the one in front ([`Reply::Silent`]), and then whatever the
//! others show is not about its window, so the look is [`FocusedInput::Unanswered`], as is a
//! look whose time ran out (`OBS-258`: a page Firefox has just opened answered in 1 to 8 s).
//! One that answered with an error has gone or does not know the question, holds no window
//! in front, and is passed over. A look that visited as many elements as it may is not about
//! time - a larger page answers so every time - and stays `Unknown`.
//!
//! The windows in front are walked down in the order of the tree, as depth first, but the
//! next [`AT_ONCE`] elements are asked together ([`find`]): a look costs a journey to the bus
//! for every so many elements and every level down, rather than one for every question. In
//! each window the element with the focus is the first one met. A field above a long list
//! is met after a few questions, where a walk a level at a time asks the whole list first.
//!
//! Nothing here reads a name, a description or text of any element: the questions are the
//! children, the states and the role of an element, and `tests/atspi_reads_only_kinds.rs`
//! holds the list of what the code may say to the bus.

// The names and the limits are asked by the Linux route, which is not built elsewhere, and
// the tests ask the decisions on every system.
#![cfg_attr(
    not(target_os = "linux"),
    allow(
        dead_code,
        reason = "the Linux route asks what the tests of other systems do not"
    )
)]

use core::time::Duration;
use std::collections::VecDeque;
use std::time::Instant;

use crate::field::FocusedInput;

/// The bus name, the root object and the interface the questions go to. The bus is a file
/// socket on this machine, found by asking the session bus for its address.
pub(crate) const REGISTRY_NAME: &str = "org.a11y.atspi.Registry";
pub(crate) const REGISTRY_ROOT: &str = "/org/a11y/atspi/accessible/root";
pub(crate) const ACCESSIBLE_INTERFACE: &str = "org.a11y.atspi.Accessible";
pub(crate) const LAUNCHER_NAME: &str = "org.a11y.Bus";
pub(crate) const LAUNCHER_PATH: &str = "/org/a11y/bus";
pub(crate) const STATUS_INTERFACE: &str = "org.a11y.Status";

/// How long one question may take. A hung application costs this much once, and its
/// answer is "no answer".
pub(crate) const CALL_WAIT: Duration = Duration::from_millis(300);

/// How long the whole look may take. Measured on Firefox: 70 to 80 elements and two
/// questions each, a few hundred milliseconds, and on a page it has just opened 1 to 8 s
/// (2026-11-13, `OBS-258`). A look that runs out is [`FocusedInput::Unanswered`], never a
/// palette that hangs.
pub(crate) const LOOK_WAIT: Duration = Duration::from_millis(900);

/// How many elements one look may visit, over all windows in front. A page with thousands
/// of elements is not walked to its end.
pub(crate) const MAX_ELEMENTS: usize = 1500;

/// How many elements the walk asks together, and how many questions may wait for an answer
/// at a time. Measured on Firefox: 32 waiting is as fast as a whole level within the noise
/// (above). The bus of accessibility of GNOME allows a connection 50000 questions waiting,
/// and the number `dbus-daemon` is built with is 128 (2026-11-13, Ubuntu 26.04).
pub(crate) const AT_ONCE: usize = 32;

/// How long an answer is kept for the next question. A press asks three times in a few
/// milliseconds (the target line, the refusal of a button, the clearing), and a look that
/// walks a browser is not made three times. The answer of the last look is the one of this
/// press only while it is this fresh, and a press a second later looks again.
pub(crate) const REUSE: Duration = Duration::from_millis(300);

/// The names of the two errors the bus sends back in place of a reply that never came, and
/// of the one it sends when this program has more questions waiting than it allows: none of
/// them is an answer of the application. Read off an answer, never said.
const NO_REPLY: &str = "org.freedesktop.DBus.Error.NoReply";
const TIMED_OUT: &str = "org.freedesktop.DBus.Error.TimedOut";
const LIMITS_EXCEEDED: &str = "org.freedesktop.DBus.Error.LimitsExceeded";

/// The states of an element the code asks about. `GetState` answers two words of 32 bits,
/// and a state is the bit of that number.
pub(crate) mod state {
    pub(crate) const ACTIVE: u32 = 1;
    pub(crate) const DEFUNCT: u32 = 6;
    pub(crate) const EDITABLE: u32 = 7;
    pub(crate) const FOCUSED: u32 = 12;
}

/// The roles of an element the code recognises, by the number `GetRole` answers.
pub(crate) mod role {
    pub(crate) const CHECK_BOX: u32 = 7;
    pub(crate) const CHECK_MENU_ITEM: u32 = 8;
    pub(crate) const COMBO_BOX: u32 = 11;
    pub(crate) const ICON: u32 = 26;
    pub(crate) const IMAGE: u32 = 27;
    pub(crate) const LIST: u32 = 31;
    pub(crate) const LIST_ITEM: u32 = 32;
    pub(crate) const MENU: u32 = 33;
    pub(crate) const MENU_BAR: u32 = 34;
    pub(crate) const MENU_ITEM: u32 = 35;
    pub(crate) const PAGE_TAB: u32 = 37;
    pub(crate) const PAGE_TAB_LIST: u32 = 38;
    pub(crate) const PASSWORD_TEXT: u32 = 40;
    pub(crate) const PROGRESS_BAR: u32 = 42;
    pub(crate) const PUSH_BUTTON: u32 = 43;
    pub(crate) const RADIO_BUTTON: u32 = 44;
    pub(crate) const RADIO_MENU_ITEM: u32 = 45;
    pub(crate) const SCROLL_BAR: u32 = 48;
    pub(crate) const SLIDER: u32 = 51;
    pub(crate) const SPIN_BUTTON: u32 = 52;
    pub(crate) const STATUS_BAR: u32 = 54;
    pub(crate) const TERMINAL: u32 = 60;
    pub(crate) const TEXT: u32 = 61;
    pub(crate) const TOGGLE_BUTTON: u32 = 62;
    pub(crate) const PARAGRAPH: u32 = 73;
    pub(crate) const EDITBAR: u32 = 77;
    pub(crate) const ENTRY: u32 = 79;
    pub(crate) const SECTION: u32 = 85;
    pub(crate) const LINK: u32 = 88;
    pub(crate) const TREE_ITEM: u32 = 91;
    pub(crate) const DOCUMENT_TEXT: u32 = 94;
    pub(crate) const DOCUMENT_WEB: u32 = 95;
    pub(crate) const DOCUMENT_EMAIL: u32 = 96;
    pub(crate) const LIST_BOX: u32 = 98;
}

/// The roles that take typed text when the element is editable.
const TEXT_ROLES: &[u32] = &[
    role::ENTRY,
    role::PASSWORD_TEXT,
    role::TEXT,
    role::PARAGRAPH,
    role::SECTION,
    role::SPIN_BUTTON,
    role::EDITBAR,
    role::DOCUMENT_TEXT,
    role::DOCUMENT_WEB,
    role::DOCUMENT_EMAIL,
];

/// The roles that take no text by definition. A table cell is NOT here, nor is a role
/// the list does not know: those are `Unknown`. A combo box is here only while it is not
/// editable (an editable one hands the focus to its entry).
const NOT_TEXT_ROLES: &[u32] = &[
    role::CHECK_BOX,
    role::CHECK_MENU_ITEM,
    role::ICON,
    role::IMAGE,
    role::LINK,
    role::LIST,
    role::LIST_BOX,
    role::LIST_ITEM,
    role::MENU,
    role::MENU_BAR,
    role::MENU_ITEM,
    role::PAGE_TAB,
    role::PAGE_TAB_LIST,
    role::PROGRESS_BAR,
    role::PUSH_BUTTON,
    role::RADIO_BUTTON,
    role::RADIO_MENU_ITEM,
    role::SCROLL_BAR,
    role::SLIDER,
    role::STATUS_BAR,
    role::TOGGLE_BUTTON,
    role::TREE_ITEM,
];

/// Whether the bit of `state` is set in the words `GetState` answered.
pub(crate) fn has(states: &[u32], state: u32) -> bool {
    let word = usize::try_from(state / 32).unwrap_or(usize::MAX);
    states
        .get(word)
        .is_some_and(|bits| bits & (1 << (state % 32)) != 0)
}

/// What kind of control an element is, from its role and its states.
///
/// A terminal is its own answer whatever its states say: what the clearing keys do there is
/// the choice of the program running in it (`OBS-141`). A text field needs a role that takes
/// text AND the state `EDITABLE`, so a read-only entry is not claimed. A control that takes
/// no text by definition is the only answer that stops a send. Everything else is
/// `Unknown`.
pub(crate) fn kind_of(code: u32, states: &[u32]) -> FocusedInput {
    if code == role::TERMINAL {
        return FocusedInput::Terminal;
    }
    let editable = has(states, state::EDITABLE);
    if editable && TEXT_ROLES.contains(&code) {
        FocusedInput::TextField
    } else if !editable && NOT_TEXT_ROLES.contains(&code) {
        FocusedInput::NotTextField
    } else if !editable && code == role::COMBO_BOX {
        // A closed list, a `select` in a browser. An editable one passes the focus on.
        FocusedInput::NotTextField
    } else {
        FocusedInput::Unknown
    }
}

/// The answer to one question of an element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Reply<T> {
    /// What the element said.
    Said(T),
    /// An error came back: the application has gone, or does not know the question, and
    /// holds no window in front.
    Refused,
    /// Nothing came back in time. The application may be stopped or busy, and it may be the
    /// one in front.
    Silent,
}

impl<T> Reply<T> {
    /// The answer, when one came.
    pub(crate) fn said(self) -> Option<T> {
        match self {
            Self::Said(answer) => Some(answer),
            Self::Refused | Self::Silent => None,
        }
    }

    /// The answer read by `read`. An answer that does not read is `Refused`: something came
    /// back.
    pub(crate) fn then<U>(self, read: impl FnOnce(T) -> Option<U>) -> Reply<U> {
        match self {
            Self::Said(answer) => read(answer).map_or(Reply::Refused, Reply::Said),
            Self::Refused => Reply::Refused,
            Self::Silent => Reply::Silent,
        }
    }
}

/// What a question that failed means, from the name of the error that came back, if one
/// did. An error is an answer, except the two the bus sends for a reply that never came and
/// the one for too many questions waiting, which the application never saw. No error at all
/// is a question that ran out of time or a connection that was shut, and that is silence.
pub(crate) fn failed<T>(error: Option<&str>) -> Reply<T> {
    match error {
        Some(NO_REPLY | TIMED_OUT | LIMITS_EXCEEDED) | None => Reply::Silent,
        Some(_) => Reply::Refused,
    }
}

/// The tree of the bus, as far as the look asks it. Every question may get no answer.
/// Inside the windows in front a part that does not answer is passed over, and so is an
/// application that answered with an error. An application that did not answer at all
/// makes the look [`Found::Silent`].
///
/// The children and the states are asked of many elements together, and come back one
/// answer for each element, in their order. No question of them is started once the time
/// of the look is out, and `None` is a time that ran out before every one was asked.
pub(crate) trait Tree {
    type Node;

    /// The applications that show a tree.
    fn applications(&mut self) -> Option<Vec<Self::Node>>;
    /// The children of each element.
    fn children(&mut self, nodes: &[Self::Node]) -> Option<Vec<Reply<Vec<Self::Node>>>>;
    /// The states of each element, as `GetState` words them.
    fn states(&mut self, nodes: &[Self::Node]) -> Option<Vec<Reply<Vec<u32>>>>;
    /// The role of an element.
    fn role(&mut self, node: &Self::Node) -> Option<u32>;
}

/// The element that holds the focus in a window, with the states it answered.
type Holder<N> = (N, Vec<u32>);

/// One question asked of many elements together, as [`Tree`] asks it.
type Question<T, A> = fn(&mut T, &[<T as Tree>::Node]) -> Option<Vec<A>>;

/// What a look found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Found {
    /// One element holds the focus in the window in front, and this is its kind.
    Kind(FocusedInput),
    /// No application shows a window in front.
    NoWindow,
    /// A window is in front and nothing in it shows the focus.
    NoFocus,
    /// More than one window in front shows an element with the focus. One of them is
    /// stale, and which is not known.
    Several,
    /// The time ran out before the answer.
    Spent,
    /// The walk visited as many elements as it may before the answer.
    TooMany,
    /// An application did not answer, so the window in front may be one of its windows and
    /// what the others show is not about it (`OBS-238`).
    Silent,
}

/// Looks for the element that holds the keyboard focus, in the windows in front.
pub(crate) fn find<T: Tree>(tree: &mut T) -> Found {
    let Some(applications) = tree.applications() else {
        return Found::NoWindow;
    };
    let holders = match in_front(tree, &applications).and_then(|windows| walk(tree, windows)) {
        Ok(holders) => holders,
        Err(found) => return found,
    };
    let mut holders = holders.into_iter();
    match (holders.next(), holders.next()) {
        (None, _) => Found::NoFocus,
        (Some(_), Some(_)) => Found::Several,
        (Some((node, states)), None) => Found::Kind(
            tree.role(&node)
                .map_or(FocusedInput::Unknown, |role| kind_of(role, &states)),
        ),
    }
}

/// The windows in front, asked of every application at once. An application or a window
/// that did not answer makes it [`Found::Silent`], whatever the others show.
fn in_front<T: Tree>(tree: &mut T, applications: &[T::Node]) -> Result<Vec<T::Node>, Found> {
    let mut silent = false;
    let mut tops = Vec::new();
    for reply in ask(tree, applications, T::children)? {
        match reply {
            Reply::Said(more) => tops.extend(more),
            Reply::Refused => {}
            Reply::Silent => silent = true,
        }
    }
    let mut windows = Vec::new();
    let replies = ask(tree, &tops, T::states)?;
    for (top, reply) in tops.into_iter().zip(replies) {
        match reply {
            Reply::Said(states) if has(&states, state::ACTIVE) => windows.push(top),
            Reply::Silent => silent = true,
            Reply::Said(_) | Reply::Refused => {}
        }
    }
    if silent {
        Err(Found::Silent)
    } else if windows.is_empty() {
        Err(Found::NoWindow)
    } else {
        Ok(windows)
    }
}

/// One question of every element of `nodes`: [`Found::Spent`] when the time ran out before
/// every one was asked. No elements, no question.
fn ask<T: Tree, A>(
    tree: &mut T,
    nodes: &[T::Node],
    question: Question<T, A>,
) -> Result<Vec<A>, Found> {
    if nodes.is_empty() {
        return Ok(Vec::new());
    }
    question(tree, nodes).ok_or(Found::Spent)
}

/// Down `windows` in the order of the tree, the windows one after the other, to the first
/// element with the focus in each. The elements not asked yet wait in that order, and the
/// first [`AT_ONCE`] of them are asked together: their states, then the children of those
/// without the focus, which take their place at the head of the wait. One at a time this is
/// the walk depth first, and all at a time the walk a level at a time. A window whose
/// element was found is not walked further, and the elements with the focus come back one
/// per window that has one, in the order of the windows.
///
/// A field above a long list is met after a few questions, as it is depth first, where a walk
/// a level at a time would ask the whole list first. The walk visits at most
/// [`MAX_ELEMENTS`], and the look is [`Found::TooMany`] when that many were asked and a
/// window without its element still has something waiting.
fn walk<T: Tree>(tree: &mut T, windows: Vec<T::Node>) -> Result<Vec<Holder<T::Node>>, Found> {
    let mut holders: Vec<Option<Holder<T::Node>>> = windows.iter().map(|_| None).collect();
    // The elements not asked yet, each with the window it is in, in the order of the tree.
    let mut waiting: VecDeque<(usize, T::Node)> = windows.into_iter().enumerate().collect();
    let mut left = MAX_ELEMENTS;
    while !waiting.is_empty() {
        if left == 0 {
            return Err(Found::TooMany);
        }
        let now = waiting.len().min(AT_ONCE).min(left);
        left -= now;
        let (owners, nodes): (Vec<usize>, Vec<T::Node>) = waiting.drain(..now).unzip();
        let replies = ask(tree, &nodes, T::states)?;
        let mut parents = Vec::new();
        for ((owner, node), reply) in owners.into_iter().zip(nodes).zip(replies) {
            let Reply::Said(states) = reply else {
                continue;
            };
            if holders[owner].is_some() || has(&states, state::DEFUNCT) {
                continue;
            }
            if has(&states, state::FOCUSED) {
                holders[owner] = Some((node, states));
            } else {
                parents.push((owner, node));
            }
        }
        parents.retain(|(owner, _)| holders[*owner].is_none());
        waiting.retain(|(owner, _)| holders[*owner].is_none());
        let (owners, nodes): (Vec<usize>, Vec<T::Node>) = parents.into_iter().unzip();
        let replies = ask(tree, &nodes, T::children)?;
        let children: Vec<(usize, T::Node)> = owners
            .into_iter()
            .zip(replies)
            .flat_map(|(owner, reply)| {
                reply
                    .said()
                    .into_iter()
                    .flatten()
                    .map(move |child| (owner, child))
            })
            .collect();
        // Where their parents were: ahead of everything that still waits.
        for child in children.into_iter().rev() {
            waiting.push_front(child);
        }
    }
    Ok(holders.into_iter().flatten().collect())
}

/// The answer in the words of the question, given whether the bus is switched on.
///
/// A look that found no window in front or nothing with the focus, with the switch of the
/// bus off, is an application that shows nothing because it was not asked to: that is
/// `ClosedToAccessibility`, and the person is told so. With the switch on the same
/// finding is just not recognised. A look whose time ran out, or that met an application
/// that did not answer, is `Unanswered`: the system did not identify the focus in time, and
/// the person is told that rather than that the focus is no field. A look that found two,
/// or visited as many elements as it may, says nothing.
pub(crate) fn settle(found: Found, switched_on: bool) -> FocusedInput {
    match found {
        Found::Kind(kind) => kind,
        Found::NoWindow | Found::NoFocus if !switched_on => FocusedInput::ClosedToAccessibility,
        Found::Spent | Found::Silent => FocusedInput::Unanswered,
        Found::NoWindow | Found::NoFocus | Found::Several | Found::TooMany => FocusedInput::Unknown,
    }
}

/// The answer of the last look, kept for a moment.
#[derive(Debug, Default)]
pub(crate) struct Recent {
    last: Option<(Instant, FocusedInput)>,
}

impl Recent {
    /// Nothing kept.
    pub(crate) const fn new() -> Self {
        Self { last: None }
    }

    /// The answer kept, while it is fresh at `now`.
    pub(crate) fn kept(&self, now: Instant) -> Option<FocusedInput> {
        self.last
            .filter(|(at, _)| now.saturating_duration_since(*at) < REUSE)
            .map(|(_, kind)| kind)
    }

    /// Keeps `kind` as the answer of a look that ended at `at`.
    pub(crate) fn keep(&mut self, at: Instant, kind: FocusedInput) {
        self.last = Some((at, kind));
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;

    /// The role of a label, which takes no focus and is not one of the lists.
    const LABEL: u32 = 29;

    /// The words of `GetState` for the states named.
    fn states_of(names: &[u32]) -> Vec<u32> {
        let mut words = vec![0_u32, 0_u32];
        for name in names {
            words[usize::try_from(name / 32).expect("a small word")] |= 1 << (name % 32);
        }
        words
    }

    /// How an element of a made-up tree answers.
    #[derive(Clone, Copy)]
    enum Answers {
        Yes,
        WithAnError,
        Not,
    }

    /// An element of a made-up tree: its role, its states and its children by position.
    struct Element {
        role: u32,
        states: Vec<u32>,
        children: Vec<usize>,
        answers: Answers,
    }

    impl Element {
        fn reply<T>(&self, answer: T) -> Reply<T> {
            match self.answers {
                Answers::Yes => Reply::Said(answer),
                Answers::WithAnError => Reply::Refused,
                Answers::Not => Reply::Silent,
            }
        }
    }

    /// A tree in a vector, with the root's children being the applications, a count of the
    /// questions asked and of the times many of them went together. The time runs out after
    /// `questions_allowed` questions, also in the middle of a batch.
    struct Fake {
        elements: Vec<Element>,
        applications: Vec<usize>,
        asked: usize,
        batches: usize,
        questions_allowed: usize,
    }

    impl Fake {
        fn new() -> Self {
            Self {
                elements: Vec::new(),
                applications: Vec::new(),
                asked: 0,
                batches: 0,
                questions_allowed: usize::MAX,
            }
        }

        /// One answer for each of `nodes`, or `None` when the time ran out before the last.
        fn batch<A>(
            &mut self,
            nodes: &[usize],
            answer: impl Fn(&Element) -> A,
        ) -> Option<Vec<Reply<A>>> {
            self.batches += 1;
            let mut replies = Vec::new();
            for node in nodes {
                if self.asked >= self.questions_allowed {
                    return None;
                }
                self.asked += 1;
                let element = &self.elements[*node];
                replies.push(element.reply(answer(element)));
            }
            Some(replies)
        }

        fn add(&mut self, role: u32, names: &[u32], children: &[usize]) -> usize {
            self.elements.push(Element {
                role,
                states: states_of(names),
                children: children.to_vec(),
                answers: Answers::Yes,
            });
            self.elements.len() - 1
        }

        fn mute(&mut self, node: usize) {
            self.elements[node].answers = Answers::Not;
        }

        fn refuse(&mut self, node: usize) {
            self.elements[node].answers = Answers::WithAnError;
        }
    }

    impl Tree for Fake {
        type Node = usize;

        fn applications(&mut self) -> Option<Vec<usize>> {
            Some(self.applications.clone())
        }

        fn children(&mut self, nodes: &[usize]) -> Option<Vec<Reply<Vec<usize>>>> {
            self.batch(nodes, |element| element.children.clone())
        }

        fn states(&mut self, nodes: &[usize]) -> Option<Vec<Reply<Vec<u32>>>> {
            self.batch(nodes, |element| element.states.clone())
        }

        fn role(&mut self, node: &usize) -> Option<u32> {
            Some(self.elements[*node].role)
        }
    }

    /// An application with one window in front holding a field with these states.
    fn window_with(tree: &mut Fake, role: u32, names: &[u32]) -> usize {
        let field = tree.add(role, names, &[]);
        let panel = tree.add(39, &[], &[field]);
        let window = tree.add(23, &[state::ACTIVE], &[panel]);
        let application = tree.add(75, &[], &[window]);
        tree.applications.push(application);
        field
    }

    #[test]
    fn a_state_is_the_bit_of_its_number_in_the_words() {
        let words = states_of(&[1, 12, 33]);
        assert!(has(&words, 1) && has(&words, 12) && has(&words, 33));
        assert!(!has(&words, 0) && !has(&words, 7) && !has(&words, 32));
        assert!(!has(&[], 1), "no words, no state");
        assert!(!has(&words, 200), "a number past the words is not set");
    }

    #[test]
    fn an_editable_entry_is_a_text_field_and_a_read_only_one_is_not_claimed() {
        let editable = states_of(&[state::EDITABLE, state::FOCUSED]);
        let read_only = states_of(&[state::FOCUSED]);
        assert_eq!(kind_of(role::ENTRY, &editable), FocusedInput::TextField);
        assert_eq!(kind_of(role::ENTRY, &read_only), FocusedInput::Unknown);
    }

    #[test]
    fn every_role_that_takes_text_is_a_text_field_when_editable() {
        let editable = states_of(&[state::EDITABLE]);
        for text in [
            role::ENTRY,
            role::PASSWORD_TEXT,
            role::TEXT,
            role::PARAGRAPH,
            role::SECTION,
            role::SPIN_BUTTON,
            role::EDITBAR,
            role::DOCUMENT_TEXT,
            role::DOCUMENT_WEB,
            role::DOCUMENT_EMAIL,
        ] {
            assert_eq!(kind_of(text, &editable), FocusedInput::TextField, "{text}");
        }
    }

    #[test]
    fn a_terminal_is_a_terminal_whatever_its_states_say() {
        for names in [&[][..], &[state::EDITABLE][..]] {
            assert_eq!(
                kind_of(role::TERMINAL, &states_of(names)),
                FocusedInput::Terminal
            );
        }
    }

    #[test]
    fn a_control_that_takes_no_text_is_not_a_text_field() {
        let none = states_of(&[state::FOCUSED]);
        for control in [
            role::PUSH_BUTTON,
            role::TOGGLE_BUTTON,
            role::CHECK_BOX,
            role::RADIO_BUTTON,
            role::LINK,
            role::LIST_ITEM,
            role::MENU_ITEM,
            role::PAGE_TAB,
            role::SLIDER,
            role::TREE_ITEM,
        ] {
            assert_eq!(
                kind_of(control, &none),
                FocusedInput::NotTextField,
                "{control}"
            );
        }
    }

    #[test]
    fn a_select_is_a_closed_list_and_an_editable_combo_box_is_left_alone() {
        assert_eq!(
            kind_of(role::COMBO_BOX, &states_of(&[])),
            FocusedInput::NotTextField
        );
        assert_eq!(
            kind_of(role::COMBO_BOX, &states_of(&[state::EDITABLE])),
            FocusedInput::Unknown
        );
    }

    #[test]
    fn a_table_cell_and_a_role_nobody_knows_stay_unknown_even_when_editable() {
        // The cell of a spreadsheet takes the clearing keys to the row (`D76`).
        for role in [56, 69, 67, 9999] {
            for names in [&[][..], &[state::EDITABLE][..]] {
                assert_eq!(
                    kind_of(role, &states_of(names)),
                    FocusedInput::Unknown,
                    "{role}"
                );
            }
        }
    }

    #[test]
    fn a_push_button_that_says_it_is_editable_is_not_a_field_either() {
        assert_eq!(
            kind_of(role::PUSH_BUTTON, &states_of(&[state::EDITABLE])),
            FocusedInput::Unknown
        );
    }

    #[test]
    fn the_focus_is_found_down_the_window_in_front() {
        let mut tree = Fake::new();
        window_with(&mut tree, role::ENTRY, &[state::FOCUSED, state::EDITABLE]);
        assert_eq!(find(&mut tree), Found::Kind(FocusedInput::TextField));
    }

    #[test]
    fn a_window_not_in_front_is_not_looked_into() {
        let mut tree = Fake::new();
        let field = tree.add(role::ENTRY, &[state::FOCUSED, state::EDITABLE], &[]);
        let window = tree.add(23, &[], &[field]);
        let application = tree.add(75, &[], &[window]);
        tree.applications.push(application);
        assert_eq!(find(&mut tree), Found::NoWindow);
    }

    #[test]
    fn nothing_with_the_focus_in_the_window_in_front_is_said() {
        let mut tree = Fake::new();
        window_with(&mut tree, role::ENTRY, &[state::EDITABLE]);
        assert_eq!(find(&mut tree), Found::NoFocus);
    }

    #[test]
    fn two_windows_in_front_with_a_focus_each_are_not_guessed_between() {
        let mut tree = Fake::new();
        window_with(&mut tree, role::ENTRY, &[state::FOCUSED, state::EDITABLE]);
        window_with(&mut tree, role::PUSH_BUTTON, &[state::FOCUSED]);
        assert_eq!(find(&mut tree), Found::Several);
    }

    #[test]
    fn a_stale_window_in_front_without_a_focus_does_not_hide_the_one_with_it() {
        let mut tree = Fake::new();
        window_with(&mut tree, LABEL, &[]);
        window_with(&mut tree, role::ENTRY, &[state::FOCUSED, state::EDITABLE]);
        assert_eq!(find(&mut tree), Found::Kind(FocusedInput::TextField));
    }

    #[test]
    fn an_application_that_has_gone_is_passed_over() {
        let mut tree = Fake::new();
        let gone = tree.add(75, &[], &[]);
        tree.refuse(gone);
        tree.applications.push(gone);
        window_with(&mut tree, role::ENTRY, &[state::FOCUSED, state::EDITABLE]);
        assert_eq!(find(&mut tree), Found::Kind(FocusedInput::TextField));
    }

    #[test]
    fn an_application_that_does_not_answer_leaves_the_focus_unknown_whatever_the_others_show() {
        // `OBS-238`: the stopped application in front says nothing, and a window of another
        // program on the same bus shows the focus on a button or in an entry.
        for (role, names) in [
            (role::PUSH_BUTTON, &[state::FOCUSED][..]),
            (role::ENTRY, &[state::FOCUSED, state::EDITABLE][..]),
        ] {
            for hung_first in [true, false] {
                let mut tree = Fake::new();
                let hung = tree.add(75, &[], &[]);
                tree.mute(hung);
                if hung_first {
                    tree.applications.push(hung);
                }
                window_with(&mut tree, role, names);
                if !hung_first {
                    tree.applications.push(hung);
                }
                assert_eq!(find(&mut tree), Found::Silent, "{role}, first {hung_first}");
            }
        }
    }

    #[test]
    fn a_window_whose_states_do_not_come_leaves_the_focus_unknown_too() {
        let mut tree = Fake::new();
        let window = tree.add(23, &[state::ACTIVE], &[]);
        tree.mute(window);
        let application = tree.add(75, &[], &[window]);
        tree.applications.push(application);
        window_with(&mut tree, role::PUSH_BUTTON, &[state::FOCUSED]);
        assert_eq!(find(&mut tree), Found::Silent);
    }

    #[test]
    fn a_window_whose_states_come_back_as_an_error_is_passed_over() {
        let mut tree = Fake::new();
        let window = tree.add(23, &[state::ACTIVE], &[]);
        tree.refuse(window);
        let application = tree.add(75, &[], &[window]);
        tree.applications.push(application);
        window_with(&mut tree, role::PUSH_BUTTON, &[state::FOCUSED]);
        assert_eq!(find(&mut tree), Found::Kind(FocusedInput::NotTextField));
    }

    #[test]
    fn only_the_errors_for_a_reply_that_never_came_are_silence() {
        assert_eq!(failed::<()>(None), Reply::Silent);
        assert_eq!(failed::<()>(Some(NO_REPLY)), Reply::Silent);
        assert_eq!(failed::<()>(Some(TIMED_OUT)), Reply::Silent);
        // Too many questions waiting: the bus refused, the application never saw it.
        assert_eq!(failed::<()>(Some(LIMITS_EXCEEDED)), Reply::Silent);
        for refusal in [
            "org.freedesktop.DBus.Error.ServiceUnknown",
            "org.freedesktop.DBus.Error.NameHasNoOwner",
            "org.freedesktop.DBus.Error.UnknownMethod",
            "org.freedesktop.DBus.Error.UnknownObject",
        ] {
            assert_eq!(failed::<()>(Some(refusal)), Reply::Refused, "{refusal}");
        }
    }

    #[test]
    fn an_answer_that_does_not_read_is_a_refusal_and_silence_stays_silence() {
        assert_eq!(Reply::Said(4).then(|n| Some(n + 1)), Reply::Said(5));
        assert_eq!(Reply::Said(4).then(|_| None::<u32>), Reply::Refused);
        assert_eq!(Reply::<u32>::Refused.then(Some), Reply::Refused);
        assert_eq!(Reply::<u32>::Silent.then(Some), Reply::Silent);
        assert_eq!(Reply::Said(4).said(), Some(4));
        assert_eq!(Reply::<u32>::Silent.said(), None);
    }

    #[test]
    fn a_subtree_that_does_not_answer_is_not_a_reason_to_stop() {
        let mut tree = Fake::new();
        let mute = tree.add(39, &[], &[]);
        tree.mute(mute);
        let field = tree.add(role::ENTRY, &[state::FOCUSED, state::EDITABLE], &[]);
        let window = tree.add(23, &[state::ACTIVE], &[mute, field]);
        let application = tree.add(75, &[], &[window]);
        tree.applications.push(application);
        assert_eq!(find(&mut tree), Found::Kind(FocusedInput::TextField));
    }

    #[test]
    fn an_element_that_is_gone_does_not_hold_the_focus() {
        let mut tree = Fake::new();
        window_with(
            &mut tree,
            role::ENTRY,
            &[state::FOCUSED, state::EDITABLE, state::DEFUNCT],
        );
        assert_eq!(find(&mut tree), Found::NoFocus);
    }

    #[test]
    fn the_walk_stops_when_the_time_is_out_and_does_not_guess() {
        let mut tree = Fake::new();
        window_with(&mut tree, role::ENTRY, &[state::FOCUSED, state::EDITABLE]);
        tree.questions_allowed = 3;
        assert_eq!(find(&mut tree), Found::Spent);
    }

    #[test]
    fn the_walk_stops_at_the_most_elements_it_may_visit() {
        let mut tree = Fake::new();
        // A window with a very long row of elements and the focus at the end of it.
        let mut row = Vec::new();
        for _ in 0..(MAX_ELEMENTS + 5) {
            row.push(tree.add(role::LIST_ITEM, &[], &[]));
        }
        let field = tree.add(role::ENTRY, &[state::FOCUSED, state::EDITABLE], &[]);
        row.push(field);
        let window = tree.add(23, &[state::ACTIVE], &row);
        let application = tree.add(75, &[], &[window]);
        tree.applications.push(application);
        assert_eq!(find(&mut tree), Found::TooMany);
    }

    #[test]
    fn the_questions_of_a_walk_are_two_per_element_visited() {
        // The cost the measurement was made of: a window, a panel and a field is three
        // elements, and the application and the window were asked their children once.
        let mut tree = Fake::new();
        window_with(&mut tree, role::ENTRY, &[state::FOCUSED, state::EDITABLE]);
        let _ = find(&mut tree);
        assert!(tree.asked <= 10, "{} questions", tree.asked);
    }

    #[test]
    fn the_questions_of_one_level_go_together_however_wide_it_is() {
        // A window, three panels and two elements in each, the focus in the last one: the
        // children of the applications, the states of their windows, then the states and
        // the children of every level - seven journeys, whatever the width.
        let mut tree = Fake::new();
        let mut panels = Vec::new();
        for last in [false, false, true] {
            let first = tree.add(role::PUSH_BUTTON, &[], &[]);
            let focus: &[u32] = if last {
                &[state::FOCUSED, state::EDITABLE]
            } else {
                &[]
            };
            let second = tree.add(role::ENTRY, focus, &[]);
            panels.push(tree.add(39, &[], &[first, second]));
        }
        let window = tree.add(23, &[state::ACTIVE], &panels);
        let application = tree.add(75, &[], &[window]);
        tree.applications.push(application);
        assert_eq!(find(&mut tree), Found::Kind(FocusedInput::TextField));
        assert_eq!(tree.batches, 7);
        assert_eq!(tree.asked, 16, "two questions per element above the focus");
    }

    #[test]
    fn a_level_wider_than_what_is_left_still_answers_when_the_focus_is_in_the_part_asked() {
        let mut tree = Fake::new();
        let field = tree.add(role::ENTRY, &[state::FOCUSED, state::EDITABLE], &[]);
        let mut row = vec![field];
        for _ in 0..(MAX_ELEMENTS + 5) {
            row.push(tree.add(role::LIST_ITEM, &[], &[]));
        }
        let window = tree.add(23, &[state::ACTIVE], &row);
        let application = tree.add(75, &[], &[window]);
        tree.applications.push(application);
        assert_eq!(find(&mut tree), Found::Kind(FocusedInput::TextField));
    }

    #[test]
    fn a_window_the_limit_cut_short_says_too_many_though_another_found_its_focus() {
        let mut tree = Fake::new();
        window_with(&mut tree, role::ENTRY, &[state::FOCUSED, state::EDITABLE]);
        let mut row = Vec::new();
        for _ in 0..(MAX_ELEMENTS + 5) {
            row.push(tree.add(role::LIST_ITEM, &[], &[]));
        }
        let window = tree.add(23, &[state::ACTIVE], &row);
        let application = tree.add(75, &[], &[window]);
        tree.applications.push(application);
        assert_eq!(find(&mut tree), Found::TooMany);
    }

    #[test]
    fn a_field_above_a_long_list_is_met_after_a_few_questions() {
        // A search box above a list of results longer than the walk may visit. Depth first
        // it is met at once, and a walk a level at a time would ask the whole list before it.
        let mut tree = Fake::new();
        let field = tree.add(role::ENTRY, &[state::FOCUSED, state::EDITABLE], &[]);
        let paragraph = tree.add(73, &[], &[field]);
        let form = tree.add(87, &[], &[paragraph]);
        let mut results = Vec::new();
        for _ in 0..(MAX_ELEMENTS * 2) {
            results.push(tree.add(role::LIST_ITEM, &[], &[]));
        }
        let list = tree.add(role::LIST, &[], &results);
        let window = tree.add(23, &[state::ACTIVE], &[form, list]);
        let application = tree.add(75, &[], &[window]);
        tree.applications.push(application);
        assert_eq!(find(&mut tree), Found::Kind(FocusedInput::TextField));
        assert!(tree.asked < 4 * AT_ONCE, "{} questions", tree.asked);
    }

    #[test]
    fn in_one_window_the_focus_met_first_is_the_one_taken_and_not_two() {
        // A stale state deep in the first panel, and the focus on a button next to it: the
        // button is asked with the panel, before what is under the panel.
        let mut tree = Fake::new();
        let deep = tree.add(role::ENTRY, &[state::FOCUSED, state::EDITABLE], &[]);
        let inner = tree.add(39, &[], &[deep]);
        let panel = tree.add(39, &[], &[inner]);
        let button = tree.add(role::PUSH_BUTTON, &[state::FOCUSED], &[]);
        let window = tree.add(23, &[state::ACTIVE], &[panel, button]);
        let application = tree.add(75, &[], &[window]);
        tree.applications.push(application);
        assert_eq!(find(&mut tree), Found::Kind(FocusedInput::NotTextField));
    }

    #[test]
    fn a_time_that_runs_out_inside_a_level_is_spent_though_the_focus_was_asked() {
        // What was not asked may be a second window with the focus.
        let mut tree = Fake::new();
        let field = tree.add(role::ENTRY, &[state::FOCUSED, state::EDITABLE], &[]);
        let other = tree.add(role::LIST_ITEM, &[], &[]);
        let window = tree.add(23, &[state::ACTIVE], &[field, other]);
        let application = tree.add(75, &[], &[window]);
        tree.applications.push(application);
        tree.questions_allowed = 5;
        assert_eq!(find(&mut tree), Found::Spent);
    }

    #[test]
    fn a_window_in_front_of_an_application_that_shows_nothing_is_closed_while_the_bus_is_off() {
        assert_eq!(
            settle(Found::NoWindow, false),
            FocusedInput::ClosedToAccessibility
        );
        assert_eq!(
            settle(Found::NoFocus, false),
            FocusedInput::ClosedToAccessibility
        );
        assert_eq!(settle(Found::NoWindow, true), FocusedInput::Unknown);
        assert_eq!(settle(Found::NoFocus, true), FocusedInput::Unknown);
    }

    #[test]
    fn a_look_that_ran_out_of_time_or_met_silence_was_not_answered_in_time_whatever_the_switch() {
        // `OBS-258`: a page Firefox had just opened answered the look in 1 to 8 s, or not at
        // all, and the person was told the focus is no field.
        for on in [true, false] {
            assert_eq!(settle(Found::Spent, on), FocusedInput::Unanswered);
            assert_eq!(settle(Found::Silent, on), FocusedInput::Unanswered);
        }
    }

    #[test]
    fn a_look_that_found_two_or_visited_all_it_may_says_nothing_whatever_the_switch() {
        for on in [true, false] {
            assert_eq!(settle(Found::Several, on), FocusedInput::Unknown);
            assert_eq!(settle(Found::TooMany, on), FocusedInput::Unknown);
        }
    }

    #[test]
    fn a_kind_found_is_the_kind_whatever_the_switch() {
        for on in [true, false] {
            assert_eq!(
                settle(Found::Kind(FocusedInput::NotTextField), on),
                FocusedInput::NotTextField
            );
        }
    }

    #[test]
    fn an_answer_is_kept_for_a_moment_and_then_asked_again() {
        let start = Instant::now();
        let mut recent = Recent::default();
        assert_eq!(recent.kept(start), None, "nothing was asked yet");
        recent.keep(start, FocusedInput::TextField);
        assert_eq!(
            recent.kept(start + REUSE / 2),
            Some(FocusedInput::TextField)
        );
        assert_eq!(recent.kept(start + REUSE), None);
        assert_eq!(recent.kept(start + REUSE * 3), None);
    }
}

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
//!   Slint window 7 to 31,
//! - with the switch of the bus (`org.a11y.Status.IsEnabled`) off, GTK applications still
//!   show their trees, and Slint, Qt, Firefox and Chromium do not show a window at all.
//!   Chromium, switch on, showed a window with no children in the run made.
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
/// questions each, a few hundred milliseconds. A look that runs out is `Unknown`, never a
/// palette that hangs.
pub(crate) const LOOK_WAIT: Duration = Duration::from_millis(900);

/// How many elements one look may visit, over all windows in front. A page with thousands
/// of elements is not walked to its end.
pub(crate) const MAX_ELEMENTS: usize = 1500;

/// How long an answer is kept for the next question. A press asks three times in a few
/// milliseconds (the target line, the refusal of a button, the clearing), and a look that
/// walks a browser is not made three times. The answer of the last look is the one of this
/// press only while it is this fresh, and a press a second later looks again.
pub(crate) const REUSE: Duration = Duration::from_millis(300);

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

/// The tree of the bus, as far as the look asks it. Every question may get no answer: an
/// application that hangs or has gone is not a reason to stop looking at the others.
pub(crate) trait Tree {
    type Node;

    /// The applications that show a tree.
    fn applications(&mut self) -> Option<Vec<Self::Node>>;
    /// The children of an element.
    fn children(&mut self, node: &Self::Node) -> Option<Vec<Self::Node>>;
    /// The states of an element, as `GetState` words them.
    fn states(&mut self, node: &Self::Node) -> Option<Vec<u32>>;
    /// The role of an element.
    fn role(&mut self, node: &Self::Node) -> Option<u32>;
    /// Whether there is time for another question.
    fn time_left(&self) -> bool;
}

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
    /// The time or the number of elements ran out before the answer.
    Spent,
}

/// What a walk down one window found.
enum Walk<N> {
    Holder(N, Vec<u32>),
    Nothing,
    Spent,
}

/// Looks for the element that holds the keyboard focus, in the windows in front.
pub(crate) fn find<T: Tree>(tree: &mut T) -> Found {
    let Some(applications) = tree.applications() else {
        return Found::NoWindow;
    };
    let mut windows = Vec::new();
    for application in applications {
        if !tree.time_left() {
            return Found::Spent;
        }
        let Some(tops) = tree.children(&application) else {
            continue;
        };
        for top in tops {
            if tree
                .states(&top)
                .is_some_and(|states| has(&states, state::ACTIVE))
            {
                windows.push(top);
            }
        }
    }
    if windows.is_empty() {
        return Found::NoWindow;
    }
    let mut left = MAX_ELEMENTS;
    let mut holders = Vec::new();
    for window in windows {
        match walk(tree, window, &mut left) {
            Walk::Holder(node, states) => holders.push((node, states)),
            Walk::Nothing => {}
            Walk::Spent => return Found::Spent,
        }
    }
    match holders.pop() {
        None => Found::NoFocus,
        Some(_) if !holders.is_empty() => Found::Several,
        Some((node, states)) => Found::Kind(
            tree.role(&node)
                .map_or(FocusedInput::Unknown, |role| kind_of(role, &states)),
        ),
    }
}

/// Depth first down `window`, to the first element with the focus.
fn walk<T: Tree>(tree: &mut T, window: T::Node, left: &mut usize) -> Walk<T::Node> {
    let mut stack = vec![window];
    while let Some(node) = stack.pop() {
        if *left == 0 || !tree.time_left() {
            return Walk::Spent;
        }
        *left -= 1;
        let Some(states) = tree.states(&node) else {
            continue;
        };
        if has(&states, state::DEFUNCT) {
            continue;
        }
        if has(&states, state::FOCUSED) {
            return Walk::Holder(node, states);
        }
        if let Some(children) = tree.children(&node) {
            stack.extend(children.into_iter().rev());
        }
    }
    Walk::Nothing
}

/// The answer in the words of the question, given whether the bus is switched on.
///
/// A look that found no window in front or nothing with the focus, with the switch of the
/// bus off, is an application that shows nothing because it was not asked to: that is
/// `ClosedToAccessibility`, and the person is told so. With the switch on the same
/// finding is just not recognised. A look that ran out, or found two, says nothing.
pub(crate) fn settle(found: Found, switched_on: bool) -> FocusedInput {
    match found {
        Found::Kind(kind) => kind,
        Found::NoWindow | Found::NoFocus if !switched_on => FocusedInput::ClosedToAccessibility,
        Found::NoWindow | Found::NoFocus | Found::Several | Found::Spent => FocusedInput::Unknown,
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

    /// An element of a made-up tree: its role, its states and its children by position.
    struct Element {
        role: u32,
        states: Vec<u32>,
        children: Vec<usize>,
        /// Whether it answers at all.
        answers: bool,
    }

    /// A tree in a vector, with the root's children being the applications, and a count of
    /// the questions asked.
    struct Fake {
        elements: Vec<Element>,
        applications: Vec<usize>,
        asked: usize,
        questions_allowed: usize,
    }

    impl Fake {
        fn new() -> Self {
            Self {
                elements: Vec::new(),
                applications: Vec::new(),
                asked: 0,
                questions_allowed: usize::MAX,
            }
        }

        fn add(&mut self, role: u32, names: &[u32], children: &[usize]) -> usize {
            self.elements.push(Element {
                role,
                states: states_of(names),
                children: children.to_vec(),
                answers: true,
            });
            self.elements.len() - 1
        }

        fn mute(&mut self, node: usize) {
            self.elements[node].answers = false;
        }
    }

    impl Tree for Fake {
        type Node = usize;

        fn applications(&mut self) -> Option<Vec<usize>> {
            Some(self.applications.clone())
        }

        fn children(&mut self, node: &usize) -> Option<Vec<usize>> {
            self.asked += 1;
            let element = &self.elements[*node];
            element.answers.then(|| element.children.clone())
        }

        fn states(&mut self, node: &usize) -> Option<Vec<u32>> {
            self.asked += 1;
            let element = &self.elements[*node];
            element.answers.then(|| element.states.clone())
        }

        fn role(&mut self, node: &usize) -> Option<u32> {
            Some(self.elements[*node].role)
        }

        fn time_left(&self) -> bool {
            self.asked < self.questions_allowed
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
    fn an_application_that_does_not_answer_is_passed_over() {
        let mut tree = Fake::new();
        let hung = tree.add(75, &[], &[]);
        tree.mute(hung);
        tree.applications.push(hung);
        window_with(&mut tree, role::ENTRY, &[state::FOCUSED, state::EDITABLE]);
        assert_eq!(find(&mut tree), Found::Kind(FocusedInput::TextField));
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
        assert_eq!(find(&mut tree), Found::Spent);
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
    fn a_look_that_ran_out_or_found_two_says_nothing_whatever_the_switch() {
        for on in [true, false] {
            assert_eq!(settle(Found::Spent, on), FocusedInput::Unknown);
            assert_eq!(settle(Found::Several, on), FocusedInput::Unknown);
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

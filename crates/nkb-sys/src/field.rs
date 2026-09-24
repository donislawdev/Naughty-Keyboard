//! What holds the keyboard focus in the window in front: a text field, surely
//! not a text field, or something this module cannot tell.
//!
//! # Why the question is asked at all
//!
//! The palette clears before it types - `Home`, `Shift+End`, `Delete` (`D54`) -
//! and that recipe keeps its promise "clearing never reaches beyond the field"
//! only while the focus IS in a field. With the focus on a list of files the
//! same three keys select from the first file to the last and delete them, on a
//! button a space from the value presses it, and on a web page with single-key
//! shortcuts every letter is a command (`OBS-135`). So the direct route asks
//! before pressing anything, the same way it asks about privileges (`D72`).
//!
//! # The measurement this stands on
//!
//! Probed 2026-09-24 on Windows 11 against applications with a KNOWN focus -
//! classic Win32 controls, Edge (Chromium), a Slint window, the console and a
//! File Explorer list - 24 readings, every one with the probed window in front:
//!
//! - the Win32 answers (`GetGUIThreadInfo`: the focus window and the caret) tell
//!   a field from a button only for classic controls. Chromium reports its one
//!   window and NO caret, in a text input and on a button alike - so the reading
//!   the documents planned (`OBS-42`: attach to the thread, ask `GetFocus`)
//!   cannot see the case testers meet most often,
//! - UI Automation tells them apart: `Edit` for inputs and text areas, `Group`
//!   with the Text pattern for `contenteditable`, `Button`, `Hyperlink`,
//!   `ListItem` (File Explorer too), `ComboBox` for a `select`, and a READ-ONLY
//!   `Document` for the page itself.
//!
//! The readings are the fixtures of the tests below, row for row.
//!
//! ⚠️ Corrected the same day: that probe also recorded Slint as `Pane` and this
//! text built on it. It was the probe's own doing - `Process.MainWindowHandle`
//! of a winit application is its hidden `Winit Thread Event Target`, the probe
//! brought THAT window to the front and the keyboard focus went with it. With
//! the real window in front, Slint answers `Edit` with both patterns from the
//! first request (measured twice, `tools/sonda-pole/zimne.ps1`).
//!
//! # A provider that is still waking up
//!
//! Measured 2026-09-24, four times out of four: a Chromium window nobody has
//! asked before answers the first requests with a placeholder - `Pane`, no
//! pattern, no native window - for 180-220 ms while it builds its tree, and
//! only then `Button` or `Edit`. Taken at its word, that `Pane` is `Unknown`
//! and the first press in a fresh browser typed into a button
//! (`tools/brak-pola.ps1`, three times out of three). So a `Pane` without a
//! native window is asked again, every [`LOOK_STEP_MS`], for at most
//! [`SECOND_LOOK_MS`], and the last answer is the one classified. Classic
//! controls come with a window, and Slint and a warm browser never answer
//! `Pane` for their fields, so nobody else waits. That `Pane` stays `Unknown`
//! for the same reason: a waking browser gives it for a text input too.
//!
//! # Three answers, and the third is never folded into the second
//!
//! [`FocusedInput::NotTextField`] is claimed only for control types that take no
//! text by definition, and it is the only answer that stops a send. Everything
//! the module does not recognise, every read that fails or times out, and every
//! element that belongs to another process than the window in front is
//! [`FocusedInput::Unknown`] - and `Unknown` presses exactly as before this
//! module existed. Guessing a cause the tool did not recognise is what
//! `product-spec.md` 9.3 forbids.
//!
//! # What this reads, and what it does not
//!
//! Six properties of ONE element, the one holding the focus: its process, its
//! control type, whether it has the Value pattern and whether that is
//! read-only, whether it has the Text pattern, and whether it is a window of
//! its own (only compared with zero, for the second look above). Never its
//! name, never its value, never its text, never any other element.
//! `tests/field_reads_only_kinds.rs` holds the list and goes red when a
//! property is added to it - the promise "does not read the contents of
//! windows" (untouchable rule 17) rests on it.
//!
//! # Cost, and a hung application
//!
//! Measured through the managed client: 2-25 ms a read once the client exists,
//! ~100 ms for the first on a thread. The client is made once per thread and
//! kept. An application that does not answer would hold a UIA call for its
//! default timeouts - seconds - so both are set to [`TIMEOUT_MS`], and a
//! timeout is `Unknown`, never a frozen palette.

use crate::WindowRef;
use std::time::{Duration, Instant};

/// What holds the keyboard focus in a window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusedInput {
    /// A control that takes typed text: an edit box, an editable document, a
    /// `contenteditable` region, a console.
    TextField,
    /// A control that takes no text by definition - a button, a link, a list
    /// item, a page that is not editable. Keys sent there act on the control.
    NotTextField,
    /// Not recognised, not readable, not answered in time, or not in the window
    /// in front. The send goes ahead exactly as before.
    Unknown,
}

/// How long one UI Automation request may take before the answer is `Unknown`.
///
/// ⚠️ An estimate, not a measurement of a hung application: ten times the
/// slowest read measured (25 ms), short enough that a tester does not feel a
/// press hang. Applies to making the connection and to each request separately.
pub const TIMEOUT_MS: u32 = 500;

/// How long a provider that may still be waking up is asked again.
///
/// ⚠️ Chosen from one measurement, not from a hung application: the waking
/// browser answered within 220 ms on this machine, and this is a bit more than
/// twice that. A heavier page or a slower machine may take longer, and then the
/// answer is `Unknown` and the send goes ahead as it did before the second
/// look existed.
pub const SECOND_LOOK_MS: u64 = 500;

/// The pause between two looks - the probe asked every 40 ms and saw the
/// answer change between the third and the fourth request.
pub const LOOK_STEP_MS: u64 = 40;

/// What holds the keyboard focus in `window`, asked fresh.
///
/// Returns [`FocusedInput::Unknown`] on systems without UI Automation.
#[must_use]
pub fn focused_input(window: WindowRef) -> FocusedInput {
    let Some(first) = platform::facts(window) else {
        return FocusedInput::Unknown;
    };
    let deadline = Instant::now() + Duration::from_millis(SECOND_LOOK_MS);
    let again = || {
        std::thread::sleep(Duration::from_millis(LOOK_STEP_MS));
        platform::facts(window)
    };
    settle(first, again, || Instant::now() < deadline)
        .map_or(FocusedInput::Unknown, |facts| classify(&facts))
}

/// What UI Automation said about the focused element - all of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Facts {
    control_type: i32,
    /// `Some(read_only)` when the element has the Value pattern.
    value: Option<bool>,
    /// Whether the element has the Text pattern.
    text: bool,
    /// Whether the element is a native window of its own. Chromium's elements
    /// and Slint's are not, classic controls are.
    windowed: bool,
}

/// Whether an answer looks like a provider that has not built its tree yet:
/// a `Pane` that is not a window of its own. See the module text.
fn worth_a_second_look(facts: &Facts) -> bool {
    facts.control_type == control::PANE && !facts.windowed
}

/// The answer to classify: `first`, or - while it looks like a waking
/// provider and `time_left` says so - whatever `again` reads next. A read that
/// fails on the way is `None`, the same as a first read that fails: the focus
/// may have left the window, and then nothing is known about it.
fn settle(
    first: Facts,
    mut again: impl FnMut() -> Option<Facts>,
    mut time_left: impl FnMut() -> bool,
) -> Option<Facts> {
    let mut facts = first;
    while worth_a_second_look(&facts) && time_left() {
        facts = again()?;
    }
    Some(facts)
}

/// UI Automation control type identifiers.
///
/// Fixed by the platform (`UIAutomationClient.h`), repeated here so the rule
/// compiles and is tested on every system. A Windows-only test pins each one to
/// the `windows-sys` constant of the same name, so they cannot drift.
mod control {
    pub const BUTTON: i32 = 50000;
    pub const CHECK_BOX: i32 = 50002;
    pub const COMBO_BOX: i32 = 50003;
    pub const EDIT: i32 = 50004;
    pub const HYPERLINK: i32 = 50005;
    pub const IMAGE: i32 = 50006;
    pub const LIST_ITEM: i32 = 50007;
    pub const LIST: i32 = 50008;
    pub const MENU: i32 = 50009;
    pub const MENU_BAR: i32 = 50010;
    pub const MENU_ITEM: i32 = 50011;
    pub const PROGRESS_BAR: i32 = 50012;
    pub const RADIO_BUTTON: i32 = 50013;
    pub const SCROLL_BAR: i32 = 50014;
    pub const SLIDER: i32 = 50015;
    pub const STATUS_BAR: i32 = 50017;
    pub const TAB: i32 = 50018;
    pub const TAB_ITEM: i32 = 50019;
    pub const TOOL_BAR: i32 = 50021;
    pub const TOOL_TIP: i32 = 50022;
    pub const TREE: i32 = 50023;
    pub const TREE_ITEM: i32 = 50024;
    pub const GROUP: i32 = 50026;
    pub const THUMB: i32 = 50027;
    pub const DOCUMENT: i32 = 50030;
    pub const SPLIT_BUTTON: i32 = 50031;
    // Named for the tests, which pin that it stays `Unknown`: the rule itself
    // never mentions it, and that silence is the point.
    #[cfg_attr(not(test), allow(dead_code))]
    pub const PANE: i32 = 50033;
    pub const HEADER: i32 = 50034;
    pub const HEADER_ITEM: i32 = 50035;
    pub const TITLE_BAR: i32 = 50037;
    pub const SEPARATOR: i32 = 50038;
}

/// Control types that take no text by definition, whatever else they carry.
///
/// Measured in the probe: `BUTTON`, `CHECK_BOX`, `HYPERLINK`, `LIST_ITEM`,
/// `SLIDER`. The rest by what the type is: a list or tree itself (the same
/// clearing hazard as its items - `Home`, `Shift+End` select them all), menus
/// (letters are mnemonics), tabs, headers (space sorts), and controls that show
/// rather than take (an image, a progress bar, a status bar, a tooltip).
///
/// 🔴 Deliberately NOT here: `Pane` (a waking browser, text inputs too), `Custom`, `Window`, `Text`
/// (terminals may report it), `Spinner` and `Calendar` (they take digits and
/// dates), and the grid family - `DataGrid`, `DataItem`, `Table` - because a
/// spreadsheet takes typing in its cells and none of it was measured.
const NOT_TEXT: &[i32] = &[
    control::BUTTON,
    control::CHECK_BOX,
    control::HYPERLINK,
    control::IMAGE,
    control::LIST_ITEM,
    control::LIST,
    control::MENU,
    control::MENU_BAR,
    control::MENU_ITEM,
    control::PROGRESS_BAR,
    control::RADIO_BUTTON,
    control::SCROLL_BAR,
    control::SLIDER,
    control::STATUS_BAR,
    control::TAB,
    control::TAB_ITEM,
    control::TOOL_BAR,
    control::TOOL_TIP,
    control::TREE,
    control::TREE_ITEM,
    control::THUMB,
    control::SPLIT_BUTTON,
    control::HEADER,
    control::HEADER_ITEM,
    control::TITLE_BAR,
    control::SEPARATOR,
];

/// The decision, apart from the reading, so it can be checked on every system.
fn classify(facts: &Facts) -> FocusedInput {
    match facts.control_type {
        control::EDIT => FocusedInput::TextField,
        // A rich edit box is a writable `Document`, a console one with the Text
        // pattern and no Value pattern - both take typing. A web page that is
        // not editable is a READ-ONLY `Document`, and there a letter can be a
        // command of the application under test.
        control::DOCUMENT => match (facts.value, facts.text) {
            (Some(true), _) => FocusedInput::NotTextField,
            (Some(false), _) | (None, true) => FocusedInput::TextField,
            (None, false) => FocusedInput::Unknown,
        },
        // `contenteditable` in Chromium. A group without text is a container
        // of something else, and the module does not know of what.
        control::GROUP if facts.text => FocusedInput::TextField,
        // An editable combo box passes the focus to its edit box, so a focused
        // combo box without text is a closed list - a `select` in a browser.
        control::COMBO_BOX if facts.text => FocusedInput::TextField,
        control::COMBO_BOX => FocusedInput::NotTextField,
        other if NOT_TEXT.contains(&other) => FocusedInput::NotTextField,
        _ => FocusedInput::Unknown,
    }
}

#[cfg(windows)]
mod platform {
    use super::{Facts, TIMEOUT_MS};
    use crate::WindowRef;
    use core::ffi::c_void;
    use core::ptr::{null, null_mut};
    use std::cell::RefCell;
    use windows_sys::Win32::Foundation::{HWND, RPC_E_CHANGED_MODE};
    use windows_sys::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
        CoUninitialize,
    };
    use windows_sys::Win32::System::Variant::{VARIANT, VT_BOOL, VT_I4, VariantClear};
    use windows_sys::Win32::UI::Accessibility::CUIAutomation8;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;
    use windows_sys::core::{GUID, HRESULT};

    /// `IUIAutomation2`, which adds the two timeouts to `IUIAutomation`.
    const IID_IUIAUTOMATION2: GUID = GUID::from_u128(0x34723aff_0c9d_49d0_9896_7ab52df8cd8a);

    // The six properties read, and the only ones - see the module text and
    // `tests/field_reads_only_kinds.rs`, which holds this list.
    pub(super) const PROPERTY_PROCESS_ID: i32 = 30002;
    pub(super) const PROPERTY_CONTROL_TYPE: i32 = 30003;
    pub(super) const PROPERTY_NATIVE_WINDOW_HANDLE: i32 = 30020;
    pub(super) const PROPERTY_IS_TEXT_PATTERN_AVAILABLE: i32 = 30040;
    pub(super) const PROPERTY_IS_VALUE_PATTERN_AVAILABLE: i32 = 30043;
    pub(super) const PROPERTY_VALUE_IS_READ_ONLY: i32 = 30046;

    // Positions in the COM method tables, counted from the start of each
    // table: `IUnknown` takes 0-2. Taken from the `windows` crate 0.62.2
    // (`IUIAutomation_Vtbl`, `IUIAutomation2_Vtbl`, `IUIAutomationElement_Vtbl`)
    // rather than counted from memory, 2026-09-24.
    const SLOT_RELEASE: usize = 2;
    const SLOT_GET_FOCUSED_ELEMENT: usize = 8;
    const SLOT_GET_CURRENT_PROPERTY_VALUE: usize = 10;
    const SLOT_SET_CONNECTION_TIMEOUT: usize = 61;
    const SLOT_SET_TRANSACTION_TIMEOUT: usize = 63;

    type Raw = *mut c_void;

    /// The function at `slot` of `object`'s method table.
    ///
    /// # Safety
    ///
    /// `object` must be a live COM interface pointer whose table has `slot`,
    /// and `F` must be that method's exact signature.
    unsafe fn method<F: Copy>(object: Raw, slot: usize) -> F {
        let table = unsafe { *object.cast::<*const *const c_void>() };
        let entry = unsafe { *table.add(slot) };
        unsafe { core::mem::transmute_copy::<*const c_void, F>(&entry) }
    }

    /// A COM reference this module holds, released on every path out.
    struct Com(Raw);

    impl Drop for Com {
        fn drop(&mut self) {
            let release: unsafe extern "system" fn(Raw) -> u32 =
                unsafe { method(self.0, SLOT_RELEASE) };
            unsafe { release(self.0) };
        }
    }

    /// One `CoInitializeEx` this thread owns, undone when the client goes.
    struct Apartment {
        owned: bool,
    }

    impl Drop for Apartment {
        fn drop(&mut self) {
            if self.owned {
                unsafe { CoUninitialize() };
            }
        }
    }

    /// The UI Automation client of this thread. Field order is drop order:
    /// the reference goes before the apartment it lives in.
    struct Client {
        automation: Com,
        _apartment: Apartment,
    }

    thread_local! {
        static CLIENT: RefCell<Option<Client>> = const { RefCell::new(None) };
    }

    impl Client {
        fn new() -> Option<Self> {
            let joined = unsafe { CoInitializeEx(null(), COINIT_MULTITHREADED as u32) };
            // S_OK and S_FALSE both count and both must be undone. A thread that
            // chose a single-threaded apartment before us answers
            // RPC_E_CHANGED_MODE: UI Automation works there too, and there is
            // nothing of ours to undo.
            let apartment = Apartment { owned: joined >= 0 };
            if joined < 0 && joined != RPC_E_CHANGED_MODE {
                return None;
            }
            let mut raw: Raw = null_mut();
            let made: HRESULT = unsafe {
                CoCreateInstance(
                    &CUIAutomation8,
                    null_mut(),
                    CLSCTX_INPROC_SERVER,
                    &IID_IUIAUTOMATION2,
                    &mut raw,
                )
            };
            if made < 0 || raw.is_null() {
                return None;
            }
            let automation = Com(raw);
            // Without both timeouts a hung application would hold the press for
            // seconds, so a client that cannot set them is not used at all.
            let set_connection: unsafe extern "system" fn(Raw, u32) -> HRESULT =
                unsafe { method(raw, SLOT_SET_CONNECTION_TIMEOUT) };
            let set_transaction: unsafe extern "system" fn(Raw, u32) -> HRESULT =
                unsafe { method(raw, SLOT_SET_TRANSACTION_TIMEOUT) };
            if unsafe { set_connection(raw, TIMEOUT_MS) } < 0
                || unsafe { set_transaction(raw, TIMEOUT_MS) } < 0
            {
                return None;
            }
            Some(Self {
                automation,
                _apartment: apartment,
            })
        }

        fn facts(&self, process: u32) -> Option<Facts> {
            let get_focused: unsafe extern "system" fn(Raw, *mut Raw) -> HRESULT =
                unsafe { method(self.automation.0, SLOT_GET_FOCUSED_ELEMENT) };
            let mut raw: Raw = null_mut();
            if unsafe { get_focused(self.automation.0, &mut raw) } < 0 || raw.is_null() {
                return None;
            }
            let element = Com(raw);
            // The focus of the SYSTEM, which is not always inside the window in
            // front - a race, a focus left in another application. An element
            // of another process says nothing about this window.
            let owner = u32::try_from(int_property(&element, PROPERTY_PROCESS_ID)?).ok()?;
            if owner != process {
                return None;
            }
            let control_type = int_property(&element, PROPERTY_CONTROL_TYPE)?;
            let value = if bool_property(&element, PROPERTY_IS_VALUE_PATTERN_AVAILABLE)? {
                Some(bool_property(&element, PROPERTY_VALUE_IS_READ_ONLY)?)
            } else {
                None
            };
            let text = bool_property(&element, PROPERTY_IS_TEXT_PATTERN_AVAILABLE)?;
            // Compared with zero and dropped - the handle itself is not kept.
            let windowed = int_property(&element, PROPERTY_NATIVE_WINDOW_HANDLE)? != 0;
            Some(Facts {
                control_type,
                value,
                text,
                windowed,
            })
        }
    }

    /// A `VARIANT` this module received, cleared on every path out - a
    /// property UIA does not support comes back as an object reference.
    struct Variant(VARIANT);

    impl Drop for Variant {
        fn drop(&mut self) {
            unsafe { VariantClear(&mut self.0) };
        }
    }

    fn property(element: &Com, id: i32) -> Option<Variant> {
        let get: unsafe extern "system" fn(Raw, i32, *mut VARIANT) -> HRESULT =
            unsafe { method(element.0, SLOT_GET_CURRENT_PROPERTY_VALUE) };
        let mut value = Variant(VARIANT::default());
        if unsafe { get(element.0, id, &mut value.0) } < 0 {
            return None;
        }
        Some(value)
    }

    // The tag is checked BEFORE the union is read, and written as a plain `if`
    // on purpose: `then_some` would read the member first and compare after.
    fn int_property(element: &Com, id: i32) -> Option<i32> {
        let value = property(element, id)?;
        let inner = unsafe { &value.0.Anonymous.Anonymous };
        if inner.vt != VT_I4 {
            return None;
        }
        Some(unsafe { inner.Anonymous.lVal })
    }

    fn bool_property(element: &Com, id: i32) -> Option<bool> {
        let value = property(element, id)?;
        let inner = unsafe { &value.0.Anonymous.Anonymous };
        if inner.vt != VT_BOOL {
            return None;
        }
        Some(unsafe { inner.Anonymous.boolVal } != 0)
    }

    pub fn facts(window: WindowRef) -> Option<Facts> {
        let mut process = 0u32;
        unsafe { GetWindowThreadProcessId(window.0 as usize as HWND, &mut process) };
        if process == 0 {
            return None;
        }
        CLIENT.with_borrow_mut(|slot| {
            if slot.is_none() {
                *slot = Client::new();
            }
            slot.as_ref()?.facts(process)
        })
    }
}

#[cfg(not(windows))]
mod platform {
    use super::Facts;
    use crate::WindowRef;

    // No route to type into other windows exists here yet (`send_text` says so
    // by name), so there is no focus to ask about. `None` makes the answer
    // `Unknown`, never a claim this build cannot see.
    pub fn facts(_window: WindowRef) -> Option<Facts> {
        None
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

    fn facts(control_type: i32, value: Option<bool>, text: bool) -> Facts {
        Facts {
            control_type,
            value,
            text,
            windowed: false,
        }
    }

    fn windowed(facts: Facts) -> Facts {
        Facts {
            windowed: true,
            ..facts
        }
    }

    const RW: Option<bool> = Some(false);
    const RO: Option<bool> = Some(true);

    /// The probe of 2026-09-24, row for row: what UI Automation said with the
    /// probed window in front, and what the focus really was. A change to the
    /// rule that turns any of these around is a change to what was MEASURED.
    #[test]
    fn every_measured_focus_is_answered_as_it_was() {
        use FocusedInput::{NotTextField, TextField, Unknown};
        let rows: &[(&str, Facts, FocusedInput)] = &[
            // Classic Win32 controls, through WinForms.
            ("edit box", facts(control::EDIT, RW, true), TextField),
            (
                "multi-line edit box",
                facts(control::EDIT, RW, true),
                TextField,
            ),
            (
                "rich edit box",
                facts(control::DOCUMENT, RW, true),
                TextField,
            ),
            (
                "read-only edit box",
                facts(control::EDIT, RO, true),
                TextField,
            ),
            ("button", facts(control::BUTTON, None, false), NotTextField),
            (
                "check box",
                facts(control::CHECK_BOX, None, false),
                NotTextField,
            ),
            (
                "list view item",
                facts(control::LIST_ITEM, None, false),
                NotTextField,
            ),
            (
                "editable combo box",
                facts(control::EDIT, RW, true),
                TextField,
            ),
            (
                "drop-down list",
                facts(control::COMBO_BOX, RW, false),
                NotTextField,
            ),
            // Recorded as "Slint", but the probe had brought winit's hidden event
            // target to the front, and the focus went there with it. Kept as a
            // reading of a Pane - with the real window in front Slint answers
            // Edit (module text).
            (
                "slint text input",
                facts(control::PANE, None, false),
                Unknown,
            ),
            (
                "slint read-only input",
                facts(control::PANE, None, false),
                Unknown,
            ),
            // Edge (Chromium).
            ("web input", facts(control::EDIT, RW, true), TextField),
            ("web password", facts(control::EDIT, RW, true), TextField),
            (
                "web read-only input",
                facts(control::EDIT, RO, true),
                TextField,
            ),
            ("web textarea", facts(control::EDIT, RW, true), TextField),
            (
                "web contenteditable",
                facts(control::GROUP, None, true),
                TextField,
            ),
            (
                "web button",
                facts(control::BUTTON, None, false),
                NotTextField,
            ),
            (
                "web link",
                facts(control::HYPERLINK, RO, false),
                NotTextField,
            ),
            (
                "web select",
                facts(control::COMBO_BOX, RW, false),
                NotTextField,
            ),
            (
                "web check box",
                facts(control::CHECK_BOX, RW, false),
                NotTextField,
            ),
            ("web range", facts(control::SLIDER, RW, false), NotTextField),
            (
                "web page, nothing focused",
                facts(control::DOCUMENT, RO, true),
                NotTextField,
            ),
            // The console, and a File Explorer list of files.
            ("console", facts(control::DOCUMENT, None, true), TextField),
            (
                "file explorer list",
                facts(control::LIST_ITEM, RW, false),
                NotTextField,
            ),
        ];
        for (what, reading, expected) in rows {
            assert_eq!(classify(reading), *expected, "{what}: {reading:?}");
        }
    }

    #[test]
    fn only_a_type_that_takes_no_text_stops_a_send() {
        // Everything outside the known list is Unknown, including the types a
        // spreadsheet or a custom widget reports - the send goes ahead there.
        for unknown in [
            50001, 50016, 50020, 50025, 50028, 50029, 50032, 50036, 50039, 50040,
        ] {
            assert_eq!(
                classify(&facts(unknown, None, false)),
                FocusedInput::Unknown,
                "control type {unknown}"
            );
        }
        // A type nobody knows yet, from a future version of the platform.
        assert_eq!(classify(&facts(59_999, None, false)), FocusedInput::Unknown);
        // A group or a document with nothing to say is not a claim either way.
        assert_eq!(
            classify(&facts(control::GROUP, None, false)),
            FocusedInput::Unknown
        );
        assert_eq!(
            classify(&facts(control::DOCUMENT, None, false)),
            FocusedInput::Unknown
        );
    }

    #[test]
    fn no_type_is_both_a_field_and_not_one() {
        // `EDIT`, `DOCUMENT`, `GROUP` and `COMBO_BOX` are decided by their
        // patterns, and the list of types that take no text must not name them,
        // or the order of the match arms would decide instead of the rule.
        for decided_by_patterns in [
            control::EDIT,
            control::DOCUMENT,
            control::GROUP,
            control::COMBO_BOX,
        ] {
            assert!(
                !NOT_TEXT.contains(&decided_by_patterns),
                "{decided_by_patterns}"
            );
        }
        assert!(
            !NOT_TEXT.contains(&control::PANE),
            "Pane is how a waking browser reports a text input"
        );
    }

    /// `tools/sonda-pole/zimne.ps1`, 2026-09-24, every answer that differed:
    /// a fresh Edge gave `Pane` without a window for its first requests, then
    /// the real control. A change to the rule that makes any of these wait, or
    /// stop waiting, is a change to what was MEASURED.
    #[test]
    fn only_a_pane_without_a_window_is_asked_again() {
        let rows: &[(&str, Facts, bool)] = &[
            (
                "fresh Edge, any focus, first 180-220 ms",
                facts(control::PANE, None, false),
                true,
            ),
            (
                "Edge a moment later, button",
                facts(control::BUTTON, None, false),
                false,
            ),
            (
                "Edge a moment later, input",
                facts(control::EDIT, RW, true),
                false,
            ),
            (
                "Slint, first request",
                facts(control::EDIT, RW, true),
                false,
            ),
            (
                "WinForms button",
                windowed(facts(control::BUTTON, None, false)),
                false,
            ),
            (
                "a native pane - a window of its own",
                windowed(facts(control::PANE, None, false)),
                false,
            ),
        ];
        for (what, reading, expected) in rows {
            assert_eq!(worth_a_second_look(reading), *expected, "{what}");
        }
    }

    #[test]
    fn a_waking_provider_is_classified_by_its_first_real_answer() {
        // Pane, Pane, then the button - the order the probe saw. The answer
        // is the button's, so the send is refused instead of typed into it.
        let mut answers = vec![
            facts(control::BUTTON, None, false),
            facts(control::PANE, None, false),
        ];
        let mut asked = 0;
        let settled = settle(
            facts(control::PANE, None, false),
            || {
                asked += 1;
                answers.pop()
            },
            || true,
        )
        .expect("every read succeeded");
        assert_eq!(asked, 2);
        assert_eq!(classify(&settled), FocusedInput::NotTextField);
    }

    #[test]
    fn a_pane_that_stays_a_pane_is_unknown_when_the_time_is_up() {
        let mut asked = 0;
        let mut looks_left = 3;
        let settled = settle(
            facts(control::PANE, None, false),
            || {
                asked += 1;
                Some(facts(control::PANE, None, false))
            },
            || {
                looks_left -= 1;
                looks_left >= 0
            },
        )
        .expect("every read succeeded");
        assert_eq!(asked, 3, "asked again exactly while time was left");
        assert_eq!(classify(&settled), FocusedInput::Unknown);
    }

    #[test]
    fn an_answer_that_needs_no_second_look_is_not_asked_again() {
        let mut asked = 0;
        let settled = settle(
            facts(control::EDIT, RW, true),
            || {
                asked += 1;
                None
            },
            || true,
        );
        assert_eq!(asked, 0);
        assert_eq!(settled, Some(facts(control::EDIT, RW, true)));
        // A window of its own is a native pane, not a waking provider.
        let native = windowed(facts(control::PANE, None, false));
        assert_eq!(settle(native, || None, || true), Some(native));
    }

    #[test]
    fn a_read_that_fails_during_the_second_look_is_unknown() {
        // The focus left the window while the browser was waking up: nothing
        // is known about where it went, so nothing is claimed.
        let settled = settle(facts(control::PANE, None, false), || None, || true);
        assert_eq!(settled, None);
    }

    #[cfg(windows)]
    #[test]
    fn the_control_type_numbers_are_the_platform_s() {
        use windows_sys::Win32::UI::Accessibility as uia;
        let pairs = [
            (control::BUTTON, uia::UIA_ButtonControlTypeId),
            (control::CHECK_BOX, uia::UIA_CheckBoxControlTypeId),
            (control::COMBO_BOX, uia::UIA_ComboBoxControlTypeId),
            (control::EDIT, uia::UIA_EditControlTypeId),
            (control::HYPERLINK, uia::UIA_HyperlinkControlTypeId),
            (control::IMAGE, uia::UIA_ImageControlTypeId),
            (control::LIST_ITEM, uia::UIA_ListItemControlTypeId),
            (control::LIST, uia::UIA_ListControlTypeId),
            (control::MENU, uia::UIA_MenuControlTypeId),
            (control::MENU_BAR, uia::UIA_MenuBarControlTypeId),
            (control::MENU_ITEM, uia::UIA_MenuItemControlTypeId),
            (control::PROGRESS_BAR, uia::UIA_ProgressBarControlTypeId),
            (control::RADIO_BUTTON, uia::UIA_RadioButtonControlTypeId),
            (control::SCROLL_BAR, uia::UIA_ScrollBarControlTypeId),
            (control::SLIDER, uia::UIA_SliderControlTypeId),
            (control::STATUS_BAR, uia::UIA_StatusBarControlTypeId),
            (control::TAB, uia::UIA_TabControlTypeId),
            (control::TAB_ITEM, uia::UIA_TabItemControlTypeId),
            (control::TOOL_BAR, uia::UIA_ToolBarControlTypeId),
            (control::TOOL_TIP, uia::UIA_ToolTipControlTypeId),
            (control::TREE, uia::UIA_TreeControlTypeId),
            (control::TREE_ITEM, uia::UIA_TreeItemControlTypeId),
            (control::GROUP, uia::UIA_GroupControlTypeId),
            (control::THUMB, uia::UIA_ThumbControlTypeId),
            (control::DOCUMENT, uia::UIA_DocumentControlTypeId),
            (control::SPLIT_BUTTON, uia::UIA_SplitButtonControlTypeId),
            (control::PANE, uia::UIA_PaneControlTypeId),
            (control::HEADER, uia::UIA_HeaderControlTypeId),
            (control::HEADER_ITEM, uia::UIA_HeaderItemControlTypeId),
            (control::TITLE_BAR, uia::UIA_TitleBarControlTypeId),
            (control::SEPARATOR, uia::UIA_SeparatorControlTypeId),
        ];
        for (ours, platform) in pairs {
            assert_eq!(ours, platform);
        }
        let properties = [
            (platform::PROPERTY_PROCESS_ID, uia::UIA_ProcessIdPropertyId),
            (
                platform::PROPERTY_CONTROL_TYPE,
                uia::UIA_ControlTypePropertyId,
            ),
            (
                platform::PROPERTY_NATIVE_WINDOW_HANDLE,
                uia::UIA_NativeWindowHandlePropertyId,
            ),
            (
                platform::PROPERTY_IS_TEXT_PATTERN_AVAILABLE,
                uia::UIA_IsTextPatternAvailablePropertyId,
            ),
            (
                platform::PROPERTY_IS_VALUE_PATTERN_AVAILABLE,
                uia::UIA_IsValuePatternAvailablePropertyId,
            ),
            (
                platform::PROPERTY_VALUE_IS_READ_ONLY,
                uia::UIA_ValueIsReadOnlyPropertyId,
            ),
        ];
        for (ours, platform) in properties {
            assert_eq!(ours, platform);
        }
    }

    #[test]
    fn a_window_that_does_not_exist_is_unknown() {
        // A null handle has no process behind it, so no element can belong to
        // it: the answer must be `Unknown`, and the call must come back.
        assert_eq!(focused_input(WindowRef(0)), FocusedInput::Unknown);
    }

    #[test]
    fn the_client_answers_where_the_route_exists() {
        // The whole COM path on this machine, against whatever is in front:
        // any of the three answers is fine, a crash or a hang is not. Asked
        // twice, so the kept client is exercised as well as the new one.
        if !crate::can_send() {
            return;
        }
        if let Some(window) = crate::foreground_window() {
            let _ = focused_input(window);
            let _ = focused_input(window);
        }
    }
}

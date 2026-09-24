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
//! # A terminal is a field whose keys belong to someone else
//!
//! Measured 2026-09-24 (`OBS-141`), nothing pressed: a terminal answers the
//! same with a shell prompt waiting and with a full-screen program running in
//! it - Windows Terminal `Text` with the Text pattern, the VS Code terminal
//! (xterm.js) an `Edit` that takes typing. What `Home`, `Shift+End` and
//! `Delete` do there is decided by the program inside, which UI Automation does
//! not show: a prompt edits its line, a file manager may select to the last
//! file and delete. So a terminal is its own answer, [`FocusedInput::Terminal`]:
//! the value goes as before, the clearing recipe never. It is recognised by the
//! class name of the focused element, from a closed list of classes measured
//! as terminals ([`TERMINAL_CLASSES`]). The classic console gives its element
//! no class name, and its element comes from another process than its window
//! (`OBS-139`), so it stays `Unknown` - and its row, `Document` with the Text
//! pattern alone, is `Unknown` too, never a field.
//!
//! # Four answers, and the unknown is never folded into another
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
//! Seven properties of ONE element, the one holding the focus: its process, its
//! control type, whether it has the Value pattern and whether that is
//! read-only, whether it has the Text pattern, whether it is a window of its
//! own (only compared with zero, for the second look above), and its class
//! name - compared with [`TERMINAL_CLASSES`] where it is read and dropped
//! there, so nothing past that comparison ever holds it. A class name is set
//! by whoever wrote the control, not by what the tester typed. Never its name,
//! never its value, never its text, never any other element.
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
    /// `contenteditable` region.
    TextField,
    /// A terminal: it takes typed text, but what its keys do belongs to the
    /// program running in it, which UI Automation does not show. A value is
    /// sent there, the clearing recipe never (`OBS-141`).
    Terminal,
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
    /// Whether the element's class name is one of [`TERMINAL_CLASSES`]. The
    /// name itself is not kept.
    terminal: bool,
}

/// Class names of the focused element that mean a terminal, each measured
/// (`tools/sonda-czyszczenie/sonda-czyszczenie.ps1`, scenes `terminal` and
/// `vscode`, 2026-09-24).
///
/// - `TermControl` - Windows Terminal, a `Text` element with the Text pattern,
/// - `xterm-helper-textarea` - the hidden text area through which xterm.js
///   takes typing: the VS Code terminal, and by the way xterm.js is built
///   every terminal made with it (not measured beyond VS Code).
///
/// Compared whole and with case, so a class that merely contains one of these
/// is not a terminal. A class missing here makes a terminal answer as it did
/// before this list - the only cost of a gap is a recipe pressed where it
/// should not be, which is why a new entry needs a measurement, not a guess.
pub const TERMINAL_CLASSES: &[&str] = &["TermControl", "xterm-helper-textarea"];

/// Whether a class name, as the UTF-16 units UI Automation hands over, is one
/// of [`TERMINAL_CLASSES`]. Compared unit by unit, so the name is never turned
/// into a string that could outlive the comparison. Called by the Windows
/// reading only - elsewhere there is no focus to ask about - and by the tests.
#[cfg_attr(not(windows), allow(dead_code))]
fn is_terminal_class(units: &[u16]) -> bool {
    TERMINAL_CLASSES
        .iter()
        .any(|name| name.encode_utf16().eq(units.iter().copied()))
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
    // Named for the tests: Windows Terminal answers with it, and the rule
    // decides a terminal by its class, never by this type - static text
    // answers `Text` too.
    #[cfg_attr(not(test), allow(dead_code))]
    pub const TEXT: i32 = 50020;
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
    // Before the type: the VS Code terminal is an `Edit`, and an `Edit` is a
    // field everywhere else.
    if facts.terminal {
        return FocusedInput::Terminal;
    }
    match facts.control_type {
        control::EDIT => FocusedInput::TextField,
        // A rich edit box is a writable `Document`. A web page that is not
        // editable is a READ-ONLY `Document`, and there a letter can be a
        // command of the application under test. A `Document` with the Text
        // pattern and no Value pattern is how the classic console answers - and
        // the console answers so under a full-screen program as well (`OBS-141`),
        // so it is not claimed as a field. Its element comes from another
        // process than its window, so today it never gets this far (`OBS-139`),
        // and this arm keeps that from turning into clearing if it ever does.
        control::DOCUMENT => match (facts.value, facts.text) {
            (Some(true), _) => FocusedInput::NotTextField,
            (Some(false), _) => FocusedInput::TextField,
            (None, _) => FocusedInput::Unknown,
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
    use super::{Facts, TIMEOUT_MS, is_terminal_class};
    use crate::WindowRef;
    use core::ffi::c_void;
    use core::ptr::{null, null_mut};
    use std::cell::RefCell;
    use windows_sys::Win32::Foundation::{HWND, RPC_E_CHANGED_MODE, SysStringLen};
    use windows_sys::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
        CoUninitialize,
    };
    use windows_sys::Win32::System::Variant::{VARIANT, VT_BOOL, VT_BSTR, VT_I4, VariantClear};
    use windows_sys::Win32::UI::Accessibility::CUIAutomation8;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;
    use windows_sys::core::{GUID, HRESULT};

    /// `IUIAutomation2`, which adds the two timeouts to `IUIAutomation`.
    const IID_IUIAUTOMATION2: GUID = GUID::from_u128(0x34723aff_0c9d_49d0_9896_7ab52df8cd8a);

    // The seven properties read, and the only ones - see the module text and
    // `tests/field_reads_only_kinds.rs`, which holds this list.
    pub(super) const PROPERTY_PROCESS_ID: i32 = 30002;
    pub(super) const PROPERTY_CONTROL_TYPE: i32 = 30003;
    pub(super) const PROPERTY_CLASS_NAME: i32 = 30012;
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
            let terminal = class_is_terminal(&element);
            Some(Facts {
                control_type,
                value,
                text,
                windowed,
                terminal,
            })
        }
    }

    /// Whether the element's class name is a terminal's. Read, compared and
    /// released here - the name goes nowhere else. A class that cannot be read
    /// is not a terminal: the element is then decided by its type, as it was
    /// before classes were asked.
    fn class_is_terminal(element: &Com) -> bool {
        let Some(value) = property(element, PROPERTY_CLASS_NAME) else {
            return false;
        };
        let inner = unsafe { &value.0.Anonymous.Anonymous };
        if inner.vt != VT_BSTR {
            return false;
        }
        let name = unsafe { inner.Anonymous.bstrVal };
        if name.is_null() {
            return false;
        }
        // The length the string says it has, not a search for a zero: a BSTR
        // may carry one inside. The units live as long as `value`, which is
        // cleared when this function returns.
        let length = unsafe { SysStringLen(name) } as usize;
        let units = unsafe { core::slice::from_raw_parts(name, length) };
        is_terminal_class(units)
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
            terminal: false,
        }
    }

    fn windowed(facts: Facts) -> Facts {
        Facts {
            windowed: true,
            ..facts
        }
    }

    fn of_a_terminal_class(facts: Facts) -> Facts {
        Facts {
            terminal: true,
            ..facts
        }
    }

    fn units(text: &str) -> Vec<u16> {
        text.encode_utf16().collect()
    }

    const RW: Option<bool> = Some(false);
    const RO: Option<bool> = Some(true);

    /// The probe of 2026-09-24, row for row: what UI Automation said with the
    /// probed window in front, and what the focus really was. A change to the
    /// rule that turns any of these around is a change to what was MEASURED.
    #[test]
    fn every_measured_focus_is_answered_as_it_was() {
        use FocusedInput::{NotTextField, Terminal, TextField, Unknown};
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
            // The console, and a File Explorer list of files. The console row was
            // `TextField` until 2026-09-24: the same reading comes with a
            // full-screen program running in it (`OBS-141`).
            ("console", facts(control::DOCUMENT, None, true), Unknown),
            (
                "file explorer list",
                facts(control::LIST_ITEM, RW, false),
                NotTextField,
            ),
            // `OBS-141`, 2026-09-24: terminals with a prompt and with `less`
            // running in them - the same reading both times, so the same answer.
            (
                "windows terminal, prompt",
                of_a_terminal_class(facts(control::TEXT, None, true)),
                Terminal,
            ),
            (
                "windows terminal, less",
                of_a_terminal_class(facts(control::TEXT, None, true)),
                Terminal,
            ),
            (
                "vs code terminal, prompt",
                of_a_terminal_class(facts(control::EDIT, RW, true)),
                Terminal,
            ),
            (
                "vs code terminal, less",
                of_a_terminal_class(facts(control::EDIT, RW, true)),
                Terminal,
            ),
            (
                "console, less",
                facts(control::DOCUMENT, None, true),
                Unknown,
            ),
        ];
        for (what, reading, expected) in rows {
            assert_eq!(classify(reading), *expected, "{what}: {reading:?}");
        }
    }

    #[test]
    fn a_terminal_class_is_matched_whole_and_with_case() {
        for measured in TERMINAL_CLASSES {
            assert!(is_terminal_class(&units(measured)), "{measured}");
        }
        for not_one in [
            "",
            "termcontrol",
            "TermControl2",
            "xterm-helper-textarea ",
            "helper-textarea",
            // The VS Code EDITOR, which is not a terminal - measured 2026-09-24,
            // when the focus left the terminal for it during the probe.
            "native-edit-context",
            "Edit",
        ] {
            assert!(!is_terminal_class(&units(not_one)), "{not_one:?}");
        }
    }

    #[test]
    fn a_terminal_is_a_terminal_whatever_type_it_reports() {
        // The xterm.js text area is an `Edit`, and an `Edit` is a field
        // everywhere else: the class must be asked before the type.
        for control_type in [control::EDIT, control::TEXT, control::DOCUMENT] {
            assert_eq!(
                classify(&of_a_terminal_class(facts(control_type, RW, true))),
                FocusedInput::Terminal,
                "control type {control_type}"
            );
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
            (control::TEXT, uia::UIA_TextControlTypeId),
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
            (platform::PROPERTY_CLASS_NAME, uia::UIA_ClassNamePropertyId),
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
        // any of the four answers is fine, a crash or a hang is not. Asked
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

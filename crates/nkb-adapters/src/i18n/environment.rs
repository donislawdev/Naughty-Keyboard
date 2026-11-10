//! What the environment would not let the tool do (`ux-spec.md` 6 B): the
//! keyboard a window could not take or hand back, a palette that could not go
//! where it should stand. Out of `i18n.rs`, which stands at the ceiling of
//! `D113` - moved word for word.

use super::fill::fill;

/// Something about the machine or the session that stops the tool doing its job.
///
/// `ux-spec.md` 6 section B announces four of these. One stands today, and the
/// other three keep their announcement mark there rather than being invented
/// here: a key with a sentence nobody can reach is a promise, and the bridge in
/// `tools/sprawdz-kontrakt.py` would not tell the difference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Environment {
    /// The palette could not be made to refuse the keyboard focus.
    FocusNotRefused,
    /// The pack window opened, and the system kept the keyboard somewhere
    /// else - the letters of the search would go to the application under
    /// test (`ux-spec.md` 5.2, measured by `tools/sonda-okno-paczek`).
    PacksFocusNotTaken,
    /// The pack window closed, and the keyboard could not be handed back to
    /// the window it was taken from (`ux-spec.md` 5.2).
    FocusNotReturned,
    /// The shortcuts window opened, and the system kept the keyboard somewhere
    /// else - a combination pressed to record it would go to the application
    /// under test, whose keys the palette does not hold meanwhile (`D90`).
    ShortcutsFocusNotTaken,
    /// The shortcuts window could not keep its menu from opening on
    /// `Alt+Space` (`D91`, `slint.md` 2.37).
    ShortcutsMenuOpen,
    /// The welcome window opened, and the system kept the keyboard somewhere
    /// else - the first value would go to the window in front, not the box
    /// (UX7, `D105`).
    WelcomeFocusNotTaken,
    /// The About window opened, and the system kept the keyboard somewhere
    /// else - Esc would go to the application under test, not close it.
    AboutFocusNotTaken,
    /// The pack folders window opened, and the system kept the keyboard
    /// somewhere else - Esc would go to the application under test (`D175`).
    FoldersFocusNotTaken,
    /// The length limit window opened, and the system kept the keyboard
    /// somewhere else - the digits of the limit would go to the application
    /// under test (`D178`).
    LimitFocusNotTaken,
    /// The palette could not be let into the space of an application in full
    /// screen (macOS), so the shortcuts work and the palette is out of sight
    /// there (`OBS-193`).
    NotOverFullScreen,
    /// The palette stepped aside from the window the desktop shows to ask the person
    /// whether keys may be pressed, and could not get back on top of the other windows
    /// (`D145` point 77), so it may stay behind the application under test.
    NotBackOnTop,
}

/// The pattern for an environment limit. `ux-spec.md` 6, section B.
///
/// The sentence has two halves and both are load-bearing. The first says what
/// the tool could not do, because untouchable rule 1 forbids a run that did less
/// than it promised from looking whole. The second says what the tester can do
/// INSTEAD, and it is a real remedy rather than a consolation: clicking into the
/// field under test moves the focus there, and measured 2026-09-22 the palette
/// does not take it back on its own.
fn pattern_environment(limit: Environment) -> &'static str {
    match limit {
        Environment::FocusNotRefused => {
            "The palette could not refuse the keyboard focus: {reason}. Click into the field you are testing before pressing a shortcut."
        }
        Environment::PacksFocusNotTaken => {
            "The pack window could not take the keyboard focus: {reason}. Click the pack window before typing, or the letters go to the application you are testing."
        }
        Environment::FocusNotReturned => {
            "The keyboard focus could not be returned to the window you were in: {reason}. Click into the field you are testing before pressing a shortcut."
        }
        Environment::ShortcutsFocusNotTaken => {
            "The shortcuts window could not take the keyboard focus: {reason}. Click the shortcuts window before pressing a combination, or it goes to the application you are testing."
        }
        Environment::ShortcutsMenuOpen => {
            "The shortcuts window could not keep its menu shut: {reason}. Press Esc if a menu opens."
        }
        Environment::WelcomeFocusNotTaken => {
            "The welcome window could not take the keyboard focus: {reason}. Click the box in the welcome window before pressing the shortcut."
        }
        Environment::AboutFocusNotTaken => {
            "The About window could not take the keyboard focus: {reason}. Close it with its Close button."
        }
        Environment::FoldersFocusNotTaken => {
            "The pack folders window could not take the keyboard focus: {reason}. Close it with its Close button."
        }
        Environment::LimitFocusNotTaken => {
            "The length limit window could not take the keyboard focus: {reason}. Click it before typing, or the digits go to the application you are testing."
        }
        Environment::NotOverFullScreen => {
            "The palette may stay out of sight while an application is in full screen: {reason}. The shortcuts still work. Leave full screen to see it."
        }
        Environment::NotBackOnTop => {
            "The palette may stay behind the other windows: {reason}. Restart it to bring it back."
        }
    }
}

/// What the tool says when the environment limited it.
///
/// `reason` is the system layer's own wording, for the same reason
/// `startup_failure` passes the graphics library's through: it is the one
/// specific thing in the sentence, and shortening it here would leave a tester
/// with nothing to put in a bug report.
#[must_use]
pub fn environment(limit: Environment, reason: &str) -> String {
    fill(pattern_environment(limit), &[("reason", reason)])
}

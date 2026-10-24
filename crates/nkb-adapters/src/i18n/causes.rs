//! The cause named in the sentence for a window that shows the system nothing
//! inside it (`D125`, `D139`), which is not the same on every system: the
//! tester can change it, so the sentence says where, in the words of the
//! program that holds the setting.

/// Why the window shows nothing, and what the tester can do, ready to end the
/// sentence "... on top of what was there."
///
/// - Windows: Firefox does so while a setting of its own keeps accessibility
///   services out (`OBS-175`).
/// - macOS: a browser built on Chromium and an application built on Electron
///   build their tree for accessibility only when a client asks for it, and a
///   read of the focus is no such request (measured 2026-10-20: Chrome 152
///   answered "no value" to 60 requests over 2.4 s, and `--force-renderer-
///   accessibility` changed nothing). The pace query of a send is one, because
///   it asks the application for its role: after the first value was typed,
///   Chrome, Visual Studio Code and a second send found a text field. So the
///   remedy is the next send, and the sentence says that this is what turns it
///   on, since it changes how the application behaves until it is restarted.
/// - Linux: the bus of accessibility is off for the session (`org.a11y.Status`,
///   `IsEnabled`), and then Firefox, Chromium, Qt and Slint show no window to
///   it at all, while GTK applications still do (measured 2026-10-24, GNOME 50).
///   What switches it on is a client of the bus, such as a screen reader, so the
///   sentence names the fact and not a setting that may not exist on the desktop
///   of the tester.
#[must_use]
pub fn closed_window() -> &'static str {
    if cfg!(target_os = "macos") {
        "Chrome and other Chromium browsers, and applications built on Electron, do this until their accessibility is on, and typing a value switches it on there until they are restarted - so the next send has the line cleared."
    } else if cfg!(target_os = "linux") {
        "Firefox, Chromium, Qt and Slint applications show their fields to this program only while the accessibility of the desktop is on, as it is while a screen reader runs, and it is off now - so the line was not cleared."
    } else {
        "Firefox does this while \"Prevent accessibility services from accessing your browser\" is on in its settings - turn it off and restart Firefox to have the line cleared."
    }
}

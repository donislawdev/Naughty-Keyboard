//! Listening for a global shortcut while the tool's own window is not active.
//!
//! # The one invariant this module exists to make unbreakable
//!
//! architektura.md 6.5 measured it and this module encodes it: on Windows a
//! `WM_HOTKEY` is posted to the message queue of the thread that REGISTERED the
//! hotkey, and that queue is drained only by the thread that owns it. Register
//! on one thread and pump on another and the message is delivered to nobody -
//! measured 0 of 3 against 3 of 3. The main thread belongs to Slint (ADR-4), so
//! the shortcut lives on a thread of its own.
//!
//! So the public surface never lets a caller register and pump separately:
//! [`listen`] spawns ONE thread that does both, the same way `nkb_core::keys`
//! makes "select all" unsayable rather than merely discouraged. A caller cannot
//! reconstruct the broken arrangement, because the pieces are never handed out
//! apart.
//!
//! # Why a registered hotkey is not a hot path
//!
//! Also from 6.5, and worth stating because the opposite is a common belief:
//! `RegisterHotKey` is NOT a low-level keyboard hook. The message sits in our
//! queue and we drain it at our own pace. Nobody's keyboard is blocked while we
//! are slow. The hot-path family (`WH_KEYBOARD_LL`) is a different mechanism the
//! tool deliberately does not use - it would see every keystroke in every
//! application, which a tool promising "reads nothing" may not do.
//!
//! # What was measured before this was written
//!
//! A probe on 2026-09-15 (Windows 11, windows-sys 0.61.2, the crate this ships
//! with) covered the parts the 2026-09-07 probe left untouched because it ended
//! with `process::exit`:
//!
//! - posting `WM_QUIT` to a thread that has not yet created its message queue
//!   fails with `ERROR_INVALID_THREAD_ID` (1444). The queue is created as a
//!   side effect of `RegisterHotKey`, so the thread reports "ready" only AFTER
//!   registering, and the stop signal cannot be lost into that gap.
//! - a gated `WM_QUIT` makes `GetMessageW` return 0, the pump leaves its loop,
//!   the thread unregisters ON ITS OWN THREAD, and it joins within the deadline.
//! - the same combination re-registers from a fresh thread afterwards, proving
//!   the release actually freed it.
//! - a second registration of a held combination returns
//!   `ERROR_HOTKEY_ALREADY_REGISTERED` (1409), which is the one failure that
//!   means "taken" and must not be folded with any other.

use std::sync::mpsc::Receiver;

/// The tag a caller gives a hotkey, echoed back when that hotkey fires.
///
/// It is also the `RegisterHotKey` id, so it must fit the application range
/// `0..=0xBFFF`. Ids at `0xC000` and above are reserved for the atom-based
/// registration that DLLs use. The tool has a handful of shortcuts, so this is
/// room to spare rather than a constraint anyone will meet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HotkeyId(pub u32);

/// One global shortcut: its modifiers, its key, and the tag to report on fire.
///
/// The key is a raw virtual-key code rather than an enum, because this crate
/// depends on nothing of ours and knows nothing of the catalogue of shortcuts.
/// The layer that maps a chosen shortcut ("next value") to a virtual key is the
/// adapter, exactly as `chord_for` maps the clearing keys - one place, named.
///
/// Auto-repeat is suppressed for every hotkey and is not optional: `ux-spec.md`
/// 3 requires that holding "next value" not insert a dozen values, and the
/// suppression belongs to the registration rather than to a caller who might
/// forget it.
///
/// Two things this thin layer does NOT police, because the policy lives one
/// layer out with the catalogue of shortcuts:
///
/// - ids within one [`listen`] call must be distinct. Two hotkeys sharing an id
///   would be indistinguishable when they fire, and the OS refuses the second
///   registration of a duplicate id on one thread anyway.
/// - a hotkey with no modifier (`ctrl`/`alt`/`shift`/`win` all false) registers
///   a BARE key globally - it would take that key from every application. The
///   `ux-spec.md` 3 defaults always carry a modifier, and the app layer enforces
///   that. This layer registers exactly what it is handed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hotkey {
    pub id: HotkeyId,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
    /// A platform virtual-key code. On Windows this is a `VK_*` value.
    pub vk: u16,
}

/// What became of registering one hotkey. Three answers, not two.
///
/// `ux-spec.md` 3 is explicit that registration ends three ways, and the third -
/// "registered, but the event never arrives because something higher intercepts
/// it" - is NOT here, because it cannot be known at registration time. It is
/// discovered live, by the shortcut-test field, and folding it in here would be
/// a guess dressed as a fact. This enum carries only what the register call
/// itself can answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyRegistration {
    /// The system accepted it. Whether the event actually arrives is a separate
    /// question this value does not answer.
    Registered,
    /// The combination is already held, here or by another application
    /// (`ERROR_HOTKEY_ALREADY_REGISTERED`). The one failure that means "taken".
    Taken,
    /// Registration failed for some other reason, carrying the raw system code
    /// rather than guessing at a meaning. Not every code can be enumerated, so
    /// the truthful thing is to report the number, not to invent a category.
    Failed { code: u32 },
}

/// Why no listener was started. Named, never a shrug - untouchable rule 1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HotkeyUnavailable {
    /// This build has no route to a global shortcut, naming the system.
    Unsupported { system: &'static str },
    /// A route exists, but the listener thread could not be started - the OS
    /// refused a new thread. Separate from `Unsupported` because it is not about
    /// the platform and can pass on a retry. It exists at all because
    /// `std::thread::spawn` would otherwise panic, which product code may not do.
    CouldNotStart,
}

impl core::fmt::Display for HotkeyUnavailable {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Unsupported { system } => write!(
                f,
                "listening for a global shortcut is not implemented on {system} yet"
            ),
            Self::CouldNotStart => f.write_str("the shortcut listener thread could not be started"),
        }
    }
}

/// A live listener plus what came of setting it up.
///
/// `outcomes` runs in the same order as the hotkeys handed to [`listen`], one
/// per hotkey, so a caller can tell which shortcut was taken without matching by
/// anything but position. `fired` yields the [`HotkeyId`] of each shortcut as it
/// is pressed. `listener` owns the thread. Dropping it, or calling
/// [`HotkeyListener::stop`], ends the thread and releases every shortcut.
pub struct Listening {
    pub outcomes: Vec<HotkeyRegistration>,
    pub fired: Receiver<HotkeyId>,
    pub listener: HotkeyListener,
}

/// Owns the thread that holds the shortcuts. Stops it on drop.
///
/// The thread is stopped either way - explicitly through [`HotkeyListener::stop`]
/// or by this handle going out of scope - so a caller that forgets to stop it
/// still does not leak the thread or leave a shortcut registered after the tool
/// no longer wants it.
pub struct HotkeyListener {
    inner: platform::Listener,
}

impl HotkeyListener {
    /// Ends the listener thread and releases every shortcut, now.
    ///
    /// Blocks until the thread has left its message loop and unregistered, which
    /// is prompt: the stop signal is a posted `WM_QUIT` and the thread's only
    /// wait is on the message queue. Dropping the handle does the same thing.
    /// This exists for a caller that wants the release to have happened by a
    /// known point rather than at end of scope.
    pub fn stop(self) {
        self.inner.stop();
    }
}

/// Whether this build can register a global shortcut at all.
///
/// Asked before the work, so a system with no route says so rather than
/// spawning a thread that can do nothing. One fact in one place, mirroring
/// [`crate::can_send`].
#[must_use]
pub const fn can_register() -> bool {
    cfg!(windows)
}

/// Starts listening for the given shortcuts on a dedicated thread.
///
/// Returns the per-hotkey [`HotkeyRegistration`] outcomes, a receiver of fired
/// [`HotkeyId`]s, and the handle that keeps the thread alive. Registration
/// happens on the listener thread and this call blocks only until that has
/// finished, which is immediate.
///
/// # Errors
///
/// Returns [`HotkeyUnavailable::Unsupported`] on a system with no route, naming
/// it, or [`HotkeyUnavailable::CouldNotStart`] when the listener thread cannot
/// be spawned. An empty slice is not an error: it registers nothing and yields a
/// listener that never fires, which is a legitimate thing to ask for.
pub fn listen(hotkeys: &[Hotkey]) -> Result<Listening, HotkeyUnavailable> {
    let (outcomes, fired, inner) = platform::listen(hotkeys)?;
    Ok(Listening {
        outcomes,
        fired,
        listener: HotkeyListener { inner },
    })
}

#[cfg(windows)]
mod platform {
    use super::{Hotkey, HotkeyId, HotkeyRegistration, HotkeyUnavailable};
    use std::sync::mpsc::{Receiver, Sender};
    use std::thread::JoinHandle;
    use windows_sys::Win32::Foundation::{ERROR_HOTKEY_ALREADY_REGISTERED, GetLastError};
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, RegisterHotKey,
        UnregisterHotKey,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetMessageW, MSG, PostThreadMessageW, WM_HOTKEY, WM_QUIT,
    };

    /// The modifier bits for one hotkey, with no-repeat always on.
    fn modifiers(hotkey: &Hotkey) -> HOT_KEY_MODIFIERS {
        let mut bits = MOD_NOREPEAT;
        if hotkey.ctrl {
            bits |= MOD_CONTROL;
        }
        if hotkey.alt {
            bits |= MOD_ALT;
        }
        if hotkey.shift {
            bits |= MOD_SHIFT;
        }
        if hotkey.win {
            bits |= MOD_WIN;
        }
        bits
    }

    fn register_one(hotkey: &Hotkey) -> HotkeyRegistration {
        // `None` window: the WM_HOTKEY is posted to this thread's queue. The id
        // is our tag. The cast is safe within the documented 0..=0xBFFF range.
        let accepted = unsafe {
            RegisterHotKey(
                core::ptr::null_mut(),
                hotkey.id.0 as i32,
                modifiers(hotkey),
                u32::from(hotkey.vk),
            )
        };
        if accepted != 0 {
            return HotkeyRegistration::Registered;
        }
        let code = unsafe { GetLastError() };
        if code == ERROR_HOTKEY_ALREADY_REGISTERED {
            HotkeyRegistration::Taken
        } else {
            HotkeyRegistration::Failed { code }
        }
    }

    /// Drains this thread's queue until `WM_QUIT`, reporting each hotkey.
    fn pump(fired: &Sender<HotkeyId>) {
        loop {
            let mut message = MSG::default();
            // Blocking, so an idle listener costs nothing. Returns 0 on WM_QUIT
            // and -1 on error, which cannot happen with a null window handle.
            let result = unsafe { GetMessageW(&mut message, core::ptr::null_mut(), 0, 0) };
            if result <= 0 {
                break;
            }
            if message.message == WM_HOTKEY {
                // wParam is the id we registered with. A send that fails means
                // the receiver was dropped - but the Listener that owns the
                // registration may still be alive, so the shortcuts stay held
                // and the press is simply discarded. Only WM_QUIT, posted when
                // the Listener drops, ends this loop. Ending it here would
                // release the shortcuts out from under a live Listener AND leave
                // this thread's id free to be reused before teardown posts to it.
                let _ = fired.send(HotkeyId(message.wParam as u32));
            }
        }
    }

    pub struct Listener {
        thread_id: u32,
        handle: Option<JoinHandle<()>>,
    }

    impl Listener {
        pub fn stop(mut self) {
            self.shut_down();
        }

        fn shut_down(&mut self) {
            let Some(handle) = self.handle.take() else {
                return;
            };
            // Post WM_QUIT ONLY while the thread is still running. A thread that
            // already exited (it registered nothing, so it never entered the
            // pump) has released its id, and the OS may have handed that id to an
            // unrelated thread - posting WM_QUIT to it could quit someone else's
            // message loop, the main thread's included. While the thread is
            // pumping, its id is its own, and WM_QUIT makes GetMessageW return 0
            // so it unregisters on its own thread and exits. Measured 2026-09-15.
            if !handle.is_finished() {
                unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0) };
            }
            let _ = handle.join();
        }
    }

    impl Drop for Listener {
        fn drop(&mut self) {
            self.shut_down();
        }
    }

    #[allow(clippy::type_complexity)]
    pub fn listen(
        hotkeys: &[Hotkey],
    ) -> Result<(Vec<HotkeyRegistration>, Receiver<HotkeyId>, Listener), HotkeyUnavailable> {
        let owned: Vec<Hotkey> = hotkeys.to_vec();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<(u32, Vec<HotkeyRegistration>)>();
        let (fired_tx, fired_rx) = std::sync::mpsc::channel::<HotkeyId>();

        // Builder rather than `thread::spawn`: the latter panics if the OS
        // refuses a thread, and product code may not panic. A refusal here is
        // reported, not crashed on. The name shows up in a debugger and a crash
        // dump years from now, which is the whole reason to spend a Builder on it.
        let handle = std::thread::Builder::new()
            .name("nkb-hotkey".to_owned())
            .spawn(move || {
                // Registering here, on the pumping thread, is the whole point: it
                // is what keeps the WM_HOTKEY in a queue this thread drains, and
                // it creates that queue so the WM_QUIT gate below is safe.
                let thread_id = unsafe { GetCurrentThreadId() };
                let outcomes: Vec<HotkeyRegistration> = owned.iter().map(register_one).collect();
                let any_registered = outcomes
                    .iter()
                    .any(|outcome| matches!(outcome, HotkeyRegistration::Registered));

                // Report the id and outcomes BEFORE pumping. Only now, with the
                // queue created by RegisterHotKey, may the parent post WM_QUIT.
                if ready_tx.send((thread_id, outcomes)).is_err() {
                    // The parent went away between spawn and here. Release and go.
                    release(&owned);
                    return;
                }

                if any_registered {
                    pump(&fired_tx);
                }
                // Whether we pumped or not, every shortcut is released on this
                // thread - UnregisterHotKey undoes only what this thread registered.
                release(&owned);
            })
            .map_err(|_| HotkeyUnavailable::CouldNotStart)?;

        // The thread reports before it pumps, so this blocks only until
        // registration, which is immediate. A recv error means the thread
        // vanished before registering - reported, not dressed as a system name.
        let (thread_id, outcomes) = ready_rx
            .recv()
            .map_err(|_| HotkeyUnavailable::CouldNotStart)?;

        Ok((
            outcomes,
            fired_rx,
            Listener {
                thread_id,
                handle: Some(handle),
            },
        ))
    }

    fn release(hotkeys: &[Hotkey]) {
        for hotkey in hotkeys {
            unsafe { UnregisterHotKey(core::ptr::null_mut(), hotkey.id.0 as i32) };
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::{Hotkey, HotkeyId, HotkeyRegistration, HotkeyUnavailable};
    use std::sync::mpsc::Receiver;

    /// Named rather than "this platform", so the message says something the
    /// reader can act on - the same wording as the rest of this crate.
    const SYSTEM: &str = if cfg!(target_os = "macos") {
        "macOS"
    } else if cfg!(target_os = "linux") {
        "Linux"
    } else {
        "this system"
    };

    /// A handle that owns nothing, because there is no thread on this system.
    pub struct Listener;

    impl Listener {
        pub fn stop(self) {}
    }

    #[allow(clippy::type_complexity)]
    pub fn listen(
        _hotkeys: &[Hotkey],
    ) -> Result<(Vec<HotkeyRegistration>, Receiver<HotkeyId>, Listener), HotkeyUnavailable> {
        // macOS registers a hotkey without a permission, but delivering a value
        // is blocked without accessibility (OBS-70), so a listener with no way
        // to act on what it hears would be a promise the tool cannot keep. Linux
        // depends on the session protocol. Both say so by name rather than
        // returning a listener that never fires.
        Err(HotkeyUnavailable::Unsupported { system: SYSTEM })
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

    /// A combination unlikely to be taken by anything on the machine, so the
    /// deterministic tests do not depend on what else is running. `vk` differs
    /// per test so two tests never collide over one combination.
    fn obscure(id: u32, vk: u16) -> Hotkey {
        Hotkey {
            id: HotkeyId(id),
            ctrl: true,
            alt: true,
            shift: true,
            win: false,
            vk,
        }
    }

    #[test]
    fn an_unsupported_build_names_the_system_rather_than_shrugging() {
        // Untouchable rule 1: on a build with no route the caller learns which
        // system it stands on, and no thread is spawned to do nothing.
        if can_register() {
            return;
        }
        let error = listen(&[obscure(0xB001, 0x70)])
            .err()
            .expect("a build that cannot register must not report success");
        let text = error.to_string();
        assert!(
            text.contains("macOS") || text.contains("Linux") || text.contains("this system"),
            "the message must name the system, got: {text}"
        );
    }

    #[test]
    fn an_empty_set_registers_nothing_and_still_stops_cleanly() {
        // A legitimate request: register no shortcuts. It must not error, and
        // the listener must still stop without hanging.
        let Ok(listening) = listen(&[]) else {
            // Only acceptable outcome on a system with no route.
            assert!(!can_register());
            return;
        };
        assert!(
            listening.outcomes.is_empty(),
            "no hotkeys asked for, none reported"
        );
        listening.listener.stop();
    }

    #[test]
    fn a_shortcut_registers_and_the_listener_stops() {
        if !can_register() {
            return;
        }
        let listening = listen(&[obscure(0xB010, 0x74)]).expect("a route exists on this build");
        assert_eq!(
            listening.outcomes.as_slice(),
            &[HotkeyRegistration::Registered],
            "an obscure combination should be free to register"
        );
        // The teardown proof: stop returns, meaning the thread left its loop,
        // unregistered on its own thread, and joined.
        listening.listener.stop();
    }

    #[test]
    fn releasing_a_shortcut_lets_it_register_again() {
        if !can_register() {
            return;
        }
        // Register, stop (which unregisters), then register the SAME combination
        // once more. If the release did not happen, the second attempt would be
        // Taken. Measured behaviour turned into a standing guard.
        let first = listen(&[obscure(0xB020, 0x75)]).expect("route exists");
        assert_eq!(first.outcomes.as_slice(), &[HotkeyRegistration::Registered]);
        first.listener.stop();

        let second = listen(&[obscure(0xB021, 0x75)]).expect("route exists");
        assert_eq!(
            second.outcomes.as_slice(),
            &[HotkeyRegistration::Registered],
            "after release the same combination must be free again"
        );
        second.listener.stop();
    }

    #[test]
    fn a_second_listener_on_a_held_combination_reports_taken() {
        if !can_register() {
            return;
        }
        // The first listener holds the combination for the length of this test.
        let holder = listen(&[obscure(0xB030, 0x76)]).expect("route exists");
        assert_eq!(
            holder.outcomes.as_slice(),
            &[HotkeyRegistration::Registered]
        );

        // A second listener asking for the same combination must be told it is
        // taken, distinctly from any other failure.
        let contender = listen(&[obscure(0xB031, 0x76)]).expect("route exists");
        assert_eq!(
            contender.outcomes.as_slice(),
            &[HotkeyRegistration::Taken],
            "a held combination is Taken, not Failed and not Registered"
        );

        contender.listener.stop();
        holder.listener.stop();
    }

    #[test]
    fn outcomes_line_up_one_per_hotkey_in_order() {
        if !can_register() {
            return;
        }
        // A held combination in the middle of a batch must show up at its own
        // position, so a caller can tell WHICH shortcut was taken by index.
        let held = listen(&[obscure(0xB040, 0x77)]).expect("route exists");
        assert_eq!(held.outcomes.as_slice(), &[HotkeyRegistration::Registered]);

        let batch = listen(&[
            obscure(0xB041, 0x78),
            obscure(0xB042, 0x77), // same vk as `held` -> Taken
            obscure(0xB043, 0x79),
        ])
        .expect("route exists");
        assert_eq!(
            batch.outcomes.as_slice(),
            &[
                HotkeyRegistration::Registered,
                HotkeyRegistration::Taken,
                HotkeyRegistration::Registered,
            ],
            "one outcome per hotkey, in the order asked"
        );

        batch.listener.stop();
        held.listener.stop();
    }
}

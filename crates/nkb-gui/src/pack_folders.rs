//! The pack folders window on the main thread (`D173` point 5, `D175`): where the
//! team's packs and the tester's own come from, the team folder chosen in the
//! system's folder window or dropped on this one, and both folders opened in the
//! file manager.
//!
//! # It stays open when the tester goes elsewhere
//!
//! Unlike the value window and About, which close when another window is
//! clicked (`OBS-147`), this one stays: dropping a folder on it starts with a
//! click in the file manager, and a window that closed at that click could never
//! be dropped on. It stands on top, ready for the drop, and remembers that the
//! tester went elsewhere - so closing it then hands the keyboard to nobody, and
//! it goes back to the field under test only while it was still here.
//!
//! # What is decided where
//!
//! The words are `nkb-adapters::i18n`'s, what a folder holds is the catalogue's,
//! and a chosen folder is looked at, kept and saved by the worker, which owns the
//! settings (`W4`: one writer). This module moves the answers onto the window
//! and asks the system for its windows - through [`FolderSystem`], so a test
//! needs neither.
//!
//! # One folder window at a time
//!
//! While the system's folder window stands, the buttons that would open a second
//! one are off, and a drop is not taken: the tester is choosing there.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use nkb_adapters::i18n::{self, Environment, FoldersLabel};
use nkb_adapters::system_folders::{PickAnswer, PickError, PickRequest, Picked, ShowAnswer};
use nkb_adapters::{FolderHolds, LentFocus, PackFolders};
use nkb_app::CatalogueSource;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::focus::{HANDLE_WAIT, POLL, window_handle};
use crate::live::{Command, TeamFolderNews};
use crate::on_request::{self, Later, Ready, Requested};
use crate::packs::{Keyboard, SystemKeyboard, build_before_the_loop, forget_held_modifiers};
use crate::shortcut_list::{Key, key_of};
use crate::{FolderAction, FolderPlace, FoldersWindow};

/// The pack folders of this run, on the main thread: the value window reads
/// them at every opening, and this window changes the team folder in them.
pub type SharedFolders = Rc<RefCell<PackFolders>>;

/// The keyboard the value window lent, handed to this window when its link
/// opens it - see [`Folders::open`].
pub type HandedOver = Rc<Cell<Option<LentFocus>>>;

/// What the value window and this one share: the folders, and the cell the
/// value window hands its lent keyboard over in.
#[derive(Clone)]
pub struct FolderLink {
    pub folders: SharedFolders,
    pub handed_over: HandedOver,
}

impl FolderLink {
    /// A link around `folders`, with nothing handed over yet.
    #[must_use]
    pub fn new(folders: PackFolders) -> Self {
        Self {
            folders: Rc::new(RefCell::new(folders)),
            handed_over: Rc::new(Cell::new(None)),
        }
    }
}

/// How long after one dropped thing another counts as part of the same drop.
/// A drop of several things arrives as one event per thing, all at once.
const ONE_DROP: Duration = Duration::from_millis(500);

/// The system's folder window and file manager - a trait so what the window
/// does with their answers is tested without either.
pub trait FolderSystem {
    /// Shows the system's folder window, answering through `answer`.
    fn pick(&self, request: PickRequest, answer: PickAnswer);
    /// Opens `folder` in the file manager, answering through `answer`.
    fn show(&self, folder: &Path, answer: ShowAnswer);
}

/// The system's own.
#[derive(Debug, Default)]
pub struct SystemFolders;

impl FolderSystem for SystemFolders {
    fn pick(&self, request: PickRequest, answer: PickAnswer) {
        nkb_adapters::system_folders::pick_folder(request, answer);
    }
    fn show(&self, folder: &Path, answer: ShowAnswer) {
        nkb_adapters::system_folders::show_folder(folder, answer);
    }
}

/// The buttons of the team folder, in the order they stand.
const CHOOSE: usize = 0;
const OPEN_TEAM: usize = 1;
const STOP_USING: usize = 2;

thread_local! {
    /// The window of THIS thread - the main one - for the answers of the worker
    /// and of the system to find. They arrive from other threads, and the
    /// window cannot travel there: it lives in an `Rc`, like every Slint handle.
    static WINDOW: RefCell<Weak<Folders>> = const { RefCell::new(Weak::new()) };
}

/// Hands one answer of the worker to the window, on the main thread.
pub(crate) fn hear(news: TeamFolderNews) {
    if let Some(folders) = WINDOW.with(|slot| slot.borrow().upgrade()) {
        folders.heard(news);
    }
}

/// Runs `then` with the window on the main thread, from any thread. Dropped
/// when the event loop is gone, for the reason `live::show` gives.
fn on_the_window(then: impl FnOnce(&Rc<Folders>) + Send + 'static) {
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(folders) = WINDOW.with(|slot| slot.borrow().upgrade()) {
            then(&folders);
        }
    });
}

/// What this window needs from the rest of the palette.
pub struct Wiring {
    pub commands: Sender<Command>,
    pub folders: SharedFolders,
    pub handed_over: HandedOver,
    /// The settings file, named where the desktop has no folder window.
    pub settings_file: Option<PathBuf>,
}

/// The pack folders window and what it shows while it is open.
pub struct Folders {
    window: FoldersWindow,
    open: Cell<bool>,
    /// The tester went to another window while this one stood open.
    away: Cell<bool>,
    /// The system's folder window stands.
    picking: Cell<bool>,
    /// A folder is dragged over the window.
    dragging: Cell<bool>,
    /// When the first thing of the last drop arrived.
    last_drop: Cell<Option<Instant>>,
    keyboard: Box<dyn Keyboard>,
    system: Box<dyn FolderSystem>,
    wiring: Wiring,
    notes: RefCell<Vec<String>>,
    /// Puts a line in the palette's message band.
    say: Box<dyn Fn(String)>,
}

impl Folders {
    /// The window, labelled and wired, not yet shown - and registered as the
    /// one the worker's and the system's answers find.
    #[must_use]
    pub fn new(
        window: FoldersWindow,
        keyboard: Box<dyn Keyboard>,
        system: Box<dyn FolderSystem>,
        wiring: Wiring,
        say: Box<dyn Fn(String)>,
    ) -> Rc<Self> {
        label(&window);
        build_before_the_loop(window.window());
        let folders = Rc::new(Self {
            window,
            open: Cell::new(false),
            away: Cell::new(false),
            picking: Cell::new(false),
            dragging: Cell::new(false),
            last_drop: Cell::new(None),
            keyboard,
            system,
            wiring,
            notes: RefCell::new(Vec::new()),
            say,
        });
        WINDOW.with(|slot| *slot.borrow_mut() = Rc::downgrade(&folders));
        wire(&folders);
        folders
    }

    /// The window as the palette has it: made at its first opening, with the
    /// system's keyboard and folders, and kept from then on.
    #[must_use]
    pub fn later(wiring: Wiring, say: Box<dyn Fn(String)>) -> Rc<Later<Self>> {
        let wiring = RefCell::new(Some(wiring));
        Later::new(say, move |say| {
            let Some(wiring) = wiring.borrow_mut().take() else {
                return Err(slint::PlatformError::Other(String::from(
                    "the pack folders window was made twice",
                )));
            };
            Ok(Self::new(
                FoldersWindow::new()?,
                Box::new(SystemKeyboard::default()),
                Box::new(SystemFolders),
                wiring,
                say,
            ))
        })
    }

    /// Whether the window is open.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.open.get()
    }

    /// The window, for a test that reads what it shows.
    #[must_use]
    pub fn window(&self) -> &FoldersWindow {
        &self.window
    }

    /// Opens the window with the keyboard in it, or brings it back. The value
    /// window's link hands over the keyboard it lent, so the field under test
    /// gets it back when this one closes. Opened otherwise, the window
    /// remembers who holds the keyboard now.
    pub fn open(self: &Rc<Self>) {
        if !self.is_open() {
            match self.wiring.handed_over.take() {
                Some(lent) => self.keyboard.take_over(lent),
                None => self.keyboard.remember(),
            }
            self.notes.borrow_mut().clear();
            self.away.set(false);
        }
        self.show();
        if let Err(error) = on_request::bring_up(self.window.window()) {
            self.open.set(false);
            (self.say)(i18n::startup_failure(
                i18n::Startup::WindowFailed,
                &error.to_string(),
            ));
            return;
        }
        self.open.set(true);
        self.window.invoke_focus_keys();
        self.take_keyboard(Instant::now());
    }

    /// Hands the keyboard back - only when it is still here - and hides the
    /// window, in that order, for the reason `Packs::close` gives.
    pub fn close(&self) {
        if self.open.replace(false)
            && !self.away.get()
            && let Err(reason) = self.keyboard.give_back(window_handle(self.window.window()))
        {
            (self.say)(i18n::environment(Environment::FocusNotReturned, &reason));
        }
        let _ = self.window.hide();
    }

    fn take_keyboard(self: &Rc<Self>, started: Instant) {
        let handle = window_handle(self.window.window());
        if handle.is_none() && started.elapsed() < HANDLE_WAIT {
            let weak = Rc::downgrade(self);
            slint::Timer::single_shot(POLL, move || {
                if let Some(folders) = weak.upgrade()
                    && folders.is_open()
                {
                    folders.take_keyboard(started);
                }
            });
            return;
        }
        if let Err(reason) = self.keyboard.take(handle) {
            (self.say)(i18n::environment(
                Environment::FoldersFocusNotTaken,
                &reason,
            ));
        }
    }

    /// Draws both folders as a fresh look at the disk finds them, and the
    /// status line.
    fn show(&self) {
        let folders = self.wiring.folders.borrow().clone();
        let catalogue = folders.catalogue();
        let busy = self.picking.get();
        self.window.set_team(team_place(
            &folders,
            catalogue.holds_of(CatalogueSource::Team),
            busy,
            self.dragging.get(),
        ));
        self.window.set_own(own_place(
            &folders,
            catalogue.holds_of(CatalogueSource::Own),
        ));
        let notes: Vec<SharedString> = self.notes.borrow().iter().map(SharedString::from).collect();
        self.window.set_notes(ModelRc::new(VecModel::from(notes)));
    }

    /// One line in the status line, in place of the last - each says what the
    /// last thing the tester did came to.
    fn tell(&self, line: String) {
        *self.notes.borrow_mut() = vec![line];
        self.show();
    }

    fn team_clicked(self: &Rc<Self>, index: i32) {
        match usize::try_from(index) {
            Ok(CHOOSE) => self.choose(),
            Ok(OPEN_TEAM) => {
                let team = self
                    .wiring
                    .folders
                    .borrow()
                    .team()
                    .ok()
                    .map(Path::to_path_buf);
                if let Some(team) = team {
                    self.open_in_file_manager(&team);
                }
            }
            Ok(STOP_USING) => self.send(None),
            _ => {}
        }
    }

    fn own_clicked(&self) {
        let folders = self.wiring.folders.borrow().clone();
        let Ok(own) = folders.own().map(Path::to_path_buf) else {
            return;
        };
        if let Err(error) = folders.make_own() {
            self.tell(i18n::own_folder_not_made(&error, &own));
            return;
        }
        self.open_in_file_manager(&own);
    }

    /// Shows the system's folder window, opening in the team folder when the
    /// last look read it - a share that does not answer would hold the folder
    /// window before it shows.
    fn choose(self: &Rc<Self>) {
        if self.picking.replace(true) {
            return;
        }
        let folders = self.wiring.folders.borrow().clone();
        let readable = matches!(
            folders.catalogue().holds_of(CatalogueSource::Team),
            Some(FolderHolds::Packs { .. })
        );
        let start = readable
            .then(|| folders.team().ok().map(Path::to_path_buf))
            .flatten();
        self.tell(i18n::folders_label(FoldersLabel::Picking).to_owned());
        self.system.pick(
            PickRequest {
                title: i18n::folders_label(FoldersLabel::PickerTitle).to_owned(),
                start,
                owner: window_handle(self.window.window()),
            },
            Box::new(|answer| on_the_window(move |folders| folders.picked(answer))),
        );
    }

    /// The system folder window's answer, on the main thread. Public, like the
    /// three below, so a test reaches it without the system's threads.
    pub fn picked(&self, answer: Result<Picked, PickError>) {
        self.picking.set(false);
        match answer {
            // Drawn again at once: the buttons come back now, not when the
            // worker answers - it may never, when the palette is closing.
            Ok(Picked::Folder(folder)) => {
                self.notes.borrow_mut().clear();
                self.show();
                self.send(Some(folder));
            }
            Ok(Picked::Cancelled) => {
                self.notes.borrow_mut().clear();
                self.show();
            }
            Err(error) => self.tell(i18n::folder_window_failed(
                &error,
                self.wiring.settings_file.as_deref(),
            )),
        }
    }

    /// Asks the worker to make `folder` the team folder, or to stop using one.
    fn send(&self, folder: Option<PathBuf>) {
        if self
            .wiring
            .commands
            .send(Command::TeamFolder(folder))
            .is_err()
        {
            // The worker is gone - the palette is closing - and nobody is left
            // to keep the folder.
            self.show();
        }
    }

    /// The worker's answer: the folders the run uses now, and what to say.
    pub fn heard(&self, news: TeamFolderNews) {
        let now = self.wiring.folders.borrow().with_team(news.team);
        *self.wiring.folders.borrow_mut() = now;
        self.tell(news.said);
    }

    fn open_in_file_manager(&self, folder: &Path) {
        let named = folder.to_path_buf();
        self.system.show(
            folder,
            Box::new(move |answer| {
                if let Err(error) = answer {
                    on_the_window(move |folders| {
                        folders.tell(i18n::file_manager_failed(&error, &named));
                    });
                }
            }),
        );
    }

    /// A folder dragged over the window, or no longer.
    pub fn dragged(&self, over: bool) {
        if self.dragging.replace(over) != over {
            self.show();
        }
    }

    /// One thing dropped on the window. The first of a drop becomes the team
    /// folder - the worker looks at it - and the rest are named, once.
    pub fn dropped(&self, path: PathBuf) {
        self.dragging.set(false);
        if self.picking.get() {
            self.show();
            return;
        }
        let now = Instant::now();
        let same_drop = self
            .last_drop
            .get()
            .is_some_and(|first| now.duration_since(first) < ONE_DROP);
        if same_drop {
            self.tell(i18n::folders_label(FoldersLabel::OnlyFirstDropped).to_owned());
            return;
        }
        self.last_drop.set(Some(now));
        self.send(Some(path));
    }

    /// Esc closes, and every other key travels on.
    fn press(&self, text: &str) -> bool {
        if key_of(text) != Key::Escape {
            return false;
        }
        self.close();
        true
    }
}

/// The callbacks of the window and the drops of its system window.
fn wire(folders: &Rc<Folders>) {
    let weak = Rc::downgrade(folders);
    folders
        .window
        .on_pressed(move |text, _control, _alt, _shift, _meta| {
            weak.upgrade()
                .is_some_and(|folders| folders.press(text.as_str()))
        });
    let weak = Rc::downgrade(folders);
    folders.window.on_close_clicked(move || {
        if let Some(folders) = weak.upgrade() {
            folders.close();
        }
    });
    let weak = Rc::downgrade(folders);
    folders.window.on_team_act(move |index| {
        if let Some(folders) = weak.upgrade() {
            folders.team_clicked(index);
        }
    });
    let weak = Rc::downgrade(folders);
    folders.window.on_own_act(move |_| {
        if let Some(folders) = weak.upgrade() {
            folders.own_clicked();
        }
    });
    let weak = Rc::downgrade(folders);
    folders.window.on_deactivated(move || {
        if let Some(folders) = weak.upgrade() {
            folders.away.set(true);
        }
    });
    let weak = Rc::downgrade(folders);
    folders.window.on_activated(move || {
        if let Some(folders) = weak.upgrade() {
            folders.away.set(false);
            forget_held_modifiers(folders.window.window());
        }
    });
    let weak = Rc::downgrade(folders);
    folders.window.window().on_close_requested(move || {
        if let Some(folders) = weak.upgrade() {
            folders.close();
        }
        // Hidden already, after the keyboard went back - see `close`.
        slint::CloseRequestResponse::KeepWindowShown
    });
    wire_drops(folders);
}

/// The drops of the window, which the window toolkit does not hand to a view:
/// winit's own events, read before the toolkit looks at them. A window the
/// toolkit does not back with winit - a test's - has none.
fn wire_drops(folders: &Rc<Folders>) {
    use slint::winit_030::winit::event::WindowEvent;
    use slint::winit_030::{EventResult, WinitWindowAccessor};
    let weak = Rc::downgrade(folders);
    folders
        .window
        .window()
        .on_winit_window_event(move |_, event| {
            let Some(folders) = weak.upgrade() else {
                return EventResult::Propagate;
            };
            match event {
                WindowEvent::HoveredFile(_) => folders.dragged(true),
                WindowEvent::HoveredFileCancelled => folders.dragged(false),
                WindowEvent::DroppedFile(path) => folders.dropped(path.clone()),
                _ => return EventResult::Propagate,
            }
            EventResult::PreventDefault
        });
}

/// The team folder as the window shows it: where, what is there, and its three
/// buttons - choose (the window's main action), open, stop using.
fn team_place(
    folders: &PackFolders,
    holds: Option<FolderHolds>,
    busy: bool,
    dragging: bool,
) -> FolderPlace {
    let holds = holds.unwrap_or(FolderHolds::Skipped(nkb_app::SourceSkipped::NotSet));
    let path = folders.team().map_or_else(
        |_| i18n::folders_label(FoldersLabel::NoTeamFolder).to_owned(),
        i18n::shown_folder,
    );
    let (state, fault) = if dragging {
        (
            i18n::folders_label(FoldersLabel::DropHere).to_owned(),
            false,
        )
    } else {
        i18n::folder_state(CatalogueSource::Team, holds)
    };
    let opens = matches!(
        holds,
        FolderHolds::Packs { .. }
            | FolderHolds::Skipped(
                nkb_app::SourceSkipped::Unreadable
                    | nkb_app::SourceSkipped::Unreachable
                    | nkb_app::SourceSkipped::TooManyFiles
                    | nkb_app::SourceSkipped::SameAsOwn
            )
    );
    let set = !matches!(holds, FolderHolds::Skipped(nkb_app::SourceSkipped::NotSet));
    FolderPlace {
        heading: i18n::folders_label(FoldersLabel::TeamHeading).into(),
        path: path.into(),
        state: state.into(),
        fault,
        targeted: dragging,
        actions: actions(vec![
            action(
                FoldersLabel::Choose,
                FoldersLabel::ChooseAction,
                !busy,
                true,
            ),
            action(
                FoldersLabel::Open,
                FoldersLabel::OpenTeamAction,
                opens && !busy,
                false,
            ),
            action(
                FoldersLabel::StopUsing,
                FoldersLabel::StopUsingAction,
                set && !busy,
                false,
            ),
        ]),
    }
}

/// The own folder as the window shows it: where, what is there, and one button,
/// which makes the folder first when it is not there yet. None where the
/// computer gives the settings no place.
fn own_place(folders: &PackFolders, holds: Option<FolderHolds>) -> FolderPlace {
    let holds = holds.unwrap_or(FolderHolds::Skipped(nkb_app::SourceSkipped::NoPlace));
    let (state, fault) = i18n::folder_state(CatalogueSource::Own, holds);
    let (path, buttons) = match folders.own() {
        Ok(own) => {
            let missing = holds == FolderHolds::Skipped(nkb_app::SourceSkipped::Missing);
            let word = if missing {
                FoldersLabel::CreateAndOpen
            } else {
                FoldersLabel::Open
            };
            (
                i18n::shown_folder(own),
                vec![action(word, FoldersLabel::OpenOwnAction, true, false)],
            )
        }
        Err(_) => (String::new(), Vec::new()),
    };
    FolderPlace {
        heading: i18n::folders_label(FoldersLabel::OwnHeading).into(),
        path: path.into(),
        state: state.into(),
        fault,
        targeted: false,
        actions: actions(buttons),
    }
}

fn action(text: FoldersLabel, does: FoldersLabel, enabled: bool, primary: bool) -> FolderAction {
    FolderAction {
        text: i18n::folders_label(text).into(),
        action: i18n::folders_label(does).into(),
        enabled,
        primary,
    }
}

fn actions(list: Vec<FolderAction>) -> ModelRc<FolderAction> {
    ModelRc::new(VecModel::from(list))
}

/// Every word that does not change while the program runs.
fn label(window: &FoldersWindow) {
    window.set_window_title(i18n::folders_label(FoldersLabel::Title).into());
    window.set_heading(i18n::folders_label(FoldersLabel::Heading).into());
    window.set_summary(i18n::folders_label(FoldersLabel::Summary).into());
    window.set_drop_hint(i18n::folders_label(FoldersLabel::DropHint).into());
    window.set_close_label(i18n::folders_label(FoldersLabel::Close).into());
    window.set_close_action(i18n::folders_label(FoldersLabel::CloseAction).into());
}

impl Requested for Folders {
    fn open(self: &Rc<Self>) {
        Self::open(self);
    }
    fn close(&self) {
        Self::close(self);
    }
    fn when_ready(self: &Rc<Self>, then: Ready) {
        on_request::when_backed(self, |folders| folders.window.window(), then);
    }
}

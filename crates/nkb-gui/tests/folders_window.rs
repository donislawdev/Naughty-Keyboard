//! The pack folders window as the product wires it (`D175`), with no screen,
//! no system folder window and no file manager: what it shows, what a choice
//! and a drop send to the worker, what an answer changes, and where the
//! keyboard goes on closing.
//!
//! # What it cannot
//!
//! Show the system's folder window or open a file manager - those are measured
//! on each system (`tools/sonda-foldery`, `tools/sondy/explorer-sciezki.ps1`) -
//! nor move a real focus, which is the porcja okien, like every window's.

#![allow(clippy::panic, clippy::expect_used)]

#[allow(dead_code)]
mod offscreen;

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver};

use nkb_adapters::PackFolders;
use nkb_adapters::system_folders::{PickAnswer, PickRequest, Picked, ShowAnswer};
use nkb_app::SourceSkipped;
use nkb_gui::live::{Command, TeamFolderNews};
use nkb_gui::pack_folders::{FolderLink, FolderSystem, Folders, Wiring};
use nkb_gui::packs::Keyboard;
use nkb_gui::{FolderPlace, FoldersWindow};
use slint::Model;

#[derive(Default)]
struct Recorded {
    keyboard: RefCell<Vec<&'static str>>,
    picks: RefCell<Vec<PickRequest>>,
    shown: RefCell<Vec<PathBuf>>,
}

struct Keys(Rc<Recorded>);

impl Keyboard for Keys {
    fn remember(&self) {
        self.0.keyboard.borrow_mut().push("remember");
    }
    fn take(&self, _window: Option<u64>) -> Result<(), String> {
        self.0.keyboard.borrow_mut().push("take");
        Ok(())
    }
    fn give_back(&self, _window: Option<u64>) -> Result<(), String> {
        self.0.keyboard.borrow_mut().push("give back");
        Ok(())
    }
}

struct System(Rc<Recorded>);

impl FolderSystem for System {
    fn pick(&self, request: PickRequest, _answer: PickAnswer) {
        self.0.picks.borrow_mut().push(request);
    }
    fn show(&self, folder: &Path, _answer: ShowAnswer) {
        self.0.shown.borrow_mut().push(folder.to_path_buf());
    }
}

/// A folder of its own in the system's temporary place, removed when dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir()
            .join("nkb-folders-window-tests")
            .join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("a scratch folder");
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct Opened {
    folders: Rc<Folders>,
    recorded: Rc<Recorded>,
    commands: Receiver<Command>,
    link: FolderLink,
}

fn opened(own: &Path) -> Opened {
    let recorded = Rc::new(Recorded::default());
    let (sent, commands) = mpsc::channel();
    let link = FolderLink::new(PackFolders::new(
        Err(SourceSkipped::NotSet),
        Ok(own.to_path_buf()),
    ));
    let folders = Folders::new(
        FoldersWindow::new().expect("the pack folders window must build"),
        Box::new(Keys(Rc::clone(&recorded))),
        Box::new(System(Rc::clone(&recorded))),
        Wiring {
            commands: sent,
            folders: Rc::clone(&link.folders),
            handed_over: Rc::clone(&link.handed_over),
            settings_file: Some(PathBuf::from("settings.toml")),
        },
        Box::new(|line| panic!("the window said {line:?} in the palette")),
    );
    folders.open();
    Opened {
        folders,
        recorded,
        commands,
        link,
    }
}

fn words(place: &FolderPlace) -> Vec<(String, bool, bool)> {
    place
        .actions
        .iter()
        .map(|action| (action.text.to_string(), action.enabled, action.primary))
        .collect()
}

fn notes(window: &FoldersWindow) -> Vec<String> {
    window
        .get_notes()
        .iter()
        .map(|note| note.to_string())
        .collect()
}

#[test]
fn a_run_without_a_team_folder_offers_to_choose_one_and_to_create_the_own_folder() {
    let _surface = offscreen::start(480, 900);
    let scratch = Scratch::new("first");
    let own = scratch.0.join("own");
    let run = opened(&own);
    let window = run.folders.window();

    assert_eq!(window.get_window_title(), "Naughty Keyboard - pack folders");
    assert_eq!(window.get_heading(), "Pack folders");
    let team = window.get_team();
    assert_eq!(team.heading, "Team folder");
    assert_eq!(team.path, "No team folder is set.");
    assert_eq!(
        team.state,
        "Choose the folder your team keeps its packs in."
    );
    assert!(!team.fault);
    assert_eq!(
        words(&team),
        [
            (String::from("Choose folder"), true, true),
            (String::from("Open"), false, false),
            (String::from("Stop using"), false, false),
        ],
        "only choosing makes sense without a team folder, and it is the main action"
    );
    let own_place = window.get_own();
    assert_eq!(
        own_place.state,
        "Not created yet. It is created when you open it."
    );
    assert_eq!(
        words(&own_place),
        [(String::from("Create and open"), true, false)]
    );
    // The keyboard itself is taken once the window has a handle - which a
    // window with no screen never gets, so the poll is About's to test.
    assert_eq!(
        *run.recorded.keyboard.borrow(),
        ["remember"],
        "opened by itself, the window remembers who had the keyboard"
    );

    // ---- the own folder is made on the click, then opened -------------------
    window.invoke_own_act(0);
    assert!(own.is_dir(), "the click did not create the own folder");
    assert_eq!(*run.recorded.shown.borrow(), std::slice::from_ref(&own));
}

#[test]
fn a_chosen_folder_goes_to_the_worker_and_its_answer_is_shown() {
    let _surface = offscreen::start(480, 900);
    let scratch = Scratch::new("choose");
    let run = opened(&scratch.0.join("own"));
    let window = run.folders.window();

    window.invoke_team_act(0);
    let picks = run.recorded.picks.borrow().clone();
    assert_eq!(picks.len(), 1);
    assert_eq!(picks[0].title, "Choose the team folder");
    assert_eq!(picks[0].start, None, "no team folder was read to open in");
    assert_eq!(
        notes(window),
        ["Choose the folder in the window the system opened."]
    );
    assert!(
        !window
            .get_team()
            .actions
            .row_data(0)
            .expect("choose")
            .enabled,
        "a second folder window could be opened over the first"
    );
    window.invoke_team_act(0);
    assert_eq!(
        run.recorded.picks.borrow().len(),
        1,
        "a second folder window opened"
    );

    let team = scratch.0.join("team packs");
    std::fs::create_dir_all(&team).expect("a team folder");
    run.folders.picked(Ok(Picked::Folder(team.clone())));
    match run.commands.try_recv() {
        Ok(Command::TeamFolder(Some(sent))) => assert_eq!(sent, team),
        other => panic!("the worker was asked {other:?}"),
    }
    assert!(
        window
            .get_team()
            .actions
            .row_data(0)
            .expect("choose")
            .enabled
    );

    run.folders.heard(TeamFolderNews {
        team: Some(team.clone()),
        said: String::from("The team folder is saved."),
    });
    assert_eq!(run.link.folders.borrow().team(), Ok(team.as_path()));
    let shown = window.get_team();
    assert_eq!(shown.path, team.display().to_string());
    assert_eq!(shown.state, "packs: 0");
    assert_eq!(
        words(&shown)
            .into_iter()
            .map(|(_, enabled, _)| enabled)
            .collect::<Vec<_>>(),
        [true, true, true],
        "a team folder that reads can be opened and stopped"
    );
    assert_eq!(notes(window), ["The team folder is saved."]);

    window.invoke_team_act(2);
    assert!(matches!(
        run.commands.try_recv(),
        Ok(Command::TeamFolder(None))
    ));

    // A cancel says nothing and sends nothing.
    window.invoke_team_act(0);
    run.folders.picked(Ok(Picked::Cancelled));
    assert!(notes(window).is_empty(), "{:?}", notes(window));
    assert!(run.commands.try_recv().is_err());
}

#[test]
fn the_first_thing_of_a_drop_is_taken_and_the_rest_are_named_once() {
    let _surface = offscreen::start(480, 900);
    let scratch = Scratch::new("drop");
    let run = opened(&scratch.0.join("own"));
    let window = run.folders.window();

    run.folders.dragged(true);
    let team = window.get_team();
    assert!(team.targeted);
    assert_eq!(team.state, "Drop it here to use it as the team folder.");
    run.folders.dropped(scratch.0.join("first"));
    run.folders.dropped(scratch.0.join("second"));
    assert!(matches!(
        run.commands.try_recv(),
        Ok(Command::TeamFolder(Some(path))) if path == scratch.0.join("first")
    ));
    assert!(
        run.commands.try_recv().is_err(),
        "the second thing was taken too"
    );
    assert_eq!(
        notes(window),
        ["More than one thing was dropped. Only the first is used."]
    );
    assert!(!window.get_team().targeted);
}

#[test]
fn the_window_stays_when_the_tester_leaves_and_then_hands_the_keyboard_to_nobody() {
    let _surface = offscreen::start(480, 900);
    let scratch = Scratch::new("leave");
    let run = opened(&scratch.0.join("own"));
    let window = run.folders.window();
    run.recorded.keyboard.borrow_mut().clear();

    window.invoke_deactivated();
    assert!(
        run.folders.is_open(),
        "the window closed when the tester left"
    );
    run.folders.close();
    assert!(
        run.recorded.keyboard.borrow().is_empty(),
        "the keyboard was taken from where the tester put it: {:?}",
        run.recorded.keyboard.borrow()
    );

    run.folders.open();
    run.recorded.keyboard.borrow_mut().clear();
    run.folders.close();
    assert_eq!(*run.recorded.keyboard.borrow(), ["give back"]);
}

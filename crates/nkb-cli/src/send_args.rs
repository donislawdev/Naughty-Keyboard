//! What `nkb send` was asked to do, read from its arguments: a pack or a
//! limit, which of its values, how long to wait and whether to clear the field
//! first.
//!
//! A wrong call is said here, on standard error, and comes back as the code to
//! end with, so the rest of the command starts from a call that is right. The
//! parts of a limit are read by the reader `nkb boundary` reads them with
//! (`limit_args`).

use std::io::Write;

use nkb_adapters::boundary_text::command;
use nkb_app::Clearing;
use nkb_core::boundary::{Limit, values};

use crate::exit::ExitCode;
use crate::help::print_send_help;
use crate::limit_args::{LimitOption, LimitWishes, Refusal, see_help};

/// The name of this command, for the sentences that send a person to its help.
pub(crate) const COMMAND: &str = "send";

/// Which switch is still waiting for the word after it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SendSwitch {
    Index,
    Delay,
    /// A part of a limit, read by the reader `nkb boundary` reads it with.
    Limit(LimitOption),
}

impl SendSwitch {
    /// The switch as the person typed it, for the sentence that says what is
    /// missing after it.
    const fn name(self) -> &'static str {
        match self {
            Self::Index => "--index",
            Self::Delay => "--delay",
            Self::Limit(option) => option.option(),
        }
    }
}

/// Where the value comes from, as the arguments named it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Source<'a> {
    /// A pack, by the name the person typed.
    Pack(&'a str),
    /// The values made for a limit (`D178`).
    Limit(Limit),
}

/// What `nkb send` was asked to do, read from its arguments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SendArgs<'a> {
    pub(crate) wanted: Source<'a>,
    pub(crate) position: u64,
    pub(crate) delay_seconds: u64,
    pub(crate) clearing: Clearing,
}

/// What the arguments said so far, before they are settled into [`SendArgs`].
struct Words<'a> {
    pack: Option<&'a str>,
    position: Option<u64>,
    delay_seconds: u64,
    clearing: Clearing,
    limit: LimitWishes,
    expecting: Option<SendSwitch>,
}

/// Says a wrong call on standard error, and ends with the code for one.
pub(crate) fn say_refused(lines: &Refusal) -> ExitCode {
    let mut err = crate::output::err();
    let _ = writeln!(err, "nkb {COMMAND}: {}", lines[0]);
    let _ = writeln!(err, "{}", lines[1]);
    ExitCode::Usage
}

/// The arguments of `nkb send`, or the exit code to stop with.
///
/// `Err` means the command is over and what there was to say has been said: a
/// usage error on standard error, or the help on standard output - which is why
/// `--help` comes back as `Err(ExitCode::Ok)`.
pub(crate) fn send_args(args: &[String]) -> Result<SendArgs<'_>, ExitCode> {
    let mut words = Words {
        pack: None,
        position: None,
        delay_seconds: 3,
        clearing: Clearing::Keep,
        limit: LimitWishes::default(),
        expecting: None,
    };
    for arg in args {
        words.read(arg)?;
    }
    words.settled()
}

impl<'a> Words<'a> {
    /// Reads one word: a switch, the word a switch waits for, or the pack.
    fn read(&mut self, arg: &'a str) -> Result<(), ExitCode> {
        if let Some(which) = self.expecting.take() {
            return self.after(which, arg);
        }
        if let Some(option) = LimitOption::of(arg) {
            self.expecting = Some(SendSwitch::Limit(option));
            return Ok(());
        }
        match arg {
            "-h" | "--help" => {
                print_send_help();
                return Err(ExitCode::Ok);
            }
            "--index" => self.expecting = Some(SendSwitch::Index),
            "--delay" => self.expecting = Some(SendSwitch::Delay),
            "--clear" => self.clearing = Clearing::Line,
            other if other.starts_with('-') => {
                let mut err = crate::output::err();
                let _ = writeln!(err, "nkb send: unknown switch '{other}'");
                let _ = writeln!(err, "Run 'nkb send --help' to see what is available.");
                return Err(ExitCode::Usage);
            }
            name => {
                if self.pack.is_some() {
                    let mut err = crate::output::err();
                    let _ = writeln!(err, "nkb send: name one pack, not two.");
                    return Err(ExitCode::Usage);
                }
                self.pack = Some(name);
            }
        }
        Ok(())
    }

    /// Takes the word a switch was waiting for.
    fn after(&mut self, which: SendSwitch, arg: &str) -> Result<(), ExitCode> {
        if let SendSwitch::Limit(option) = which {
            return self
                .limit
                .take(option, Some(arg), COMMAND)
                .map_err(|lines| say_refused(&lines));
        }
        let name = which.name();
        let Ok(number) = arg.parse::<u64>() else {
            let mut err = crate::output::err();
            let _ = writeln!(err, "nkb send: {name} needs a whole number, got '{arg}'.");
            return Err(ExitCode::Usage);
        };
        if which == SendSwitch::Index {
            self.position = Some(number);
        } else {
            self.delay_seconds = number;
        }
        Ok(())
    }

    /// What the words came to, once every one was read.
    fn settled(mut self) -> Result<SendArgs<'a>, ExitCode> {
        if let Some(which) = self.expecting {
            if let SendSwitch::Limit(option) = which {
                // The reader's own sentence for an option with nothing after it.
                return Err(self
                    .limit
                    .take(option, None, COMMAND)
                    .map_or_else(|lines| say_refused(&lines), |()| ExitCode::Usage));
            }
            let name = which.name();
            let mut err = crate::output::err();
            let _ = writeln!(err, "nkb send: {name} needs a number.");
            return Err(ExitCode::Usage);
        }
        let wanted = self.source()?;
        let position = match (wanted, self.position) {
            (_, Some(position)) => position,
            (Source::Pack(_), None) => 1,
            (Source::Limit(limit), None) => return Err(say_index_needed(limit)),
        };
        Ok(SendArgs {
            wanted,
            position,
            delay_seconds: self.delay_seconds,
            clearing: self.clearing,
        })
    }

    /// The pack named, or the limit given - one of the two, never both.
    fn source(&self) -> Result<Source<'a>, ExitCode> {
        match (self.pack, self.limit.any()) {
            (Some(_), true) => Err(say_refused(&[
                "name a pack or give a limit, not both.".to_owned(),
                see_help(COMMAND),
            ])),
            (Some(name), false) => Ok(Source::Pack(name)),
            (None, true) => self
                .limit
                .settled(COMMAND)
                .map(Source::Limit)
                .map_err(|lines| say_refused(&lines)),
            (None, false) => {
                let mut err = crate::output::err();
                let _ = writeln!(
                    err,
                    "nkb send: name a pack, or give a limit with --max-length or --min-length."
                );
                let _ = writeln!(err, "Run 'nkb packs' to see what there is.");
                Err(ExitCode::Usage)
            }
        }
    }
}

/// A limit with no `--index`: how many values there are, and the command that
/// lists them.
fn say_index_needed(limit: Limit) -> ExitCode {
    say_refused(&[
        format!(
            "--index is needed with a limit, which gives {} values.",
            values(limit).len()
        ),
        format!(
            "`{}` lists them in that order, counting from 1.",
            command(limit)
        ),
    ])
}

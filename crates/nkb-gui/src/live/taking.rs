//! Taking the shortcuts, and naming them once the desktop or the system has answered:
//! the methods of the worker that register the table in effect, say what came of it, and
//! put the words that name the shortcuts on the palette. Kept apart from the loop so that
//! `live.rs` stays under the ceiling of `D113`.

use super::*;

impl Worker<'_> {
    /// Takes the shortcuts the settings give now, and the palette's view of
    /// it - with the words naming them, which may have changed.
    ///
    /// What the file wishes and cannot have is said again: it is still true,
    /// and the tester may just have looked at it in the window.
    pub(super) fn resume(&mut self) -> View {
        self.resume_after(Vec::new())
    }

    /// [`Worker::resume`], with `lines` said before what it has to say.
    pub(super) fn resume_after(&mut self, mut lines: Vec<String>) -> View {
        let (bindings, refused) = self.in_effect();
        self.memory.bindings.set(bindings);
        lines.extend(
            refused.iter().filter_map(|message| {
                i18n::settings_message(message, &self.memory.file, &bindings)
            }),
        );
        lines.extend(self.take(bindings));
        let legend = self.naming(bindings);
        self.view(lines, ValueBand::Keep, Some(legend))
    }

    /// The words that name the shortcuts, with what the desktop gave where it
    /// chose the chords. The table they follow is the one every sentence of the
    /// palette reads from now on, and it is never saved or registered.
    pub(super) fn naming(&mut self, asked: Bindings) -> Legend {
        let (shown, apart) = self.hold.as_given(asked);
        self.memory.bindings.set(shown);
        Legend {
            links: self.hold.links(),
            ..legend_of(&shown, &apart)
        }
    }

    /// Registers `bindings` in place of whatever is held, and what is worth
    /// saying about it - with the route asked for at start, the first time
    /// the shortcuts are held.
    pub(super) fn take(&mut self, bindings: Bindings) -> Vec<String> {
        let lines = self.hold.take(&bindings);
        self.with_the_route(lines, bindings)
    }

    /// What the palette draws when the desktop has answered the shortcuts it was
    /// asked for: the legend of the table, as after a resume.
    pub(super) fn answered(&mut self, lines: Vec<String>) -> View {
        let asked = self.in_effect().0;
        let legend = self.naming(asked);
        let lines = self.with_the_route(lines, self.memory.bindings.get());
        self.view(lines, ValueBand::Keep, Some(legend))
    }

    /// `lines`, and what the route asked for at start adds once the shortcuts are
    /// held.
    fn with_the_route(&mut self, mut lines: Vec<String>, bindings: Bindings) -> Vec<String> {
        if self.hold.live().is_some()
            && let Some(route) = self.route.take()
        {
            // A system with no direct route starts in clipboard mode here,
            // rather than failing the first press.
            lines.extend(
                self.sequence
                    .choose_route(route, self.ports)
                    .iter()
                    .map(|message| i18n::message(message, &self.pack, &bindings)),
            );
        }
        lines
    }
}

//! The words for the text services of the system that may have changed a typed
//! value (`D139`), out of `i18n.rs` under the ceiling of `D113`.

use nkb_core::substitutions::Substitutions;

/// The text services that may have changed a value, ready to stand inside a
/// sentence: "a period added after two spaces and straight quotes made curly".
/// Nothing set gives an empty text, which no sentence asks for.
#[must_use]
pub fn substitution_names(services: Substitutions) -> String {
    let mut names: Vec<&str> = Vec::with_capacity(3);
    if services.capitalization {
        names.push("the first letter of a sentence made a capital");
    }
    if services.period {
        names.push("a period added after two spaces");
    }
    if services.quotes {
        names.push("straight quotes made curly");
    }
    match names.as_slice() {
        [] => String::new(),
        [only] => (*only).to_owned(),
        [head @ .., last] => format!("{} and {last}", head.join(", ")),
    }
}

/// The sentence for the services that may have changed a value: `pattern` with
/// its `{what}` filled.
pub(super) fn sentence(pattern: &str, services: Substitutions) -> String {
    super::fill(pattern, &[("what", &substitution_names(services))])
}

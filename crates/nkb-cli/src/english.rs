//! English inflection for the command line, which is English for good.
//!
//! # Why the command line may inflect and the palette may not
//!
//! Rule 8 of the project keeps every word `nkb` prints in English, forever:
//! only the graphical interface is translated. So the English rule for the
//! plural is the whole truth here, and "1 code points" next to the shortest
//! values in the catalogue - the ones this tool exists for - is simply a
//! mistake.
//!
//! The palette is translated, and a translated sentence cannot carry the
//! English rule: Polish has three plural forms and Arabic six. It writes every
//! count after a label instead (`graphemes: 1`), so no language needs a rule
//! at all (`D80`). This file is the reason the two surfaces read differently,
//! and it is on purpose.

/// A count with its noun, in the number the count actually is.
///
/// Document 08 asks for interface text a person would write. `noun` is the
/// singular, and every noun this tool counts makes its plural with an `s`.
pub fn plural(count: usize, noun: &str) -> String {
    if count == 1 {
        format!("{count} {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

/// Words joined the way a sentence lists them: `a`, `a and b`, `a, b and c`.
///
/// For a closed list the format keeps in code, so a sentence that names the
/// allowed words names the ones the check uses, and the two cannot drift.
pub fn listed<S: AsRef<str>>(words: &[S]) -> String {
    match words {
        [] => String::new(),
        [one] => one.as_ref().to_owned(),
        [rest @ .., last] => {
            let head: Vec<&str> = rest.iter().map(AsRef::as_ref).collect();
            format!("{} and {}", head.join(", "), last.as_ref())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_list_is_joined_with_commas_and_a_last_and() {
        assert_eq!(listed::<&str>(&[]), "");
        assert_eq!(listed(&["normal"]), "normal");
        assert_eq!(listed(&["normal", "offensive"]), "normal and offensive");
        assert_eq!(
            listed(&["look-alike", "range", "identity"]),
            "look-alike, range and identity"
        );
    }

    #[test]
    fn a_count_of_one_is_written_in_the_singular() {
        // "1 code points" appears beside the shortest values in the catalogue,
        // which are exactly the ones this tool exists for.
        assert_eq!(plural(1, "code point"), "1 code point");
        assert_eq!(plural(0, "byte"), "0 bytes");
        assert_eq!(plural(2, "byte"), "2 bytes");
        assert_eq!(plural(1, "UTF-16 unit"), "1 UTF-16 unit");
    }
}

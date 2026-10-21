//! Putting a value into the `{name}` places of a sentence.

/// Substitutes `{name}` placeholders in one pass over the pattern.
///
/// One pass, not a chain of `replace` calls, and the difference is not style: a
/// chain re-scans what it has just substituted, so a value that happens to
/// contain braces would be substituted again. Here a substituted value is
/// copied into the output and never looked at.
///
/// An unknown placeholder is left standing rather than swallowed. That is
/// deliberate: a `{sent}` visible in the palette is ugly and gets fixed, while a
/// silently emptied sentence reads as finished. The test below refuses any
/// pattern that still holds a brace after its own arguments are applied, so the
/// ugly version never reaches a tester.
pub(crate) fn fill(pattern: &str, values: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(pattern.len());
    let mut rest = pattern;

    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            // No closing brace at all: the rest is literal text.
            out.push_str(&rest[open..]);
            return out;
        };
        let name = &after[..close];
        match values.iter().find(|(key, _)| *key == name) {
            Some((_, value)) => out.push_str(value),
            None => {
                out.push('{');
                out.push_str(name);
                out.push('}');
            }
        }
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_does_not_rescan_what_it_substituted() {
        // A value carrying braces must be copied out, not treated as a new
        // placeholder. Today no argument can contain one - identifiers come from
        // a narrow alphabet - but the property is what makes that safe to stop
        // thinking about.
        let out = fill("a {x} b", &[("x", "{y}"), ("y", "NO")]);
        assert_eq!(out, "a {y} b");
    }

    #[test]
    fn fill_leaves_an_unknown_placeholder_visible() {
        assert_eq!(fill("a {x} b", &[]), "a {x} b");
        assert_eq!(fill("a { b", &[]), "a { b");
        assert_eq!(fill("{x}", &[("x", "")]), "");
    }
}

//! The counterstring: text that says how long it is, at every star.
//!
//! Every star is preceded by the number of its own position, counted from
//! one. Twelve characters read `*3*5*7*9*12*`. A field that kept only part of
//! it ends in a number and perhaps a few characters more, and that is where it
//! cut - no counting by eye, which is what makes it the value to reach for when
//! a limit is only suspected.
//!
//! A module of its own because two things build it: a value held by the
//! palette ([`crate::value::ValueBody::Counterstring`]) and the answer to a
//! limit ([`crate::boundary`]). One builder, so the two cannot disagree.

/// The counterstring of `length` characters.
///
/// Built from the end, because the last star is the one whose place is known.
/// Only the very first position can be too narrow for a number, and it takes a
/// bare star. Digits and stars alone, so its length is the same in graphemes,
/// code points, UTF-16 units and bytes.
#[must_use]
pub fn counterstring(length: usize) -> String {
    const DIGITS: &[u8; 10] = b"0123456789";
    let mut backwards = Vec::with_capacity(length);
    let mut left = length;
    while left > 0 {
        backwards.push(b'*');
        let mut number = left;
        left -= 1;
        while number > 0 && left > 0 {
            backwards.push(DIGITS[number % 10]);
            number /= 10;
            left -= 1;
        }
    }
    backwards.into_iter().rev().map(char::from).collect()
}

/// The first and the last `kept` characters of the counterstring of `length`,
/// without the counterstring: what a preview of a long one shows.
///
/// The end is the first thing the builder writes. The start is reached by
/// arithmetic alone: the text before any star is itself a counterstring, of
/// the length that star's number leaves, so the walk goes from star to star
/// down to the first one near enough to the start, and only that short text
/// is built. `None` when the two ends would meet, where the caller shows all
/// of it.
#[must_use]
pub fn ends(length: usize, kept: usize) -> Option<(String, String)> {
    if length <= kept.saturating_mul(2) {
        return None;
    }
    // From a star at `left`, the one before it stands at `left` less the star
    // and the digits of its number.
    let before = |left: usize| left.checked_sub(1 + digits(left));
    let mut left = length;
    let mut tail = Vec::with_capacity(kept + MOST_DIGITS + 1);
    while tail.len() < kept {
        tail.push(b'*');
        let mut position = left;
        while position > 0 {
            tail.push(b'0' + u8::try_from(position % 10).unwrap_or(0));
            position /= 10;
        }
        left = before(left)?;
    }
    tail.truncate(kept);
    // The start is walked to from the end again: the end may have stopped
    // nearer the start than `kept`.
    let mut left = length;
    while let Some(next) = before(left).filter(|&next| next >= kept) {
        left = next;
    }
    let head = counterstring(left).chars().take(kept).collect();
    Some((head, tail.into_iter().rev().map(char::from).collect()))
}

/// The digits of the largest number a `usize` holds, with room to spare.
const MOST_DIGITS: usize = 20;

/// How many decimal digits `number` is written with.
fn digits(number: usize) -> usize {
    number.checked_ilog10().map_or(1, |log| log as usize + 1)
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;

    #[test]
    fn the_ends_are_the_ends_of_the_built_text_at_every_length() {
        // Every length across the places a number grows a digit, where the walk
        // from star to star changes its step.
        let lengths = (0..=1300)
            .chain(9_980..=10_030)
            .chain([99_999, 100_000, 100_001]);
        for length in lengths {
            for kept in [1, 7, 50] {
                let built = counterstring(length);
                let Some((head, tail)) = ends(length, kept) else {
                    assert!(length <= kept * 2, "{length} kept {kept}");
                    continue;
                };
                assert_eq!(head, built[..kept], "the start of {length}");
                assert_eq!(tail, built[length - kept..], "the end of {length}");
            }
        }
    }

    #[test]
    fn the_ends_of_the_longest_value_are_found_without_building_it() {
        // A million characters: the walk is arithmetic, and what is built is a
        // few dozen characters. The run time is the assertion.
        let (head, tail) = ends(1_000_000, 50).expect("far longer than its ends");
        assert_eq!(head.len(), 50);
        assert!(tail.ends_with("*1000000*"), "{tail}");
    }

    #[test]
    fn a_counterstring_reads_as_the_known_examples() {
        assert_eq!(counterstring(0), "");
        assert_eq!(counterstring(1), "*");
        assert_eq!(counterstring(2), "2*");
        assert_eq!(counterstring(3), "*3*");
        assert_eq!(counterstring(11), "2*4*6*8*11*");
        assert_eq!(counterstring(12), "*3*5*7*9*12*");
    }

    #[test]
    fn every_star_of_a_counterstring_stands_where_the_number_before_it_says() {
        for length in (1..=1200).chain([9_999, 10_000, 100_001]) {
            let text = counterstring(length);
            assert_eq!(text.len(), length, "ASCII, so bytes are characters");
            assert!(text.ends_with('*'));
            let mut number = String::new();
            for (index, character) in text.chars().enumerate() {
                if character.is_ascii_digit() {
                    number.push(character);
                    continue;
                }
                assert_eq!(character, '*');
                if !number.is_empty() {
                    assert_eq!(number, (index + 1).to_string(), "in {length}");
                    number.clear();
                } else {
                    assert_eq!(index, 0, "only the first star may stand bare");
                }
            }
        }
    }

    #[test]
    fn a_cut_counterstring_says_where_it_was_cut() {
        // 51 characters cut to 50: the last whole star says 48, two more follow.
        let kept = &counterstring(51)[..50];
        assert!(kept.ends_with("*48*51"), "{kept}");
    }
}

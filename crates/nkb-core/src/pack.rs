//! A pack as the rest of the tool needs it, rather than as the validator sees it.
//!
//! # Why this type did not exist until now
//!
//! Everything written so far reads a pack file in order to find fault with it.
//! `PackFormat` says so in as many words: it does not return a pack, because a
//! validator has to see a file as it actually is, including the parts a lenient
//! reader would forgive. That was the right shape for `nkb lint`, and it is the
//! wrong shape for `nkb packs`, `nkb show` and `nkb emit`, which need the thing
//! the file describes rather than a list of complaints about it.
//!
//! So this is the second direction of the same format, and the two are kept
//! apart deliberately: nothing here reports a problem, and nothing in `lint`
//! builds one of these.
//!
//! # All of it or none of it
//!
//! `pack-format.md` 11 settles what happens to a pack with one bad value out of
//! thirty-four: **nothing loads**. A pack is correct in full or it is absent,
//! and the tool says which file and how many problems.
//!
//! The reasoning is the product's own reasoning turned on itself. Loading the
//! good thirty-three would put `7 / 33` in the palette for a file holding
//! thirty-four values, and the sequence would differ from what the file plainly
//! says - a silent disagreement between the data and what the user sees, which
//! is the exact class of fault this tool exists to find in other people's
//! software.
//!
//! Warnings are the one exception: a pack carrying them loads normally and the
//! warnings show up in its information. They do not change what is inside.
//!
//! # A value here is still a recipe
//!
//! [`ValueBody`] is carried unexpanded, because `architektura.md` 6.1 requires
//! the size of a generated value to be known BEFORE the text exists - otherwise
//! the promised warning about a length bomb arrives after the bomb has gone off
//! inside the tool. Turning a recipe into text is a separate and explicit step,
//! and it is not in this file.

use crate::value::ValueBody;

/// How dangerous a value is to put into somebody else's application.
///
/// Two levels, closed, and the vocabulary is the format's own. It decides
/// whether the palette marks a value before a tester sends it, so an unknown
/// word here would not degrade to a smaller feature - it would degrade to no
/// warning at all, which is why `E017` refuses one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Risk {
    /// Safe to send anywhere.
    #[default]
    Normal,
    /// Offensive in content. Marked before sending, never sent by surprise.
    Offensive,
}

impl Risk {
    /// The word the format writes. A public contract: it travels into machine
    /// readable output, so it is never respelled.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Offensive => "offensive",
        }
    }

    /// Reads the format's word, or nothing when it is not one of the two.
    #[must_use]
    pub fn parse(word: &str) -> Option<Self> {
        match word {
            "normal" => Some(Self::Normal),
            "offensive" => Some(Self::Offensive),
            _ => None,
        }
    }
}

/// One value out of a pack.
///
/// `breaks` and `expect` are optional here and required by the format. That is
/// not a contradiction: a pack reaching this type has passed validation, so they
/// are present in practice - but a type that cannot represent their absence
/// forces whoever builds one to invent a sentence, and an invented sentence in a
/// field a tester reads is worse than a visible gap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackValue {
    pub id: String,
    pub name: String,
    /// The recipe. Never expanded by anything in this module.
    pub body: ValueBody,
    /// What breaks when an application mishandles this. Prose, English.
    pub breaks: Option<String>,
    /// What correct handling looks like. Prose, English.
    pub expect: Option<String>,
    /// Declared on the value itself. `None` means "whatever the pack says",
    /// which is a different fact from "normal" and is kept apart on purpose:
    /// `W063` asks precisely whether a value declares its own.
    pub risk: Option<Risk>,
    /// Field kinds this value suits, when it narrows the pack's own list.
    pub fields: Vec<String>,
    pub tags: Vec<String>,
    /// Retired values stay in the file so their identifiers are never reused.
    pub deprecated: bool,
    /// The value that replaces a retired one, named within this same pack.
    pub replaced_by: Option<String>,
}

/// Two values that belong together, and the reason they do.
///
/// A pair carries no value of its own - it names two that already exist - which
/// is why it has fewer fields than a value rather than optional ones.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackPair {
    pub id: String,
    pub name: String,
    /// One of the three the format defines: look-alike, range, identity.
    pub relation: String,
    /// Identifier of the first value, within this pack.
    pub a: String,
    /// Identifier of the second value, within this pack.
    pub b: String,
    pub breaks: Option<String>,
    pub expect: Option<String>,
}

/// A whole pack file, loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pack {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub updated: String,
    pub license: String,
    pub authors: Vec<String>,
    /// Language of the PROSE in this file, never of the data inside it. The two
    /// are separate on purpose - untouchable rule 8 - and `locale-pl` is the
    /// case that proves it: Polish data, English prose.
    pub language: String,
    /// The pack's own risk level, and the default for every value in it.
    pub risk: Risk,
    pub tags: Vec<String>,
    /// Field kinds this pack suits, and the default for every value in it.
    pub fields: Vec<String>,
    pub values: Vec<PackValue>,
    pub pairs: Vec<PackPair>,
}

impl Pack {
    /// The risk that actually applies to a value: its own if it declares one,
    /// otherwise the pack's.
    ///
    /// A method rather than a field computed at parse time, so that "declared
    /// nothing" stays visible in the data. Flattening the two at the door would
    /// make `W063` unaskable one layer up.
    #[must_use]
    pub fn risk_of(&self, value: &PackValue) -> Risk {
        value.risk.unwrap_or(self.risk)
    }

    /// The field kinds that actually apply to a value.
    ///
    /// Same inheritance as risk, and the same reason for keeping the empty case
    /// distinguishable: a value listing nothing is taking the pack's list, not
    /// declaring that it suits no field at all.
    #[must_use]
    pub fn fields_of<'a>(&'a self, value: &'a PackValue) -> &'a [String] {
        if value.fields.is_empty() {
            &self.fields
        } else {
            &value.fields
        }
    }

    /// Whether anything in this pack is offensive, however it was declared.
    ///
    /// Asked by the palette before it shows a pack, and by `W063`, which is
    /// about a pack staying silent while carrying one.
    #[must_use]
    pub fn carries_offensive(&self) -> bool {
        self.risk == Risk::Offensive
            || self
                .values
                .iter()
                .any(|value| self.risk_of(value) == Risk::Offensive)
    }

    /// The value with this identifier.
    #[must_use]
    pub fn value(&self, id: &str) -> Option<&PackValue> {
        self.values.iter().find(|value| value.id == id)
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "a failed expectation in a test is a failed test"
)]
mod tests {
    use super::*;
    use crate::text::LiteralText;

    fn value(id: &str, risk: Option<Risk>) -> PackValue {
        PackValue {
            id: id.to_owned(),
            name: format!("Value {id}"),
            body: ValueBody::Literal(LiteralText::new("x")),
            breaks: None,
            expect: None,
            risk,
            fields: Vec::new(),
            tags: Vec::new(),
            deprecated: false,
            replaced_by: None,
        }
    }

    fn pack(risk: Risk, values: Vec<PackValue>) -> Pack {
        Pack {
            id: "sample".to_owned(),
            name: "Sample".to_owned(),
            description: "A pack for the tests in this module.".to_owned(),
            version: "1.0".to_owned(),
            updated: "2026-09-08".to_owned(),
            license: "CC-BY-4.0".to_owned(),
            authors: vec!["Naughty Keyboard".to_owned()],
            language: "en".to_owned(),
            risk,
            tags: Vec::new(),
            fields: vec!["any".to_owned()],
            values,
            pairs: Vec::new(),
        }
    }

    #[test]
    fn a_value_without_its_own_risk_takes_the_packs() {
        let subject = pack(Risk::Offensive, vec![value("a", None)]);
        let first = subject.values.first().expect("one value");
        assert_eq!(subject.risk_of(first), Risk::Offensive);
    }

    #[test]
    fn a_value_with_its_own_risk_keeps_it() {
        let subject = pack(Risk::Offensive, vec![value("a", Some(Risk::Normal))]);
        let first = subject.values.first().expect("one value");
        assert_eq!(subject.risk_of(first), Risk::Normal);
    }

    #[test]
    fn declaring_nothing_stays_distinguishable_from_declaring_normal() {
        // The whole point of keeping `risk` an Option: W063 asks whether the
        // value said anything, and a parse that flattened this could not answer.
        let silent = value("a", None);
        let explicit = value("b", Some(Risk::Normal));
        assert!(silent.risk.is_none());
        assert_eq!(explicit.risk, Some(Risk::Normal));
    }

    #[test]
    fn a_normal_pack_holding_one_offensive_value_still_carries_offensive() {
        // The shape W063 is about: the header says nothing and the content does.
        let subject = pack(
            Risk::Normal,
            vec![value("a", None), value("b", Some(Risk::Offensive))],
        );
        assert!(subject.carries_offensive());
    }

    #[test]
    fn a_pack_of_ordinary_values_carries_nothing_offensive() {
        let subject = pack(Risk::Normal, vec![value("a", None), value("b", None)]);
        assert!(!subject.carries_offensive());
    }

    #[test]
    fn a_value_narrows_the_packs_fields_but_an_empty_list_inherits_them() {
        let mut narrowing = value("a", None);
        narrowing.fields = vec!["email".to_owned()];
        let inheriting = value("b", None);
        let subject = pack(Risk::Normal, vec![narrowing, inheriting]);

        let first = subject.values.first().expect("two values");
        let second = subject.values.get(1).expect("two values");
        assert_eq!(subject.fields_of(first), ["email"]);
        assert_eq!(subject.fields_of(second), ["any"]);
    }

    #[test]
    fn risk_words_survive_a_round_trip_and_nothing_else_is_accepted() {
        // These words go into machine readable output, so they are a contract.
        assert_eq!(Risk::parse("normal"), Some(Risk::Normal));
        assert_eq!(Risk::parse("offensive"), Some(Risk::Offensive));
        assert_eq!(Risk::parse("Offensive"), None);
        assert_eq!(Risk::parse("rude"), None);
        assert_eq!(Risk::Offensive.as_str(), "offensive");
    }

    #[test]
    fn a_value_is_found_by_its_identifier_and_missing_ones_report_nothing() {
        let subject = pack(Risk::Normal, vec![value("trailing-space", None)]);
        assert!(subject.value("trailing-space").is_some());
        assert!(subject.value("no-such-value").is_none());
    }
}

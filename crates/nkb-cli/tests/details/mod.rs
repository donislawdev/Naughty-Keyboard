//! What a built program says about itself on Windows: the version block in its
//! resources, read back from the file the test run built - the half of the
//! file's details the build script cannot check, because it only writes them.
//!
//! Shared by `file_details.rs` in `nkb-cli` and the file of the same name in
//! `nkb-gui`, for the reason `imports/mod.rs` gives: a package's own
//! integration tests are the only place Cargo hands over the path of the
//! binary it has just built (`CARGO_BIN_EXE_*`).
//!
//! # What is held, and against what
//!
//! The version, the author, the licence and the address against the package
//! manifest, through the variables Cargo gives the test - so the file follows
//! a release that raises the version, and a block that stopped following it
//! is red. The product's name, the year and each program's description
//! against the words written HERE, apart from the build script that writes
//! the block: a change to them is a change somebody makes twice, on purpose.
//!
//! The reading is done by hand rather than by asking Windows, because the
//! structure is small and documented (`VS_VERSIONINFO`, `VS_FIXEDFILEINFO`)
//! and asking the system would need a library this test does not otherwise
//! link. Measured against the system on 2026-10-10: `FileVersionInfo` read the
//! same nine strings from both programs.

use object::read::pe::{PeFile64, ResourceDirectoryEntryData, ResourceNameOrId};
use object::{LittleEndian as LE, pe};
use std::collections::BTreeMap;

/// The product both programs belong to - pinned here, apart from the build
/// script that writes it.
pub const PRODUCT: &str = "Naughty Keyboard";
/// The copyright's year, pinned the same way.
pub const FIRST_YEAR: &str = "2026";

/// The resource type of a version block, `RT_VERSION`.
const RT_VERSION: u16 = 16;
/// What a `VS_FIXEDFILEINFO` starts with.
const FIXED_SIGNATURE: u32 = 0xFEEF_04BD;

/// One program, as its version block should describe it.
pub struct Program {
    /// The file's name without `.exe`.
    pub name: &'static str,
    /// What the task manager calls it.
    pub description: &'static str,
}

/// The version block of one built file.
#[derive(Debug, Default)]
pub struct Details {
    /// `FILEVERSION`, the four numbers.
    pub file_version: [u16; 4],
    /// `PRODUCTVERSION`, the four numbers.
    pub product_version: [u16; 4],
    /// `FILEFLAGS`, masked by `FILEFLAGSMASK`.
    pub flags: u32,
    /// Every string of the string table, by its key.
    pub strings: BTreeMap<String, String>,
}

impl Details {
    /// The string under `key`, or a failure naming it.
    pub fn string(&self, key: &str) -> &str {
        self.strings
            .get(key)
            .unwrap_or_else(|| panic!("the version block has no {key}: {:#?}", self.strings))
    }
}

/// Reads the version block of the file at `path`.
pub fn read(path: &str) -> Details {
    let data = std::fs::read(path).unwrap_or_else(|error| panic!("cannot read {path}: {error}"));
    let block = version_block(&data).unwrap_or_else(|| panic!("{path} has no version block"));
    let root =
        Node::parse(block).unwrap_or_else(|| panic!("{path}: the version block is malformed"));
    assert_eq!(
        root.key, "VS_VERSION_INFO",
        "{path}: the block's root has another name"
    );
    let mut details = fixed(root.value).unwrap_or_else(|| panic!("{path}: no fixed file info"));
    for table in root
        .children
        .iter()
        .filter(|child| child.key == "StringFileInfo")
        .flat_map(|info| &info.children)
    {
        assert_eq!(
            table.key, "040904B0",
            "{path}: a string table in another language"
        );
        for string in &table.children {
            details
                .strings
                .insert(string.key.clone(), text(string.value));
        }
    }
    details
}

/// Holds `details` to the manifest and to the words pinned in this module.
pub fn assert_describes(details: &Details, program: &Program) {
    let version = env!("CARGO_PKG_VERSION");
    let author = env!("CARGO_PKG_AUTHORS").replace(':', ", ");
    let numbers = [
        number(env!("CARGO_PKG_VERSION_MAJOR")),
        number(env!("CARGO_PKG_VERSION_MINOR")),
        number(env!("CARGO_PKG_VERSION_PATCH")),
        0,
    ];
    let expected = [
        ("CompanyName", author.clone()),
        ("FileDescription", program.description.to_owned()),
        ("FileVersion", version.to_owned()),
        ("InternalName", program.name.to_owned()),
        (
            "LegalCopyright",
            format!(
                "Copyright \u{a9} {FIRST_YEAR} {author}. {}",
                env!("CARGO_PKG_LICENSE")
            ),
        ),
        ("OriginalFilename", format!("{}.exe", program.name)),
        ("ProductName", PRODUCT.to_owned()),
        ("ProductVersion", version.to_owned()),
        ("Comments", env!("CARGO_PKG_HOMEPAGE").to_owned()),
    ];
    for (key, value) in &expected {
        assert_eq!(details.string(key), value, "{}: {key}", program.name);
    }
    assert_eq!(
        details.strings.len(),
        expected.len(),
        "{}: strings nobody asked for: {:#?}",
        program.name,
        details.strings
    );
    assert_eq!(
        details.file_version, numbers,
        "{}: FILEVERSION",
        program.name
    );
    assert_eq!(
        details.product_version, numbers,
        "{}: PRODUCTVERSION",
        program.name
    );
    let prerelease = !env!("CARGO_PKG_VERSION_PRE").is_empty();
    let flags = u32::from(cfg!(debug_assertions)) | (u32::from(prerelease) << 1);
    assert_eq!(details.flags, flags, "{}: FILEFLAGS", program.name);
    for (key, value) in &details.strings {
        assert!(!value.is_empty(), "{}: {key} is empty", program.name);
    }
}

fn number(text: &str) -> u16 {
    text.parse()
        .unwrap_or_else(|_| panic!("a version number over 16 bits: {text}"))
}

/// The bytes of the first `RT_VERSION` resource, through the three levels of
/// the resource directory: type, name, language.
fn version_block(data: &[u8]) -> Option<&[u8]> {
    let file = PeFile64::parse(data).ok()?;
    let sections = file.section_table();
    let directory = file
        .data_directories()
        .resource_directory(data, &sections)
        .ok()??;
    let mut table = directory.root().ok()?;
    let mut want_type = true;
    loop {
        let entry = table.entries.iter().find(|entry| {
            !want_type || matches!(entry.name_or_id(), ResourceNameOrId::Id(RT_VERSION))
        })?;
        want_type = false;
        match entry.data(directory).ok()? {
            ResourceDirectoryEntryData::Table(next) => table = next,
            ResourceDirectoryEntryData::Data(leaf) => {
                return leaf_bytes(data, &sections, leaf);
            }
        }
    }
}

fn leaf_bytes<'a>(
    data: &'a [u8],
    sections: &object::read::pe::SectionTable<'a>,
    leaf: &pe::ImageResourceDataEntry,
) -> Option<&'a [u8]> {
    let size = usize::try_from(leaf.size.get(LE)).ok()?;
    sections
        .pe_data_at(data, leaf.offset_to_data.get(LE))?
        .get(..size)
}

/// One node of a version block: `wLength`, `wValueLength`, `wType`, the key,
/// the value, the children - each part aligned to four bytes.
struct Node<'a> {
    key: String,
    value: &'a [u8],
    children: Vec<Node<'a>>,
}

impl<'a> Node<'a> {
    fn parse(bytes: &'a [u8]) -> Option<Self> {
        let word = |at: usize| -> Option<usize> {
            Some(usize::from(u16::from_le_bytes([
                *bytes.get(at)?,
                *bytes.get(at + 1)?,
            ])))
        };
        let length = word(0)?;
        let bytes = bytes.get(..length)?;
        let value_length = word(2)?;
        // A text value counts its length in UTF-16 units, a binary one in bytes.
        let value_bytes = if word(4)? == 1 {
            value_length * 2
        } else {
            value_length
        };
        let key_end = (6..bytes.len())
            .step_by(2)
            .find(|&at| bytes.get(at) == Some(&0) && bytes.get(at + 1) == Some(&0))?;
        let key = text(bytes.get(6..key_end)?);
        let value_start = align(key_end + 2);
        let value_end = (value_start + value_bytes).min(bytes.len());
        let value = bytes.get(value_start..value_end)?;
        let mut children = Vec::new();
        let mut at = align(value_end);
        while at + 6 <= bytes.len() {
            let child_length = word(at)?;
            if child_length == 0 {
                break;
            }
            children.push(Node::parse(bytes.get(at..)?)?);
            at = align(at + child_length);
        }
        Some(Self {
            key,
            value,
            children,
        })
    }
}

fn align(at: usize) -> usize {
    (at + 3) & !3
}

/// UTF-16 little endian up to the first NUL.
fn text(bytes: &[u8]) -> String {
    let (pairs, _) = bytes.as_chunks::<2>();
    let units: Vec<u16> = pairs
        .iter()
        .map(|pair| u16::from_le_bytes(*pair))
        .take_while(|&unit| unit != 0)
        .collect();
    String::from_utf16(&units).unwrap_or_else(|_| panic!("not UTF-16: {bytes:?}"))
}

/// The numbers of a `VS_FIXEDFILEINFO`.
fn fixed(value: &[u8]) -> Option<Details> {
    let dword = |index: usize| -> Option<u32> {
        let at = index * 4;
        Some(u32::from_le_bytes(value.get(at..at + 4)?.try_into().ok()?))
    };
    if dword(0)? != FIXED_SIGNATURE {
        return None;
    }
    let four = |high: u32, low: u32| -> [u16; 4] {
        let halves = |v: u32| [(v >> 16) as u16, (v & 0xFFFF) as u16];
        let [a, b] = halves(high);
        let [c, d] = halves(low);
        [a, b, c, d]
    };
    Some(Details {
        file_version: four(dword(2)?, dword(3)?),
        product_version: four(dword(4)?, dword(5)?),
        flags: dword(7)? & dword(6)?,
        strings: BTreeMap::new(),
    })
}

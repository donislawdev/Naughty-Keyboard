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
/// The resource types of an icon: each image (`RT_ICON`), named by a number,
/// and the group that lists the images with their sizes (`RT_GROUP_ICON`).
const RT_ICON: u16 = 3;
const RT_GROUP_ICON: u16 = 14;
/// The one icon file both programs carry, kept with the drawings it is made
/// from. The path is the same from either package's folder.
const ICON_FILE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../nkb-gui/assets/edamame.ico");
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

/// Holds the icon of the file at `path` to the icon file that ships, byte for
/// byte: its group lists the images the `.ico` lists, in the same order and
/// with the same sizes, and every image it names is the one the `.ico` holds.
/// Read from the built program, so a resource script that stopped naming the
/// icon, or named another, is red - the chain of scripts is not trusted.
pub fn assert_wears_the_icon(path: &str) {
    let data = std::fs::read(path).unwrap_or_else(|error| panic!("cannot read {path}: {error}"));
    let group =
        resource(&data, RT_GROUP_ICON, None).unwrap_or_else(|| panic!("{path} carries no icon"));
    let ico =
        std::fs::read(ICON_FILE).unwrap_or_else(|error| panic!("cannot read {ICON_FILE}: {error}"));
    let count = usize::from(le16(&ico, 4));
    assert_eq!(
        (le16(group, 0), le16(group, 2)),
        (0, 1),
        "{path}: the group's header is not an icon's"
    );
    assert_eq!(
        usize::from(le16(group, 4)),
        count,
        "{path}: the group lists another number of images than {ICON_FILE}"
    );
    for i in 0..count {
        // An entry of the file is 16 bytes and ends in the image's offset, an
        // entry of the group is 14 and ends in the image's resource number.
        // The first twelve - size, colours, planes, bits, length - are shared.
        let in_file = slice(&ico, 6 + 16 * i, 16);
        let in_group = slice(group, 6 + 14 * i, 14);
        assert_eq!(
            slice(in_group, 0, 12),
            slice(in_file, 0, 12),
            "{path}: image {i} is described otherwise than in {ICON_FILE}"
        );
        let number = le16(in_group, 12);
        let image = resource(&data, RT_ICON, Some(number))
            .unwrap_or_else(|| panic!("{path}: image {i} (resource {number}) is not there"));
        let length = le32(in_file, 8) as usize;
        let offset = le32(in_file, 12) as usize;
        assert!(
            image == slice(&ico, offset, length),
            "{path}: image {i} is not the one {ICON_FILE} holds"
        );
    }
}

fn slice(bytes: &[u8], at: usize, length: usize) -> &[u8] {
    bytes
        .get(at..at + length)
        .unwrap_or_else(|| panic!("{length} bytes at {at} lie outside {} bytes", bytes.len()))
}

fn le16(bytes: &[u8], at: usize) -> u16 {
    let two = slice(bytes, at, 2);
    u16::from_le_bytes([two[0], two[1]])
}

fn le32(bytes: &[u8], at: usize) -> u32 {
    let four = slice(bytes, at, 4);
    u32::from_le_bytes([four[0], four[1], four[2], four[3]])
}

/// The bytes of the first `RT_VERSION` resource.
fn version_block(data: &[u8]) -> Option<&[u8]> {
    resource(data, RT_VERSION, None)
}

/// The bytes of the first resource of type `kind` - or of the one numbered
/// `name` - through the three levels of the resource directory: type, name,
/// language.
fn resource(data: &[u8], kind: u16, name: Option<u16>) -> Option<&[u8]> {
    let file = PeFile64::parse(data).ok()?;
    let sections = file.section_table();
    let directory = file
        .data_directories()
        .resource_directory(data, &sections)
        .ok()??;
    let mut table = directory.root().ok()?;
    let mut level = 0;
    loop {
        let wanted = match level {
            0 => Some(kind),
            1 => name,
            _ => None,
        };
        let entry = table.entries.iter().find(|entry| {
            wanted.is_none_or(
                |id| matches!(entry.name_or_id(), ResourceNameOrId::Id(found) if found == id),
            )
        })?;
        level += 1;
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

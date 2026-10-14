//! The links in the palette's header - GitHub, Donate and About, in that order
//! (the owner's points of 2026-10-14) - and the two pages the first two open
//! in the browser.
//!
//! 🔴 The one place in the product that hands an address to anything. The tool
//! promises it opens no network connection, and it still does not: the
//! browser makes the connection, on a click, to one of the two addresses
//! below. Nothing here takes an address from a pack, a setting, a view or a
//! sentence, and no function here takes an address at all - a caller names a
//! link, and the link has its page. The guard `nothing_reaches_the_network.rs`
//! grants `webbrowser::` to this file alone and holds the addresses in it to
//! [`SOURCE_PAGE`] and [`SUPPORT_PAGE`].

use nkb_adapters::i18n::{self, PaletteLabel};
use slint::{ModelRc, SharedString, VecModel};

use crate::about::shown_address;
use crate::{HeaderLink, HeaderLinkKind, Palette};

/// The page the GitHub link opens: the project's repository, the address the
/// workspace manifest gives as `repository` (a test holds the two together).
pub const SOURCE_PAGE: &str = "https://github.com/donislawdev/Naughty-Keyboard";

/// The page the Donate link opens.
pub const SUPPORT_PAGE: &str = "https://donislawdev.com/support/";

/// The links of the header, in the order they stand: the two that open a page
/// in the browser together, then About, which opens a window of the program,
/// beside the counter.
#[must_use]
pub fn links() -> Vec<HeaderLink> {
    [
        (HeaderLinkKind::SourceCode, PaletteLabel::SourceCodeLink),
        (HeaderLinkKind::Donate, PaletteLabel::DonateLink),
        (HeaderLinkKind::About, PaletteLabel::AboutLink),
    ]
    .into_iter()
    .map(|(kind, label)| HeaderLink {
        kind,
        text: i18n::label(label).into(),
    })
    .collect()
}

/// Hands the links to the palette.
pub fn show(palette: &Palette) {
    palette.set_header_links(ModelRc::new(VecModel::from(links())));
}

/// The page a link opens in the browser, or none for a link that opens a
/// window of the program.
fn page_of(link: HeaderLinkKind) -> Option<&'static str> {
    match link {
        HeaderLinkKind::SourceCode => Some(SOURCE_PAGE),
        HeaderLinkKind::Donate => Some(SUPPORT_PAGE),
        HeaderLinkKind::About => None,
    }
}

/// A click on a link that opens a page, on the MAIN thread: the page opens,
/// or the palette's status line says it did not and names the page, so the
/// tester can type it - a click with no effect is never a silence
/// (untouchable rule 1). A link that opens no page does nothing here: the
/// About window is the caller's.
pub fn open_page(palette: &Palette, link: HeaderLinkKind) {
    let Some(page) = page_of(link) else {
        return;
    };
    if webbrowser::open(page).is_err() {
        palette.set_messages(ModelRc::new(VecModel::from(vec![SharedString::from(
            i18n::browser_did_not_open(&shown_address(page)),
        )])));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_github_link_opens_the_repository_the_manifest_names() {
        assert_eq!(SOURCE_PAGE, env!("CARGO_PKG_REPOSITORY"));
    }

    #[test]
    fn each_link_opens_its_own_page_and_about_opens_none() {
        assert_eq!(page_of(HeaderLinkKind::SourceCode), Some(SOURCE_PAGE));
        assert_eq!(page_of(HeaderLinkKind::Donate), Some(SUPPORT_PAGE));
        assert_eq!(page_of(HeaderLinkKind::About), None);
    }

    /// The order the owner chose: the two pages in the browser together, About
    /// beside the counter. Each with its own words.
    #[test]
    fn the_links_stand_github_donate_about() {
        let links = links();
        let kinds: Vec<HeaderLinkKind> = links.iter().map(|link| link.kind).collect();
        assert_eq!(
            kinds,
            [
                HeaderLinkKind::SourceCode,
                HeaderLinkKind::Donate,
                HeaderLinkKind::About
            ]
        );
        let words: Vec<&str> = links.iter().map(|link| link.text.as_str()).collect();
        assert_eq!(words, ["GitHub", "Donate", "About"]);
    }

    #[test]
    fn a_page_the_browser_did_not_open_is_named_the_way_a_tester_types_it() {
        assert_eq!(
            i18n::browser_did_not_open(&shown_address(SOURCE_PAGE)),
            "The browser did not open. The page is github.com/donislawdev/Naughty-Keyboard."
        );
        assert_eq!(
            i18n::browser_did_not_open(&shown_address(SUPPORT_PAGE)),
            "The browser did not open. The page is donislawdev.com/support."
        );
    }
}

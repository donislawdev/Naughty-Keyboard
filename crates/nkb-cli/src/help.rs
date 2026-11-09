//! The help of `nkb` and of each of its commands - out of `main.rs`, which
//! stands at the ceiling of `D113`. Every word here is also on the website:
//! `tests/site_facts.rs` reads each help and fails while the site says
//! something else.

use crate::VERSION;
use crate::output::outln;

/// The option every command that reads the catalogue takes (`D173`), worded
/// once.
fn team_folder_option() {
    outln!("      --team-folder <folder>  Read the team's packs from this folder instead");
    outln!("                 of the one the settings file names. A relative path is");
    outln!("                 read from the current folder");
}

/// Where packs come from, said once for the commands that read them.
fn three_places() {
    outln!("Packs come from three places: the ones built in, the team folder (from");
    outln!("--team-folder or the settings file) and your own folder beside the");
    outln!("settings file. A pack in a folder is used instead of a built-in pack of");
    outln!("the same name, and an own pack instead of a team pack.");
}

pub(crate) fn print_packs_help() {
    outln!("nkb packs - list the packs this build can offer");
    outln!();
    outln!("Usage:");
    outln!("  nkb packs [--team-folder <folder>]");
    outln!();
    three_places();
    outln!();
    outln!("Every run also states which pack sources were read and which were not,");
    outln!("because a list drawn from one source looks exactly like a complete one.");
    outln!();
    outln!("Options:");
    team_folder_option();
    outln!("  -h, --help     Show this help and exit with 0");
}

pub(crate) fn print_send_help() {
    outln!("nkb send - type one value into the focused field");
    outln!();
    outln!("Usage:");
    outln!("  nkb send <pack> [--index N] [--delay S] [--clear] [--team-folder <folder>]");
    outln!();
    outln!("Put one value of a pack into whatever field has the keyboard focus.");
    outln!();
    three_places();
    outln!();
    outln!("Options:");
    outln!("  --index N      which value, counting from 1 in the pack's own order");
    outln!("                 (default: 1). The order of values is the order of testing.");
    outln!("  --delay S      seconds to wait first, so you can focus the target");
    outln!("                 (default: 3). Use 0 when something else focuses it.");
    outln!("  --clear        press Home, Shift+End, Delete first, clearing the current");
    outln!("                 line of the field. Never selects beyond the line, so a");
    outln!("                 multi-line field keeps its other lines. Without this switch");
    outln!("                 the only keys sent are the value's own characters.");
    team_folder_option();
    outln!("  -h, --help     Show this help and exit with 0");
    outln!();
    outln!("The value goes to the focused window, not to standard output.");
    outln!("Everything you read here is on standard error.");
    outln!("Nothing is sent while Ctrl, Alt, Shift or Win is held on the keyboard:");
    outln!("the command waits up to two seconds for them to come up, then refuses.");
    outln!("Nothing is sent to a window running with higher privileges than nkb,");
    outln!("such as an application started as administrator: the system would drop");
    outln!("the keystrokes without a word, so the command refuses and exits with 4.");
    outln!("Nothing is sent when the keyboard focus is on a button, a link, a list item");
    outln!("or a page that is not editable: keys there act on that control, so the");
    outln!("command refuses and exits with 4. Where it cannot tell, it sends.");
    outln!("Press Escape to stop a send in progress: nothing more is typed, the command");
    outln!("says how much arrived and exits with 4. Escape belongs to the command only");
    outln!("while it types - before and after, the key is the application's.");
}

pub(crate) fn print_emit_help() {
    outln!("nkb emit - print a pack's values for a script or a file");
    outln!();
    outln!("Usage:");
    outln!("  nkb emit <pack> [--format json|csv|lines] [--escaped|--raw] [--base64]");
    outln!();
    outln!("Values go to standard output. The account of what came out, and any");
    outln!("note about what a format could not carry, go to standard error - so a");
    outln!("redirected file holds values and nothing else.");
    outln!();
    three_places();
    outln!();
    outln!("Formats:");
    outln!("  json     Both forms of every value, and every field. The default,");
    outln!("           because it is the only one that carries the whole catalogue");
    outln!("  csv      One row per value. Escaped by default, because this");
    outln!("           catalogue contains values that destroy CSV files");
    outln!("  lines    One value per line. Refuses a value containing a line");
    outln!("           break by name, rather than splitting it silently");
    outln!();
    outln!("Options:");
    outln!("      --escaped  Print values as a pack file stores them");
    outln!("      --raw      Print values literally");
    outln!("      --base64   Encode values, for channels that damage text");
    team_folder_option();
    outln!("  -h, --help     Show this help and exit with 0");
}

pub(crate) fn print_show_help() {
    outln!("nkb show - print one pack in full");
    outln!();
    outln!("Usage:");
    outln!("  nkb show <pack> [--team-folder <folder>]");
    outln!();
    outln!("Values are printed ESCAPED, which is the only readable form for a value");
    outln!("made of characters nobody can see. A generated value is printed as its");
    outln!("recipe and is never expanded here. The first line says where the pack");
    outln!("comes from when it is not built in.");
    outln!();
    three_places();
    outln!();
    outln!("Options:");
    team_folder_option();
    outln!("  -h, --help     Show this help and exit with 0");
}

pub(crate) fn print_help() {
    outln!("nkb {VERSION} - malicious test data, one shortcut away");
    outln!();
    outln!("Usage:");
    outln!("  nkb <command> [options]");
    outln!();
    outln!("Commands:");
    outln!("  packs             List the packs this build can offer");
    outln!("  show <pack>       Print one pack in full");
    outln!("  emit <pack>       Print a pack's values for a script or a file");
    outln!("  send <pack>       Type one value into the focused field");
    outln!("  lint <file>       Check a pack file against the format rules");
    outln!("  fmt <file>        Rewrite a pack file in canonical shape");
    outln!("  new-pack <name>   Write the skeleton for a new pack");
    outln!();
    outln!("Options:");
    outln!("  -h, --help        Show this help and exit with 0");
    outln!("      --version     Print the version and exit with 0");
    outln!();
    outln!("The graphical interface is a separate executable: nkb-gui");
}

pub(crate) fn print_new_pack_help() {
    outln!("nkb new-pack - write the skeleton for a new pack");
    outln!();
    outln!("Usage:");
    outln!("  nkb new-pack <name>");
    outln!();
    outln!("The name becomes both the file name and the pack identifier, so it has");
    outln!("to be lower case letters, digits and hyphens, starting with a letter.");
    outln!("The file is written in the current folder and never over an existing one.");
    outln!();
    outln!("Options:");
    outln!("  -h, --help     Show this help and exit with 0");
}

pub(crate) fn print_lint_help() {
    outln!("nkb lint - check a pack file against the format rules");
    outln!();
    outln!("Usage:");
    outln!("  nkb lint <file.toml> [--json] [--explain]");
    outln!();
    outln!("Options:");
    outln!("      --json     Write the verdict as JSON, and nothing else");
    outln!("      --explain  List every rule and whether this build checks it");
    outln!("  -h, --help     Show this help and exit with 0");
    outln!();
    outln!("Exit codes:");
    outln!("  0  the pack passed every rule this build checks");
    outln!("  1  the pack broke at least one blocking rule");
    outln!("  2  the command was called wrongly");
    outln!("  3  there is no pack file to read at that path");
    outln!("  5  the file is there and could not be read, or the verdict");
    outln!("     could not be written to standard output");
}

pub(crate) fn print_fmt_help() {
    outln!("nkb fmt - rewrite a pack file in canonical shape");
    outln!();
    outln!("Usage:");
    outln!("  nkb fmt <file.toml> [--dry-run]");
    outln!();
    outln!("Puts the fields of each table in the order the format defines and lines");
    outln!("up the equals signs. Comments, blank lines and the order of the values");
    outln!("themselves are left exactly as they are - the order of the values is the");
    outln!("order a tester meets them.");
    outln!();
    outln!("Options:");
    outln!("      --dry-run  Say what would change and write nothing");
    outln!("  -h, --help     Show this help and exit with 0");
    outln!();
    outln!("Exit codes:");
    outln!("  0  the file is in shape, or was put in shape");
    outln!("  1  the file is not TOML, so there was nothing to put in order");
    outln!("  2  the command was called wrongly");
    outln!("  3  there is no pack file to read at that path");
    outln!("  4  formatting would have changed a value, so nothing was written");
    outln!("  5  the file could not be read or written, or standard output");
    outln!("     could not be written");
}

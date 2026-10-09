#!/usr/bin/env python3
"""Sign, notarise and staple the .app bundles of a release. Runs ON THE MAC.

    python3 sign_macos.py <directory> <certificate-sha256> <notary-profile>
    python3 sign_macos.py --check <certificate-sha256>

sign_release.py copies this file and one tar per macOS archive into a directory
on the Mac, each tar holding only the .app bundle of that archive, and runs this
over ssh with a terminal. For every `<name>.app.tar` it writes
`<name>.app.signed.tar` beside it, and sign_release.py puts the signed bundle
back into the archive with nothing else moving.

Only the standard library, because this file travels alone. Python 3.11 or newer
for nothing in particular but the date, and the Python that ships with macOS is
3.9, so it runs under the one from Homebrew.

What it refuses, and why each refusal is there. Every one is a failure that is
quiet otherwise, measured on the owner's Mac for another project on 2026-08-28:

  * a keychain this ssh session cannot use. codesign then exits 1 with
    errSecInternalComponent and leaves the bundle UNSIGNED. An ssh session is a
    security session of its own, so a keychain unlocked in the Mac's own Terminal
    is still locked here, and whether it is locked cannot even be asked over ssh,
    so the password is asked every time, on the terminal;
  * a notary profile that does not answer, told apart from a locked keychain,
    which reports the same error;
  * a certificate that is not the pinned one, read back out of the signed bundle;
  * a notarisation that came back anything but Accepted;
  * a ticket that did not staple, a bundle that does not verify strictly, and a
    bundle Gatekeeper rejects when Gatekeeper is on.

Nothing here changes directory. A relative path given to a script that changes
directory names something else afterwards, and on the first release of the
project this ritual came from that turned into a refusal that blamed the
certificate.
"""
import hashlib
import io
import os
import subprocess
import sys
import tarfile
import tempfile

IDENTITY_KIND = "Developer ID Application"
KEYCHAIN = os.path.expanduser("~/Library/Keychains/login.keychain-db")
INBOX = ".app.tar"
OUTBOX = ".app.signed.tar"


class Refused(Exception):
    pass


def say(text):
    print("  " + text, flush=True)


def ask(command, run=subprocess.run):
    """A command's exit code and everything it printed."""
    try:
        return run(command, capture_output=True, text=True, encoding="utf-8", errors="replace")
    except OSError as error:
        return subprocess.CompletedProcess(command, 127, "", str(error))


# --------------------------------------------------------------------------- #
# the keychain, the credentials and the certificate
# --------------------------------------------------------------------------- #

def unlock_keychain(run=subprocess.run):
    """Asked every time, on the terminal. A password in argv is visible in ps to
    every process on the machine, and over ssh it cannot be asked whether the
    keychain is already unlocked: that question answers "User interaction is not
    allowed" either way. Unlocking an unlocked keychain changes nothing."""
    print("\n  An ssh session cannot see whether the keychain is unlocked, so it asks.\n"
          "  Your Mac password. If the keychain is already unlocked, it is the same.\n", flush=True)
    if run(["security", "unlock-keychain", KEYCHAIN]).returncode != 0:
        raise Refused("the keychain would not unlock, so nothing has been signed")
    say("keychain reachable")


def check_profile(profile, run=subprocess.run):
    done = ask(["xcrun", "notarytool", "history", "--keychain-profile", profile], run)
    if done.returncode == 0:
        say("the notary profile %s answers" % profile)
        return
    said = done.stdout + done.stderr
    if "keychainLocked" in said:
        raise Refused("the keychain locked itself again, and nothing has been signed")
    if "No Keychain password item" in said:
        raise Refused("there is no notary profile called %s on this Mac. Create it once, in the "
                      "Mac's own Terminal and not over ssh:\n  xcrun notarytool store-credentials %s "
                      "--apple-id <apple id> --team-id <team id>\nwith an app-specific password from "
                      "appleid.apple.com." % (profile, profile))
    raise Refused("notarytool does not answer for %s:\n%s" % (profile, said.strip()))


def sha1_for(listing, pin):
    """The SHA-1 of the certificate whose SHA-256 is the pin, out of what
    `security find-certificate -a -Z` prints: each certificate's SHA-256 line,
    then its SHA-1 line. codesign selects by SHA-1, which is not worth pinning,
    so the SHA-1 is found from the pin and never written down."""
    current = None
    for line in listing.splitlines():
        line = line.strip()
        if line.startswith("SHA-256 hash:"):
            current = line.split(":", 1)[1].strip().lower()
        elif line.startswith("SHA-1 hash:"):
            if current == pin.lower():
                return line.split(":", 1)[1].strip().upper()
            current = None
    return None


def find_identity(pin, run=subprocess.run):
    listing = ask(["security", "find-certificate", "-a", "-c", IDENTITY_KIND, "-Z"], run)
    sha1 = sha1_for(listing.stdout, pin)
    if not sha1:
        raise Refused("no %s certificate on this Mac hashes to the pin %s. A renewal is a "
                      "DIFFERENT certificate, and .github/release/codesign.toml moves with it." % (IDENTITY_KIND, pin))
    identities = ask(["security", "find-identity", "-v", "-p", "codesigning"], run)
    if sha1 not in identities.stdout.upper():
        raise Refused("the pinned certificate is on this Mac without its private key, so it cannot sign")
    say("the pinned certificate is here, with its key")
    return sha1


def certificate_of(bundle, run=subprocess.run):
    """The SHA-256 of the certificate that actually signed a bundle. The prefix
    codesign writes to is a path, so nothing changes directory."""
    with tempfile.TemporaryDirectory() as folder:
        ask(["codesign", "-d", "--extract-certificates=" + os.path.join(folder, "cert"), bundle], run)
        first = os.path.join(folder, "cert0")
        if not os.path.isfile(first):
            return "none"
        with open(first, "rb") as handle:
            return hashlib.sha256(handle.read()).hexdigest()


# --------------------------------------------------------------------------- #
# one bundle, all the way through
# --------------------------------------------------------------------------- #

def notarised(output):
    return "status: Accepted" in output


def must(done, what):
    if done.returncode != 0:
        raise Refused("%s (exit %d):\n%s" % (what, done.returncode, (done.stdout + done.stderr).strip()))


def sign_bundle(app, sha1, pin, profile, work, run=subprocess.run):
    name = os.path.basename(app)
    # The hardened runtime is what notarisation asks for, and the timestamp is
    # what keeps the signature alive past the certificate.
    must(ask(["codesign", "--force", "--options", "runtime", "--timestamp", "--sign", sha1, app], run),
         "codesign could not sign %s. The usual cause is a keychain this session cannot use, which "
         "fails as errSecInternalComponent and leaves the bundle unsigned" % name)
    actual = certificate_of(app, run)
    if actual != pin.lower():
        raise Refused("%s was signed by a DIFFERENT certificate\n  expected %s\n  got      %s"
                      % (name, pin.lower(), actual))
    must(ask(["codesign", "--verify", "--strict", "--deep", app], run), "%s does not verify strictly" % name)
    say("%s signed by the pinned certificate" % name)

    # Apple takes a zip, not the tar.gz that is published, so this zip only
    # carries the bundle there. What is stapled and handed back is the bundle.
    upload = os.path.join(work, name + ".notarise.zip")
    must(ask(["ditto", "-c", "-k", "--keepParent", app, upload], run), "ditto could not zip %s" % name)
    say("notarising %s, which takes a few minutes" % name)
    done = ask(["xcrun", "notarytool", "submit", upload, "--keychain-profile", profile, "--wait"], run)
    os.remove(upload)
    if done.returncode != 0 or not notarised(done.stdout):
        raise Refused("the notarisation of %s did not come back Accepted:\n%s" % (name, (done.stdout + done.stderr).strip()))

    must(ask(["xcrun", "stapler", "staple", app], run), "the ticket would not staple onto %s" % name)
    must(ask(["xcrun", "stapler", "validate", app], run), "%s does not validate against its own ticket" % name)
    # Gatekeeper can be switched off, and then it accepts everything. Asked only
    # when it is on, after the two checks above that do not depend on it.
    if "assessments enabled" in ask(["spctl", "--status"], run).stdout:
        must(ask(["spctl", "-a", "-t", "exec", "-vv", app], run), "Gatekeeper rejects %s" % name)
        say("%s notarised, stapled, and Gatekeeper accepts it" % name)
    else:
        say("%s notarised and stapled. Gatekeeper is off on this Mac, so it was not asked" % name)


# --------------------------------------------------------------------------- #
# the tars that carry the bundles to the Mac and back
# --------------------------------------------------------------------------- #

def unpack(path, into):
    """One .app bundle of files and directories, every name checked before
    anything is written: nothing absolute, nothing that climbs out with `..`,
    no link, and one bundle at the top with nothing beside it."""
    with tarfile.open(path) as packed:
        members = packed.getmembers()
        tops = {m.name.split("/")[0] for m in members}
        for member in members:
            parts = member.name.split("/")
            if member.name.startswith("/") or ".." in parts or not (member.isdir() or member.isreg()):
                raise Refused("%s holds %s, which is not a file or a directory inside the bundle"
                              % (os.path.basename(path), member.name))
        if len(tops) != 1 or not next(iter(tops)).endswith(".app"):
            raise Refused("%s holds %s, and it should hold one .app bundle and nothing else"
                          % (os.path.basename(path), sorted(tops)))
        for member in members:
            target = os.path.join(into, *member.name.split("/"))
            if member.isdir():
                os.makedirs(target, exist_ok=True)
                continue
            os.makedirs(os.path.dirname(target), exist_ok=True)
            with packed.extractfile(member) as source, open(target, "wb") as out:
                out.write(source.read())
            os.chmod(target, 0o755 if member.mode & 0o111 else 0o644)
    return os.path.join(into, tops.pop())


def pack(folder, path):
    """The signed bundle, as a tar with no owner and no extended attributes."""
    with tarfile.open(path, "w", format=tarfile.USTAR_FORMAT) as out:
        for base, dirs, files in os.walk(folder):
            dirs.sort()
            for name in sorted(dirs) + sorted(files):
                full = os.path.join(base, name)
                info = out.gettarinfo(full, os.path.relpath(full, folder).replace(os.sep, "/"))
                info.uid = info.gid = 0
                info.uname = info.gname = ""
                info.mtime = 0
                if info.isreg():
                    with open(full, "rb") as handle:
                        out.addfile(info, io.BytesIO(handle.read()))
                else:
                    out.addfile(info)


def inbox(directory):
    return sorted(n for n in os.listdir(directory) if n.endswith(INBOX))


def main(argv=None):
    argv = list(sys.argv[1:] if argv is None else argv)
    try:
        if argv[:1] == ["--check"] and len(argv) == 2:
            find_identity(argv[1])
            return 0
        if len(argv) != 3:
            print(__doc__.splitlines()[2].strip() + "\n" + __doc__.splitlines()[3].strip(), file=sys.stderr)
            return 2
        directory, pin, profile = os.path.abspath(argv[0]), argv[1].lower(), argv[2]
        bundles = inbox(directory)
        if not bundles:
            raise Refused("%s holds no %s files to sign" % (directory, INBOX))
        print("[1/3] the keychain and the credentials", flush=True)
        unlock_keychain()
        check_profile(profile)
        sha1 = find_identity(pin)
        print("[2/3] the bundles", flush=True)
        for name in bundles:
            work = os.path.join(directory, name[:-len(INBOX)] + ".work")
            os.makedirs(work)
            app = unpack(os.path.join(directory, name), work)
            sign_bundle(app, sha1, pin, profile, directory)
            pack(work, os.path.join(directory, name[:-len(INBOX)] + OUTBOX))
        print("[3/3] %d bundle(s) signed, notarised and stapled" % len(bundles), flush=True)
        return 0
    except Refused as refusal:
        print("sign_macos: %s\nNothing has been handed back." % refusal, file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())

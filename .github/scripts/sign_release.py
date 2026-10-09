#!/usr/bin/env python3
"""Phase B of the release: sign the build on the machines that hold the keys.

    python .github/scripts/sign_release.py v0.1.0 --macos-host user@mac
    python .github/scripts/sign_release.py --rehearse <run id> --macos-host user@mac
    python .github/scripts/sign_release.py v0.1.0 --macos-host user@mac --dry-run
    python .github/scripts/sign_release.py v0.1.0 --attest-only

Run by a person, at the machine the card is plugged into, with the Mac awake.
The card asks for its PIN and the Mac for its password on this terminal, and
nothing in this repository ever holds either.

Why a person runs it
--------------------
The key for Windows is on a cryptographic card that cannot be exported, so no
runner GitHub hosts can reach it. A runner of our own could, and in a public
repository that is a machine anybody can aim a pull request at. The key for
macOS and the credentials of Apple's notary service are in a keychain on a Mac.
So the build happens on GitHub (phase A, release.yml) and the signatures happen
here, and what travels between them is the build and a signed statement of how
it was made.

What it does, in order, and what it refuses
-------------------------------------------
 0. before anything is downloaded: the pinned Windows certificate is in this
    machine's store and has not expired, signtool is here, and the Mac answers
    with a Python that can run its half and holds the pinned certificate with
    its key. A missing piece found after the card has signed would leave half a
    release;
 1. downloads the build phase A handed over;
 2. verifies the statement phase A made about every file of it before touching
    any: built by release.yml in this repository, from this tag, on a runner
    GitHub hosts. Signing something nobody checked is how a supply chain gets a
    signature on it;
 3. signs the two Windows programs with an RFC 3161 timestamp, and reads the
    certificate back out of each signed file. A second code signing
    certificate on this machine signs just as willingly;
 4. hands the two macOS bundles to the Mac, which signs, notarises and staples
    them (sign_macos.py);
 5. puts every signed program back into its archive with release.py, so the
    archive is written by the same code that wrote it in phase A, and only the
    signed files differ;
 6. writes verify-SHA256SUMS.txt over everything that will be on the page,
    uploads to the DRAFT and checks the draft holds what was signed;
 7. starts phase C (attest-release.yml) from the code of the tag, with the
    digest of the checksums, to make the statement about the signed files;
 8. waits for it, stopping at once if that run fails, then checks the draft is
    the whole release: every file and no other, each the one signed here, the
    statement describing every archive by the command README.md gives, and
    still a draft. A draft missing one file looks almost exactly like a
    finished one.

--rehearse signs the build of a run started by hand on main and stops before
step 6, so the whole ritual can be tried without a tag. --attest-only runs
steps 7 and 8 again for a draft that holds the signed files already, without
signing anything again. Nothing here publishes: the release stays a draft until
a person reads it and presses the button.
"""
import argparse
import dataclasses
import datetime
import hashlib
import io
import json
import os
import re
import shlex
import shutil
import subprocess
import sys
import tarfile
import time
import tomllib

import contents
import release
from contents import ROOT, Refused

PINS = os.path.join(ROOT, ".github", "release", "codesign.toml")
SIGNER_WORKFLOW = "/.github/workflows/release.yml"
CODE_SIGNING_OID = "1.3.6.1.5.5.7.3.3"
WARN_DAYS = 90
SUMS = release.SUMS
BUILD_BUNDLE = "build.provenance.sigstore.json"
MAC_SCRIPT = os.path.join(ROOT, ".github", "scripts", "sign_macos.py")
MAC_PYTHON = "/opt/homebrew/bin/python3.14"
# What signing and stapling may add to a bundle: the seal of its resources, the
# folder it lives in, and the notarisation ticket. Anything else that appears or
# goes is refused.
SIGNED_ADDITIONS = ("Contents/_CodeSignature",) + release.SIGNED_FILES
# Phase C, which makes the statement about the signed bytes, and how long to
# wait for it. A run takes about a minute when GitHub is not busy.
ATTEST_WORKFLOW = "attest-release.yml"
WAIT_FOR_STATEMENT = 300
POLL_SECONDS = 10


def say(text):
    print(text, flush=True)


def run(command, **kw):
    """Run something and let it print, a failure refused with its command."""
    say("    $ " + " ".join(str(c) for c in command))
    if subprocess.run(command, **kw).returncode != 0:
        raise Refused(["%s failed" % " ".join(str(c) for c in command)])


def sha256_of(path):
    return contents.sha256_of(path)


# --------------------------------------------------------------------------- #
# the pins
# --------------------------------------------------------------------------- #

@dataclasses.dataclass(frozen=True)
class Pin:
    sha256: str
    expires: datetime.date


@dataclasses.dataclass(frozen=True)
class Pins:
    windows: Pin
    timestamp_url: str
    macos: Pin
    notary_profile: str


def load_pins(path=PINS):
    data = contents.read_toml(path)
    problems = []
    if data.get("schema") != 1:
        problems.append("codesign.toml: schema %r, and this script reads 1" % data.get("schema"))

    def pin(side):
        table = data.get(side, {})
        digest, expires = table.get("certificate-sha256", ""), table.get("expires")
        if not re.fullmatch(r"[0-9a-f]{64}", digest):
            problems.append("codesign.toml: [%s] certificate-sha256 is not a SHA-256" % side)
        if not isinstance(expires, datetime.date):
            problems.append("codesign.toml: [%s] expires is not a date" % side)
        return Pin(digest, expires)

    windows, macos = pin("windows"), pin("macos")
    url = data.get("windows", {}).get("timestamp-url", "")
    profile = data.get("macos", {}).get("notary-profile", "")
    if not url.startswith("http"):
        problems.append("codesign.toml: [windows] timestamp-url is not an address")
    if not profile:
        problems.append("codesign.toml: [macos] has no notary-profile")
    if problems:
        raise Refused(problems)
    return Pins(windows, url, macos, profile)


def expiry_notice(expires, today):
    """What to say about a certificate that will not last, and a refusal for one
    that did not."""
    days = (expires - today).days
    if days < 0:
        raise Refused(["the certificate expired on %s, and nothing can be signed with it. A renewal "
                       "is a DIFFERENT certificate, so .github/release/codesign.toml moves with it." % expires])
    if days < WARN_DAYS:
        return "WARNING: %d days left on this certificate. A renewal is a DIFFERENT certificate, " \
               "so its digest and date in .github/release/codesign.toml move with it." % days
    return "%d days left on this certificate" % days


# --------------------------------------------------------------------------- #
# Windows: the store, signtool and the signed file
# --------------------------------------------------------------------------- #

def powershell(script, values=None, run_=subprocess.run):
    """A PowerShell script, with every value it needs in the environment and
    never in its text: a file name with a quote in it would end a string and
    the rest would run as PowerShell. Whatever it writes to stderr is printed,
    because a PowerShell error does not have to change its exit code."""
    done = run_(["pwsh", "-NoProfile", "-NonInteractive", "-Command", script],
                capture_output=True, text=True, encoding="utf-8", errors="replace",
                env={**os.environ, **(values or {})})
    if done.stderr.strip():
        say("    PowerShell said:")
        for line in done.stderr.strip().splitlines():
            say("      " + line)
    if done.returncode != 0:
        raise Refused(["PowerShell failed (exit %d)" % done.returncode])
    return done.stdout


# The store is opened through .NET, not through the Cert: drive, which a
# PowerShell started from another one may not have loaded - and its absence is
# reported with exit code zero. The certificate is chosen by the OID of code
# signing, not by a name Windows translates.
STORE_SCRIPT = "\n".join([
    "$out = @()",
    "foreach ($where in 'CurrentUser', 'LocalMachine') {",
    "  $store = New-Object System.Security.Cryptography.X509Certificates.X509Store('My', $where)",
    "  try { $store.Open('ReadOnly') } catch { continue }",
    "  foreach ($c in $store.Certificates) {",
    "    $eku = @()",
    "    foreach ($x in $c.Extensions) {",
    "      if ($x -is [System.Security.Cryptography.X509Certificates.X509EnhancedKeyUsageExtension]) {",
    "        foreach ($u in $x.EnhancedKeyUsages) { $eku += $u.Value }",
    "      }",
    "    }",
    "    if ($eku -notcontains $env:NKB_OID) { continue }",
    "    $h = [System.Security.Cryptography.SHA256]::HashData($c.RawData)",
    "    $out += [pscustomobject]@{",
    "      sha256 = [Convert]::ToHexString($h).ToLower()",
    "      thumbprint = $c.Thumbprint",
    "      key = $c.HasPrivateKey",
    "    }",
    "  }",
    "  $store.Close()",
    "}",
    "ConvertTo-Json -InputObject @($out) -Compress",
])

SIGNATURE_SCRIPT = "\n".join([
    "$s = Get-AuthenticodeSignature -LiteralPath $env:NKB_SIGNED_FILE",
    "$h = ''",
    "if ($s.SignerCertificate) {",
    "  $h = [Convert]::ToHexString([System.Security.Cryptography.SHA256]::HashData($s.SignerCertificate.RawData)).ToLower()",
    "}",
    "[pscustomobject]@{",
    "  status = $s.Status.ToString()",
    "  sha256 = $h",
    "  timestamped = [bool]$s.TimeStamperCertificate",
    "} | ConvertTo-Json -Compress",
])


def thumbprint_for(entries, pin):
    """signtool selects by SHA-1, which is not worth pinning, so the SHA-1 is
    found from the pinned SHA-256 and never written down."""
    for entry in entries:
        if entry.get("sha256") == pin:
            if not entry.get("key"):
                raise Refused(["the pinned certificate is in the store without its private key. "
                               "Is the card in the reader?"])
            return entry["thumbprint"]
    raise Refused(["the pinned certificate (%s...) is not in the Windows store. Plug in the card "
                   "reader and check that the card manager sees the card." % pin[:16]])


def signature_problems(result, pin, name):
    problems = []
    if result.get("status") != "Valid":
        problems.append("%s: the signature is %s, not Valid" % (name, result.get("status")))
    if result.get("sha256") != pin:
        problems.append("%s was signed by a DIFFERENT certificate\n  expected %s\n  got      %s"
                        % (name, pin, result.get("sha256") or "none"))
    if not result.get("timestamped"):
        problems.append("%s carries no timestamp, so its signature would die with the certificate" % name)
    return problems


def find_signtool(kits=r"C:\Program Files (x86)\Windows Kits\10\bin"):
    """The x64 signtool of the newest Windows SDK, by version number."""
    versions = []
    if os.path.isdir(kits):
        for name in os.listdir(kits):
            if re.fullmatch(r"\d+(\.\d+)+", name) and os.path.isfile(os.path.join(kits, name, "x64", "signtool.exe")):
                versions.append(tuple(int(n) for n in name.split(".")))
    if not versions:
        raise Refused(["no signtool.exe under %s. Install the Windows SDK, its Signing Tools are enough." % kits])
    return os.path.join(kits, ".".join(str(n) for n in max(versions)), "x64", "signtool.exe")


def sign_command(signtool, thumbprint, timestamp_url, path):
    return [signtool, "sign", "/sha1", thumbprint, "/fd", "sha256", "/tr", timestamp_url,
            "/td", "sha256", path]


# --------------------------------------------------------------------------- #
# the build, and the check before touching it
# --------------------------------------------------------------------------- #

def handover_files(tag):
    return sorted(release.handover_names(tag) + [release.BUILD_SUMS, BUILD_BUNDLE])


def verify_command(path, tag, rehearsal, repository):
    """The statement phase A made, held to who made it and from what."""
    ref = "refs/heads/main" if rehearsal else "refs/tags/%s" % tag
    return ["gh", "attestation", "verify", path, "--bundle", os.path.join(os.path.dirname(path), BUILD_BUNDLE),
            "--repo", repository, "--signer-workflow", repository + SIGNER_WORKFLOW,
            "--source-ref", ref, "--deny-self-hosted-runners"]


def check_build(directory, tag, rehearsal, repository, run_=None):
    found = sorted(os.listdir(directory))
    wanted = handover_files(tag)
    if found != wanted:
        raise Refused(["the build is not what phase A hands over",
                       "missing: %s" % sorted(set(wanted) - set(found)),
                       "not listed: %s" % sorted(set(found) - set(wanted))])
    with open(os.path.join(directory, release.BUILD_SUMS), encoding="utf-8") as handle:
        listed = release.listed_digests(handle.read(), release.BUILD_SUMS)
    subjects = release.handover_names(tag)
    if sorted(listed) != subjects:
        raise Refused(["%s lists %s" % (release.BUILD_SUMS, sorted(listed))])
    for name in subjects:
        if sha256_of(os.path.join(directory, name)) != listed[name]:
            raise Refused(["%s is not the file %s lists" % (name, release.BUILD_SUMS)])
        (run_ or run)(verify_command(os.path.join(directory, name), tag, rehearsal, repository))
    say("  %d files verified against the statement phase A made" % len(subjects))


# --------------------------------------------------------------------------- #
# archives in and out
# --------------------------------------------------------------------------- #

def replace_entry(entries, name, data):
    found = [e for e in entries if e.name == name]
    if len(found) != 1 or found[0].kind != "file":
        raise Refused(["the archive does not hold %s as one file" % name])
    return [dataclasses.replace(e, data=data) if e.name == name else e for e in entries]


def bundle_entries(entries, app):
    return [e for e in entries if e.name == app or e.name.startswith(app + "/")]


def tar_bytes(entries):
    """The bundle alone, as an uncompressed tar, to carry it to the Mac."""
    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode="w", format=tarfile.USTAR_FORMAT) as out:
        for entry in entries:
            info = tarfile.TarInfo(entry.name)
            info.mode = entry.mode
            if entry.kind == "directory":
                info.type = tarfile.DIRTYPE
                out.addfile(info)
            elif entry.kind == "link":
                info.type, info.linkname = tarfile.SYMTYPE, entry.target
                out.addfile(info)
            else:
                info.size = len(entry.data)
                out.addfile(info, io.BytesIO(entry.data))
    return buffer.getvalue()


def entries_of_tar(data):
    found = []
    with tarfile.open(fileobj=io.BytesIO(data)) as packed:
        for member in packed.getmembers():
            name = member.name[2:] if member.name.startswith("./") else member.name
            if member.isdir():
                found.append(release.Entry(name, "directory", 0o755))
            elif member.isreg():
                mode = 0o755 if member.mode & 0o111 else 0o644
                found.append(release.Entry(name, "file", mode, packed.extractfile(member).read()))
            else:
                raise Refused(["the signed bundle holds %s, which is neither a file nor a directory" % name])
    return sorted(found, key=lambda e: e.name)


def signed_bundle(before, after, app):
    """The bundle as it came back, refused unless signing added only the seal
    and the ticket, removed nothing, and kept the program executable."""
    names_before = {e.name for e in before}
    names_after = {e.name for e in after}
    allowed = {app + "/" + n for n in SIGNED_ADDITIONS}
    problems = []
    if names_before - names_after:
        problems.append("signing removed %s" % sorted(names_before - names_after))
    if names_after - names_before - allowed:
        problems.append("signing added %s" % sorted(names_after - names_before - allowed))
    for needed in allowed - {app + "/Contents/_CodeSignature"}:
        if needed not in names_after:
            problems.append("the bundle came back without %s, so it is not signed and stapled" % needed)
    for entry in before:
        if entry.kind == "file" and entry.mode & 0o111:
            again = next((e for e in after if e.name == entry.name), None)
            if again is not None and not again.mode & 0o111:
                problems.append("%s is no longer executable" % entry.name)
    if problems:
        raise Refused(["%s: %s" % (app, p) for p in problems])
    return after


def rewrite(path, entries, archive_format, epoch, before):
    """The archive again, refused if a name came or went that nobody signed."""
    release.write_archive(path, archive_format, entries, epoch)
    _, again, _ = release.read_archive(path)
    return sorted(e.name for e in again), sorted(e.name for e in before)


# --------------------------------------------------------------------------- #
# the two halves
# --------------------------------------------------------------------------- #

def sign_windows(directory, tag, pins, thumbprint, signtool, dry_run):
    version = contents.version_of_tag(tag, contents.load_workspace())
    for archive in release.archives_for("windows"):
        path = os.path.join(directory, archive.name(version))
        archive_format, entries, epoch = release.read_archive(path)
        program = release.program_file(archive)
        work = os.path.join(directory, archive.name(version) + ".work")
        os.makedirs(work, exist_ok=True)
        target = os.path.join(work, program)
        with open(target, "wb") as handle:
            handle.write(next(e.data for e in entries if e.name == program))
        if dry_run:
            say("  DRY RUN, would run: " + " ".join(sign_command(signtool, thumbprint, pins.timestamp_url, target)))
            continue
        run(sign_command(signtool, thumbprint, pins.timestamp_url, target))
        run([signtool, "verify", "/pa", target])
        result = json.loads(powershell(SIGNATURE_SCRIPT, {"NKB_SIGNED_FILE": target}) or "{}")
        problems = signature_problems(result, pins.windows.sha256, program)
        if problems:
            raise Refused(problems + ["Nothing has been uploaded."])
        with open(target, "rb") as handle:
            signed = replace_entry(entries, program, handle.read())
        names, before = rewrite(path, signed, archive_format, epoch, entries)
        if names != before:
            raise Refused(["%s came back holding %s, and held %s" % (archive.name(version), names, before)])
        shutil.rmtree(work)
        say("  %s: %s signed by the pinned certificate, timestamped, repacked" % (archive.name(version), program))


def sign_macos(directory, tag, pins, host, python, dry_run):
    version = contents.version_of_tag(tag, contents.load_workspace())
    remote = "nkb-signing-%s" % tag
    held = {}
    for archive in release.archives_for(contents.MACOS):
        name = archive.name(version)
        archive_format, entries, epoch = release.read_archive(os.path.join(directory, name))
        app = release.program_file(archive) + ".app"
        held[name] = (archive_format, entries, epoch, app)
        with open(os.path.join(directory, name + ".app.tar"), "wb") as handle:
            handle.write(tar_bytes(bundle_entries(entries, app)))
    if dry_run:
        say("  DRY RUN, would sign %d bundle(s) on %s" % (len(held), host))
        return
    quoted = shlex.quote(remote)
    run(["ssh", host, "rm -rf %s && mkdir -p %s" % (quoted, quoted)])
    run(["scp", MAC_SCRIPT] + [os.path.join(directory, n + ".app.tar") for n in sorted(held)] + ["%s:%s/" % (host, remote)])
    run(["ssh", "-t", host, "%s %s/sign_macos.py %s %s %s" % (
        shlex.quote(python), quoted, quoted, pins.macos.sha256, shlex.quote(pins.notary_profile))])
    for name in sorted(held):
        run(["scp", "%s:%s/%s.app.signed.tar" % (host, remote, name), directory])
    run(["ssh", host, "rm -rf %s" % quoted])
    for name, (archive_format, entries, epoch, app) in sorted(held.items()):
        with open(os.path.join(directory, name + ".app.signed.tar"), "rb") as handle:
            after = signed_bundle(bundle_entries(entries, app), entries_of_tar(handle.read()), app)
        rest = [e for e in entries if e not in bundle_entries(entries, app)]
        path = os.path.join(directory, name)
        release.write_archive(path, archive_format, sorted(rest + after, key=lambda e: e.name), epoch)
        for leftover in (name + ".app.tar", name + ".app.signed.tar"):
            os.remove(os.path.join(directory, leftover))
        say("  %s: %s signed, notarised and stapled, repacked" % (name, app))


# --------------------------------------------------------------------------- #
# what goes on the page
# --------------------------------------------------------------------------- #

def name_for_publication(directory, tag):
    """build.sha256 describes bytes that signing changed, so it does not go on
    the page beside the real list. The statement of the build does, under a name
    a person can use: it still describes, byte for byte, every file nothing
    signed - the Linux archives and the bill of materials."""
    version = contents.version_of_tag(tag, contents.load_workspace())
    os.remove(os.path.join(directory, release.BUILD_SUMS))
    os.replace(os.path.join(directory, BUILD_BUNDLE),
               os.path.join(directory, release.provenance_bundle_name(version)))


def checksums(directory, names):
    return "".join("%s  %s\n" % (sha256_of(os.path.join(directory, n)), n) for n in sorted(names))


def write_checksums(directory, tag):
    names = [n for n in release.published_files(tag) if n != SUMS]
    found = sorted(n for n in os.listdir(directory) if not n.endswith(".work"))
    if found != sorted(names):
        raise Refused(["what is about to be published is not what it should be",
                       "missing: %s" % sorted(set(names) - set(found)),
                       "not listed: %s" % sorted(set(found) - set(names))])
    contents.write(os.path.join(directory, SUMS), checksums(directory, names))
    say("  %s lists %d files" % (SUMS, len(names)))


def draft_problems(view, expected, sums_on_page, sums_here):
    problems = []
    names = sorted(asset["name"] for asset in view.get("assets", []))
    missing = sorted(set(expected) - set(names))
    if missing:
        problems.append("the draft is missing %s" % missing)
    if sums_on_page != sums_here:
        problems.append("the %s on the draft is not the one written here" % SUMS)
    if not view.get("isDraft"):
        problems.append("the release is no longer a draft. Nothing here publishes, so somebody or "
                        "something else did")
    return problems


def upload(directory, tag, repository):
    state = release.release_state(tag)
    if state != "draft":
        raise Refused(["%s is %s, and phase B uploads only to the draft phase A opened" % (tag, state)])
    files = [os.path.join(directory, n) for n in published_files(tag)]
    run(["gh", "release", "upload", tag, "--repo", repository, "--clobber"] + files)
    view = json.loads(subprocess.run(["gh", "release", "view", tag, "--repo", repository, "--json",
                                      "assets,isDraft"], capture_output=True, text=True).stdout or "{}")
    back = os.path.join(directory, "confirm")
    run(["gh", "release", "download", tag, "--repo", repository, "--pattern", SUMS, "--dir", back, "--clobber"])
    with open(os.path.join(back, SUMS), "rb") as one, open(os.path.join(directory, SUMS), "rb") as two:
        problems = draft_problems(view, published_files(tag), one.read(), two.read())
    shutil.rmtree(back)
    if problems:
        raise Refused(problems)
    say("  the draft holds every signed file and the checksums written here, and is still a draft")


# --------------------------------------------------------------------------- #
# phase C, and the draft it leaves
# --------------------------------------------------------------------------- #

def gh_json(command, what, run_=subprocess.run):
    done = run_(["gh"] + command, capture_output=True, text=True)
    if done.returncode != 0:
        raise Refused(["gh could not read %s:\n%s" % (what, done.stderr.strip())])
    return json.loads(done.stdout or "null")


def draft_view(tag, repository, run_=subprocess.run):
    return gh_json(["release", "view", tag, "--repo", repository, "--json", "assets,isDraft"],
                   "the draft of %s" % tag, run_)


def statement_id(view, tag):
    """The id of the statement about the signed bytes on the draft, or None. An
    id, because a statement left by an earlier signing has the same name and
    describes other bytes."""
    name = release.sbom_bundle_name(tag[1:])
    return next((a.get("id") for a in view.get("assets", []) if a.get("name") == name), None)


def attestation_runs(repository, run_=subprocess.run):
    runs = gh_json(["run", "list", "--repo", repository, "--workflow", ATTEST_WORKFLOW, "--limit", "20",
                    "--json", "databaseId,status,conclusion,url"], "the runs of %s" % ATTEST_WORKFLOW, run_)
    return {str(r["databaseId"]): r for r in runs or []}


def ask_for_the_statement(tag, digest, repository, run_=subprocess.run):
    """Starts phase C from the code of the tag itself, so the statement is made
    by the workflow this release carries. Returns the runs there were before."""
    earlier = set(attestation_runs(repository, run_))
    command = ["gh", "workflow", "run", ATTEST_WORKFLOW, "--repo", repository, "--ref", tag,
               "-f", "tag=" + tag, "-f", "digest=" + digest]
    say("    $ " + " ".join(command))
    done = run_(command, capture_output=True, text=True)
    if done.returncode != 0:
        raise Refused(["gh could not start %s:\n%s" % (ATTEST_WORKFLOW, done.stderr.strip())])
    return earlier


def again(tag):
    return ("Nothing is signed again by asking once more: python .github/scripts/sign_release.py %s "
            "--attest-only" % tag)


def wait_for_the_statement(tag, repository, earlier_runs, earlier_id, run_=subprocess.run,
                           sleep=time.sleep, clock=time.monotonic, limit=WAIT_FOR_STATEMENT):
    """The view of the draft once a NEW statement is on it. A run of phase C that
    failed stops the wait at once, and one that passed without leaving a
    statement is refused rather than waited out."""
    started = clock()
    passed = False
    while True:
        view = draft_view(tag, repository, run_)
        current = statement_id(view, tag)
        if current is not None and current != earlier_id:
            return view
        if passed:
            raise Refused(["%s passed and the draft holds no new statement" % ATTEST_WORKFLOW, again(tag)])
        ours = [r for i, r in attestation_runs(repository, run_).items() if i not in earlier_runs]
        failed = [r for r in ours if r.get("status") == "completed" and r.get("conclusion") != "success"]
        if failed:
            raise Refused(["%s ended %s: %s" % (ATTEST_WORKFLOW, failed[0].get("conclusion"), failed[0].get("url")),
                           again(tag)])
        passed = any(r.get("status") == "completed" for r in ours)
        if clock() - started > limit:
            raise Refused(["the statement about the signed files has not reached the draft after %d seconds. "
                           "Look at the runs of %s before publishing anything." % (limit, ATTEST_WORKFLOW),
                           again(tag)])
        if not passed:
            say("    waiting for %s" % ATTEST_WORKFLOW)
            sleep(POLL_SECONDS)


def signed_problems(view, tag, directory):
    """What phase B uploaded is on the draft, each file the one signed here,
    and the draft is still a draft."""
    problems = []
    on_page = {a.get("name"): a for a in view.get("assets", [])}
    for name in release.published_files(tag):
        here = "sha256:" + sha256_of(os.path.join(directory, name))
        if on_page.get(name, {}).get("digest") != here:
            problems.append("%s on the draft is not the file signed here" % name)
    if not view.get("isDraft"):
        problems.append("the release is no longer a draft. Nothing here publishes, so somebody or "
                        "something else did")
    return problems


def complete_problems(view, tag, directory):
    """What keeps the draft from being the release: a file missing or one too
    many, a file that is not the one signed here, or a draft no longer a draft."""
    names = sorted(a.get("name") for a in view.get("assets", []))
    wanted = release.page_files(tag)
    problems = [] if names == wanted else ["the draft holds %s, and a release of %s holds %s"
                                           % (names, tag, wanted)]
    return problems + signed_problems(view, tag, directory)


def confirm_draft(view, tag, directory, repository, run_=None):
    """The draft is the release, and the statement on it describes every archive
    signed here: checked with the command README.md gives, against the copy the
    draft holds."""
    problems = complete_problems(view, tag, directory)
    if problems:
        raise Refused(problems)
    name = release.sbom_bundle_name(tag[1:])
    back = os.path.join(directory, "confirm")
    run(["gh", "release", "download", tag, "--repo", repository, "--pattern", name, "--dir", back, "--clobber"])
    promise = release.load_promise()
    for archive in contents.load_archives():
        path = os.path.join(directory, archive.name(tag[1:]))
        (run_ or run)(release.attestation_check(path, repository, promise.predicate, os.path.join(back, name)))
    shutil.rmtree(back, ignore_errors=True)
    say("  the draft holds the %d files of the release, each the one signed here, the statement describes "
        "every archive, and it is still a draft" % len(release.page_files(tag)))


# --------------------------------------------------------------------------- #
# fetching
# --------------------------------------------------------------------------- #

def rehearsal_tag(run_id, repository):
    """The build of a run started by hand on main, and the label it carries."""
    done = subprocess.run(["gh", "run", "view", str(run_id), "--repo", repository, "--json",
                           "event,headBranch,conclusion,workflowName"], capture_output=True, text=True)
    if done.returncode != 0:
        raise Refused(["gh cannot read run %s:\n%s" % (run_id, done.stderr.strip())])
    view = json.loads(done.stdout)
    if (view.get("event"), view.get("headBranch"), view.get("conclusion"), view.get("workflowName")) != \
            ("workflow_dispatch", "main", "success", "Release"):
        raise Refused(["run %s is not a successful run of Release started by hand on main: %s" % (run_id, view)])
    names = subprocess.run(["gh", "api", "repos/%s/actions/runs/%s/artifacts" % (repository, run_id),
                            "--jq", ".artifacts[].name"], capture_output=True, text=True).stdout.split()
    builds = [n for n in names if n.startswith("unsigned-build-")]
    if len(builds) != 1:
        raise Refused(["run %s hands over %s, and phase B needs one build" % (run_id, builds)])
    return builds[0][len("unsigned-build-"):]


def fetch(directory, tag, run_id, repository):
    if os.path.isdir(directory):
        shutil.rmtree(directory)
    os.makedirs(directory)
    command = ["gh", "run", "download"] + ([str(run_id)] if run_id else []) + [
        "--repo", repository, "--name", "unsigned-build-%s" % tag, "--dir", directory]
    run(command)


# --------------------------------------------------------------------------- #
# the command line
# --------------------------------------------------------------------------- #

def attest(tag, directory, repository, wait):
    """Steps 7 and 8: phase C makes its statement, and the draft is checked to be
    the whole release."""
    say("\n[7/8] asking %s for the statement about the signed files" % ATTEST_WORKFLOW)
    earlier_id = statement_id(draft_view(tag, repository), tag)
    earlier_runs = ask_for_the_statement(tag, sha256_of(os.path.join(directory, SUMS)), repository)
    say("\n[8/8] waiting for it, then checking the draft is the whole release")
    view = wait_for_the_statement(tag, repository, earlier_runs, earlier_id, limit=wait)
    confirm_draft(view, tag, directory, repository)
    say("\nDone. %s is a complete draft, and nothing is published. Read it, then publish it on the "
        "Releases page. verify-release.yml then checks the published page the way a person would." % tag)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("tag", nargs="?", help="the tag of the release, such as v0.1.0")
    parser.add_argument("--rehearse", metavar="RUN_ID",
                        help="sign the build of a Release run started by hand on main, and stop before uploading")
    parser.add_argument("--attest-only", action="store_true",
                        help="the files are signed and on the draft already: only ask for the statement again")
    parser.add_argument("--macos-host", default=os.environ.get("NKB_MACOS_HOST"),
                        help="user@host of the Mac that signs the bundles, or NKB_MACOS_HOST")
    parser.add_argument("--macos-python", default=MAC_PYTHON,
                        help="a Python 3.11 or newer on the Mac (default %s)" % MAC_PYTHON)
    parser.add_argument("--wait", type=int, default=WAIT_FOR_STATEMENT,
                        help="seconds to wait for the statement (default %d)" % WAIT_FOR_STATEMENT)
    parser.add_argument("--dry-run", action="store_true",
                        help="everything except signing, notarising and uploading")
    args = parser.parse_args(argv)
    try:
        if bool(args.tag) == bool(args.rehearse):
            raise Refused(["give a tag, or --rehearse with the id of a run, and not both"])
        repository = release.repository_name(contents.load_workspace())
        if args.attest_only:
            if not args.tag or args.dry_run:
                raise Refused(["--attest-only asks again for the statement about a release already on its "
                               "draft, so it takes a tag, and there is nothing to try dry"])
            directory = os.path.join(ROOT, "target", "signing", args.tag)
            problems = signed_problems(draft_view(args.tag, repository), args.tag, directory)
            if problems:
                raise Refused(problems)
            attest(args.tag, directory, repository, args.wait)
            return 0
        if not sys.platform.startswith("win"):
            raise Refused(["the card is read on Windows, so this runs there"])
        if not args.macos_host:
            raise Refused(["no Mac to sign the macOS bundles on: pass --macos-host user@host or set "
                           "NKB_MACOS_HOST. Without the bundles signed and stapled, macOS refuses "
                           "the two archives a person downloads."])
        tag = args.tag or rehearsal_tag(args.rehearse, repository)
        contents.version_of_tag(tag, contents.load_workspace())
        pins = load_pins()
        today = datetime.date.today()

        say("\n[0/8] before anything is downloaded")
        say("  Windows: " + expiry_notice(pins.windows.expires, today))
        say("  macOS:   " + expiry_notice(pins.macos.expires, today))
        thumbprint = thumbprint_for(json.loads(powershell(STORE_SCRIPT, {"NKB_OID": CODE_SIGNING_OID}) or "[]"),
                                    pins.windows.sha256)
        signtool = find_signtool()
        say("  the card's certificate is in the store, and signtool is %s" % signtool)
        run(["ssh", "-o", "BatchMode=yes", args.macos_host, "%s -c %s" % (
            shlex.quote(args.macos_python), shlex.quote("import sys, tomllib\nprint(sys.version.split()[0])"))])
        with open(MAC_SCRIPT, "rb") as handle:
            run(["ssh", "-o", "BatchMode=yes", args.macos_host, "%s - --check %s" % (
                shlex.quote(args.macos_python), pins.macos.sha256)], stdin=handle)

        directory = os.path.join(ROOT, "target", "signing", tag)
        say("\n[1/8] fetching the build of %s" % tag)
        fetch(directory, tag, args.rehearse, repository)
        say("\n[2/8] checking it before touching it")
        check_build(directory, tag, bool(args.rehearse), repository)
        say("\n[3/8] signing the Windows programs with the card")
        sign_windows(directory, tag, pins, thumbprint, signtool, args.dry_run)
        say("\n[4/8] signing, notarising and stapling the macOS bundles on %s" % args.macos_host)
        sign_macos(directory, tag, pins, args.macos_host, args.macos_python, args.dry_run)
        if args.dry_run:
            say("\nDry run: nothing was signed, notarised or uploaded.")
            return 0
        say("\n[5/8] the files for the page")
        name_for_publication(directory, tag)
        write_checksums(directory, tag)
        if args.rehearse:
            say("\nRehearsal: signed, and stopped before uploading. The files are in %s." % directory)
            return 0
        say("\n[6/8] uploading to the draft")
        upload(directory, tag, repository)
        attest(tag, directory, repository, args.wait)
        return 0
    except Refused as refusal:
        print("sign_release: refused:", file=sys.stderr)
        for problem in refusal.problems:
            print("  - " + problem.replace("\n", "\n    "), file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())

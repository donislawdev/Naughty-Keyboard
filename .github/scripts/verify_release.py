#!/usr/bin/env python3
"""Phase D of the release: check the published page the way a person would.

    python3 .github/scripts/verify_release.py plan --tag v0.1.0
    python3 .github/scripts/verify_release.py page --tag v0.1.0 --dir published
    python3 .github/scripts/verify_release.py fetch --tag v0.1.0 --os windows --dir published
    python3 .github/scripts/verify_release.py programs --tag v0.1.0 --os windows --unpacked unpacked

Run by .github/workflows/verify-release.yml when a release is published, and by
hand with a tag to ask whether an older release still verifies.

The three phases before this one check a build, a draft and a digest handed
between them. None of them looks at the published page, which is the only thing
a person ever sees, and that is the gap this fills: it downloads what a person
downloads and does what a person is told to do, with the commands of README.md
run word for word rather than written again here.

Every check is asked, and every problem is listed, before the answer is given:
a person reads a release page once and wants the whole list of what is wrong
with it, not the first line. It only reads. A verifier that can publish is not
a verifier any more.
"""
import argparse
import base64
import json
import os
import shlex
import subprocess
import sys

import contents
import release
import sign_macos
import sign_release
from contents import ROOT, Refused

# Who makes each statement, as the commands of a person can be told to ask.
BUILT_BY = "release.yml"
ATTESTED_BY = "attest-release.yml"
# The oldest glibc README.md promises the Linux programs start on.
GLIBC = "2.35"


def say(text):
    print(text, flush=True)


def gh_json(arguments, what, run=subprocess.run):
    done = release.gh(arguments, run)
    if done.returncode != 0:
        raise Refused(["gh could not read %s:\n%s" % (what, done.stderr.strip())])
    return json.loads(done.stdout or "null")


def ran(command, run, cwd=None):
    """None when a command passed, and what it said when it did not."""
    done = release.capture(command, run, cwd)
    if done.returncode == 0:
        return None
    return "%s\n%s" % (" ".join(command), (done.stdout + done.stderr).strip())


def statement_of(bundle):
    with open(bundle, encoding="utf-8") as handle:
        envelope = json.load(handle)["dsseEnvelope"]
    return json.loads(base64.b64decode(envelope["payload"]))


def digests_of(directory, names):
    return {n: contents.sha256_of(os.path.join(directory, n)) for n in names}


# --------------------------------------------------------------------------- #
# the page
# --------------------------------------------------------------------------- #

def release_problems(view, tag):
    """The release itself: published, immutable, and a candidate marked as one."""
    problems = []
    if view.get("isDraft"):
        problems.append("%s is still a draft, so nobody can download it" % tag)
    if not view.get("isImmutable"):
        problems.append("%s is not immutable, so a file on it could still be replaced" % tag)
    if "-" in tag and not view.get("isPrerelease"):
        problems.append("%s is a release candidate and is not marked as a pre-release, so it can be "
                        "offered as the latest release" % tag)
    names = sorted(a.get("name") for a in view.get("assets", []))
    if names != release.page_files(tag):
        problems.append("the page holds %s, and a release of %s holds %s" % (names, tag, release.page_files(tag)))
    return problems


def files_problems(view, directory, tag):
    """What was downloaded is what the page says it holds, and what the
    checksums list."""
    problems = []
    found = sorted(os.listdir(directory))
    if found != release.page_files(tag):
        return ["the download of %s holds %s" % (tag, found)]
    digests = digests_of(directory, found)
    for asset in view.get("assets", []):
        if asset.get("digest") != "sha256:" + digests.get(asset.get("name"), ""):
            problems.append("%s downloaded is not the file GitHub lists for it" % asset.get("name"))
    with open(os.path.join(directory, release.SUMS), encoding="utf-8") as handle:
        listed = release.listed_digests(handle.read(), release.SUMS)
    if sorted(listed) != [n for n in release.published_files(tag) if n != release.SUMS]:
        problems.append("%s lists %s" % (release.SUMS, sorted(listed)))
    for name, digest in sorted(listed.items()):
        if digests.get(name) != digest:
            problems.append("%s is not the file %s lists" % (name, release.SUMS))
    return problems


def immutable_problems(answer, directory, tag):
    """GitHub's own statement of what this release published, made when it was
    published, against what was downloaded."""
    subjects = answer.get("verificationResult", {}).get("statement", {}).get("subject", [])
    named = {s["name"]: s.get("digest", {}).get("sha256") for s in subjects if "name" in s}
    if named != digests_of(directory, release.page_files(tag)):
        return ["GitHub's statement of what %s published, %s, is not what was downloaded" % (tag, sorted(named))]
    return []


def provenance_problems(directory, tag):
    """The statement of the unsigned build describes, byte for byte, the files
    nothing signed - the Linux archives and the bill of materials - and none of
    the signed ones. A Windows or macOS archive it still describes is an
    unsigned build on the page."""
    version = tag[1:]
    subjects = {s.get("name"): s.get("digest", {}).get("sha256") for s in
                statement_of(os.path.join(directory, release.provenance_bundle_name(version)))["subject"]}
    problems = []
    for archive in contents.load_archives():
        name = archive.name(version)
        same = subjects.get(name) == contents.sha256_of(os.path.join(directory, name))
        if archive.os == "linux" and not same:
            problems.append("%s is not the file the build made" % name)
        if archive.os != "linux" and same:
            problems.append("%s is the file the build made, so it was never signed" % name)
    if subjects.get(release.sbom_name(version)) != contents.sha256_of(os.path.join(directory, release.sbom_name(version))):
        problems.append("%s is not the bill of materials the build made" % release.sbom_name(version))
    return problems


def unsigned_files(tag):
    version = tag[1:]
    return sorted([a.name(version) for a in contents.load_archives() if a.os == "linux"] + [release.sbom_name(version)])


def statement_problems(directory, tag, promise, repository, run):
    """Every archive against the statement of what it holds, offline and through
    GitHub, and every file nothing signed against the statement of how it was
    built, each held to the workflow that made it, the tag, and a runner GitHub
    hosts."""
    version = tag[1:]
    ref = "refs/tags/%s" % tag
    sbom = release.sbom_bundle_name(version)
    provenance = release.provenance_bundle_name(version)
    checks = []
    for archive in contents.load_archives():
        name = archive.name(version)
        checks.append(release.attestation_check(name, repository, promise.predicate, sbom))
        checks.append(release.attestation_check(name, repository, promise.predicate, workflow=ATTESTED_BY, ref=ref))
    for name in unsigned_files(tag):
        checks.append(release.attestation_check(name, repository, bundle=provenance, workflow=BUILT_BY, ref=ref))
    return ["a statement does not answer: %s" % failed for failed in (ran(c, run, directory) for c in checks) if failed]


def command_problems(directory, tag, promise, run):
    """The commands of README.md, word for word, for this version."""
    problems = []
    for line in promise.rendered(tag):
        say("  $ " + line)
        failed = ran(shlex.split(line), run, directory)
        if failed:
            problems.append("the command README.md gives fails: %s" % failed)
    return problems


def notes_problems(body, tag, promise):
    """The page tells a person what was just checked. Only the commands and the
    name of the checksums are asked for, so a sentence added to the notes later,
    such as the one that withdraws a release, changes nothing here."""
    text = body or ""
    lines = [line.strip() for line in text.splitlines()]
    missing = [line for line in promise.rendered(tag) if line not in lines]
    if release.SUMS not in text:
        missing.append(release.SUMS)
    return ["the notes of %s do not say: %s" % (tag, line) for line in missing]


def latest_problems(view, tag, latest):
    """The download button aims at the latest release, so a full release has to
    be it. A pre-release never is, and that includes a withdrawn one."""
    if view.get("isPrerelease"):
        say("  %s is a pre-release, so it is not expected to be the latest" % tag)
        return [] if latest != tag else ["%s is a pre-release and is offered as the latest" % tag]
    return [] if latest == tag else ["the latest release is %s, and this one is %s" % (latest, tag)]


def command_page(args, run=subprocess.run):
    promise = release.load_promise()
    repository = release.repository_name(contents.load_workspace())
    view = gh_json(["release", "view", args.tag, "--json", "assets,body,isDraft,isImmutable,isPrerelease"],
                   "the release %s" % args.tag, run)
    problems = release_problems(view, args.tag)
    say("downloading every file of %s" % args.tag)
    done = release.gh(["release", "download", args.tag, "--dir", args.dir], run)
    if done.returncode != 0:
        raise Refused(problems + ["gh could not download %s:\n%s" % (args.tag, done.stderr.strip())])
    problems += files_problems(view, args.dir, args.tag)
    if not problems:
        answer = gh_json(["release", "verify", args.tag, "--format", "json"], "GitHub's statement", run)
        problems += immutable_problems(answer, args.dir, args.tag)
        problems += provenance_problems(args.dir, args.tag)
        say("the commands README.md gives, word for word")
        problems += command_problems(args.dir, args.tag, promise, run)
        say("every statement, held to who made it")
        problems += statement_problems(args.dir, args.tag, promise, repository, run)
    problems += notes_problems(view.get("body"), args.tag, promise)
    latest = release.gh(["api", "repos/%s/releases/latest" % repository, "--jq", ".tag_name"], run)
    problems += latest_problems(view, args.tag, latest.stdout.strip() if latest.returncode == 0 else "")
    return verdict("the page of %s" % args.tag, problems)


# --------------------------------------------------------------------------- #
# the programs of one system
# --------------------------------------------------------------------------- #

def command_fetch(args, run=subprocess.run):
    """The archives of one system and the checksums, by their exact names."""
    version = contents.version_of_tag(args.tag, contents.load_workspace())
    names = sorted(a.name(version) for a in release.archives_for(args.os))
    patterns = [word for name in names + [release.SUMS] for word in ("--pattern", name)]
    done = release.gh(["release", "download", args.tag, "--dir", args.dir] + patterns, run)
    if done.returncode != 0:
        raise Refused(["gh could not download %s:\n%s" % (args.tag, done.stderr.strip())])
    with open(os.path.join(args.dir, release.SUMS), encoding="utf-8") as handle:
        listed = release.listed_digests(handle.read(), release.SUMS)
    problems = ["%s is not the file %s lists" % (n, release.SUMS) for n in names
                if listed.get(n) != contents.sha256_of(os.path.join(args.dir, n))]
    if problems:
        raise Refused(problems)
    say("downloaded %s, each the file %s lists" % (", ".join(names), release.SUMS))
    return 0


def windows_problems(program, pin, run):
    """Valid, by the pinned certificate, with a timestamp: read by Windows from
    the file a person downloads."""
    result = json.loads(sign_release.powershell(sign_release.SIGNATURE_SCRIPT, {"NKB_SIGNED_FILE": program}, run))
    return sign_release.signature_problems(result, pin, os.path.basename(program))


def macos_problems(app, pin, run):
    """What a Mac asks of a bundle, with the hardened runtime and a timestamp
    that notarisation needs, by the pinned certificate. Gatekeeper is asked only
    when it is on, because when it is off it accepts everything."""
    name = os.path.basename(app)
    problems = [failed for failed in (ran(["codesign", "--verify", "--strict", "--deep", app], run),
                                      ran(["xcrun", "stapler", "validate", app], run)) if failed]
    shown = release.capture(["codesign", "-d", "--verbose=2", app], run)
    said = shown.stdout + shown.stderr
    if "(runtime)" not in said:
        problems.append("%s is not signed with the hardened runtime" % name)
    if "Timestamp=" not in said:
        problems.append("%s carries no timestamp" % name)
    actual = sign_macos.certificate_of(app, run)
    if actual != pin:
        problems.append("%s was signed by a DIFFERENT certificate\n  expected %s\n  got      %s" % (name, pin, actual))
    if "assessments enabled" in release.capture(["spctl", "--status"], run).stdout:
        failed = ran(["spctl", "--assess", "--type", "exec", "-vv", app], run)
        if failed:
            problems.append("Gatekeeper rejects %s: %s" % (name, failed))
    else:
        say("  Gatekeeper is off on this machine, so it was not asked about %s" % name)
    return problems


def glibc_problems(program, run):
    needed = release.glibc_needed(program, run)
    if needed is None:
        return ["objdump could not say which glibc %s needs" % os.path.basename(program)]
    if tuple(int(n) for n in needed.split(".")) > tuple(int(n) for n in GLIBC.split(".")):
        return ["%s needs glibc %s, and README.md promises %s" % (os.path.basename(program), needed, GLIBC)]
    return []


def command_programs(args, run=subprocess.run):
    workspace = contents.load_workspace()
    version = contents.version_of_tag(args.tag, workspace)
    pins = sign_release.load_pins()
    packs = sorted(os.path.join(ROOT, "packs", n) for n in os.listdir(os.path.join(ROOT, "packs"))
                   if n.endswith(".toml"))
    problems = []
    for archive in release.archives_for(args.os):
        directory = os.path.join(args.unpacked, archive.name(version))
        program = os.path.join(directory, release.program_file(archive))
        found = release.tried_programs(archive, workspace.version, directory, packs, run, signed=archive.os != "linux")
        if archive.os == "windows":
            found += windows_problems(program, pins.windows.sha256, run)
        elif archive.os == contents.MACOS:
            found += macos_problems(program + ".app", pins.macos.sha256, run)
        else:
            found += glibc_problems(program, run)
        problems += ["%s: %s" % (archive.name(version), p) for p in found]
        if not found:
            done = "unpacked and run" if archive.package == "nkb-cli" else "unpacked"
            more = {"windows": "signed by the pinned certificate, with a timestamp",
                    contents.MACOS: "signed by the pinned certificate, notarised and stapled"}.get(
                        archive.os, "needs glibc %s or older" % GLIBC)
            say("%s: %s, %s" % (archive.name(version), done, more))
    return verdict("the %s programs of %s" % (args.os, args.tag), problems)


# --------------------------------------------------------------------------- #
# the command line
# --------------------------------------------------------------------------- #

def verdict(what, problems):
    if problems:
        raise Refused(["%d thing(s) wrong with %s, every one listed:" % (len(problems), what)] + problems)
    say("%s: every check passed" % what)
    return 0


def command_plan(args):
    contents.version_of_tag(args.tag, contents.load_workspace())
    platforms = json.dumps(release.platforms_of(contents.load_archives()), separators=(",", ":"))
    if args.output:
        release.write_outputs({"tag": args.tag, "platforms": platforms}, args.output)
    say("checking the published %s" % args.tag)
    return 0


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    commands = parser.add_subparsers(dest="command", required=True)
    plan = commands.add_parser("plan", help="which release, and on which systems")
    plan.add_argument("--tag", required=True)
    plan.add_argument("--output", default=os.environ.get("GITHUB_OUTPUT"))
    page = commands.add_parser("page", help="check the page and every statement on it")
    page.add_argument("--tag", required=True)
    page.add_argument("--dir", required=True)
    fetch = commands.add_parser("fetch", help="download the archives of one system")
    fetch.add_argument("--tag", required=True)
    fetch.add_argument("--os", required=True)
    fetch.add_argument("--dir", required=True)
    programs = commands.add_parser("programs", help="run and check the unpacked programs of one system")
    programs.add_argument("--tag", required=True)
    programs.add_argument("--os", required=True)
    programs.add_argument("--unpacked", required=True)
    args = parser.parse_args(argv)
    try:
        return {"plan": command_plan, "page": command_page, "fetch": command_fetch,
                "programs": command_programs}[args.command](args)
    except Refused as refusal:
        print("verify_release: %s refused:" % args.command, file=sys.stderr)
        for problem in refusal.problems:
            print("  - " + problem.replace("\n", "\n    "), file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())

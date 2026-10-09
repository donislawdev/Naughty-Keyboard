#!/usr/bin/env python3
"""Phase C of the release: the statement of what the signed files hold.

    python3 .github/scripts/attest_release.py fetch --tag v0.1.0 --digest <sha256> --dir signed --subjects subjects.sha256
    python3 .github/scripts/attest_release.py publish --tag v0.1.0 --dir signed --bundle <path> --out statement

Run by .github/workflows/attest-release.yml, which sign_release.py starts once
the signed files are on the draft, with the digest of the checksums it wrote.

The files come FROM THE DRAFT, not from the digest that started the run. A
statement about a number somebody handed over is a statement about the number,
and the number and the files are the same thing only if nothing went wrong in
between, which is what is being asked. The digest stays as a cross-check: if
the list on the draft is not the list that was signed, the run stops.

What is attested is the bill of materials, against the six archives as they
are on the draft. Never build provenance: that says a workflow made these
bytes, and a person made them, on the machines that hold the keys. The
statement of how the unsigned build was made is phase A's, and it still answers
for every file nothing signed.

The kind of statement this makes has to be the kind README.md tells a person to
ask for. gh asks for build provenance unless told otherwise, so the commands
there carry --predicate-type, and a statement of another type would answer them
"no attestation found", which reads like a broken release rather than a wrong
flag. So the type is read out of the statement just made and compared with the
commands, and every archive is checked with them before anything is uploaded.
"""
import argparse
import base64
import json
import os
import re
import shutil
import subprocess
import sys

import contents
import release
from contents import Refused


def say(text):
    print(text, flush=True)


def checked_ref(tag, environ):
    """The run has to be on the tag it attests, so that the workflow making the
    statement, and the README.md it compares with, are the ones this release
    carries."""
    ref = environ.get("GITHUB_REF", "")
    if ref != "refs/tags/%s" % tag:
        raise Refused(["this run is on %r and attests %s. Start it from the tag: gh workflow run "
                       "attest-release.yml --ref %s -f tag=%s -f digest=..." % (ref, tag, tag, tag)])


def fetched_problems(directory, tag, digest):
    """What is wrong with the files fetched from the draft, as one list."""
    found = sorted(os.listdir(directory))
    wanted = release.published_files(tag)
    if found != wanted:
        return ["the draft holds %s, and phase B uploads %s" % (found, wanted)]
    sums = os.path.join(directory, release.SUMS)
    actual = contents.sha256_of(sums)
    if actual != digest.lower():
        return ["%s on the draft hashes to %s, and this run was started for %s. Something changed it "
                "between signing and now." % (release.SUMS, actual, digest)]
    with open(sums, encoding="utf-8") as handle:
        listed = release.listed_digests(handle.read(), release.SUMS)
    problems = []
    if sorted(listed) != [n for n in wanted if n != release.SUMS]:
        problems.append("%s lists %s" % (release.SUMS, sorted(listed)))
    for name, expected in sorted(listed.items()):
        if name in found and contents.sha256_of(os.path.join(directory, name)) != expected:
            problems.append("%s on the draft is not the file %s lists" % (name, release.SUMS))
    return problems


def subjects_of(directory, tag):
    """The six archives as sha256sum writes them: what the statement is about.
    Not the files whose names start with verify-, which are about the archives."""
    version = tag[1:]
    names = sorted(a.name(version) for a in contents.load_archives())
    return "".join("%s  %s\n" % (contents.sha256_of(os.path.join(directory, n)), n) for n in names)


def command_fetch(args, environ=os.environ, run=subprocess.run):
    if not re.fullmatch(r"[0-9a-fA-F]{64}", args.digest):
        raise Refused(["%r is not a SHA-256" % args.digest])
    checked_ref(args.tag, environ)
    if release.release_state(args.tag, run) != "draft":
        raise Refused(["%s is not a draft. A published release cannot be given a file, so phase C "
                       "runs before publishing." % args.tag])
    if os.path.isdir(args.dir):
        shutil.rmtree(args.dir)
    done = release.gh(["release", "download", args.tag, "--dir", args.dir], run)
    if done.returncode != 0:
        raise Refused(["gh could not download the draft of %s:\n%s" % (args.tag, done.stderr.strip())])
    # A statement an earlier run left on the draft describes bytes it was given
    # then. This run makes its own and replaces it.
    earlier = os.path.join(args.dir, release.sbom_bundle_name(args.tag[1:]))
    if os.path.isfile(earlier):
        os.remove(earlier)
        say("the draft holds a statement from an earlier run, which this one replaces")
    problems = fetched_problems(args.dir, args.tag, args.digest)
    if problems:
        raise Refused(problems)
    contents.write(args.subjects, subjects_of(args.dir, args.tag))
    sbom = os.path.join(args.dir, release.sbom_name(args.tag[1:]))
    if args.output:
        release.write_outputs({"sbom": sbom}, args.output)
    say("the draft of %s holds the %d files phase B uploaded, each the one %s lists"
        % (args.tag, len(release.published_files(args.tag)), release.SUMS))
    return 0


def statement_of(bundle):
    """The in-toto statement inside a Sigstore bundle."""
    with open(bundle, encoding="utf-8") as handle:
        envelope = json.load(handle)["dsseEnvelope"]
    return json.loads(base64.b64decode(envelope["payload"]))


def statement_problems(statement, promise, subjects):
    problems = []
    if statement.get("predicateType") != promise.predicate:
        problems.append("the statement is of type %s, and README.md tells a person to ask for %s. As "
                        "written, the commands would answer that there is no attestation."
                        % (statement.get("predicateType"), promise.predicate))
    named = {s.get("name"): s.get("digest", {}).get("sha256") for s in statement.get("subject", [])}
    if named != subjects:
        problems.append("the statement is about %s, and the signed archives are %s" % (named, subjects))
    return problems


def command_publish(args, environ=os.environ, run=subprocess.run):
    checked_ref(args.tag, environ)
    promise = release.load_promise()
    repository = release.repository_name(contents.load_workspace())
    subjects = release.listed_digests(subjects_of(args.dir, args.tag), "the subjects")
    problems = statement_problems(statement_of(args.bundle), promise, subjects)
    for name in sorted(subjects):
        check = release.attestation_check(os.path.join(args.dir, name), repository, promise.predicate, args.bundle)
        done = release.capture(check, run)
        if done.returncode != 0:
            problems.append("%s does not verify with the command README.md gives: %s\n%s"
                            % (name, " ".join(check), (done.stdout + done.stderr).strip()))
    if problems:
        raise Refused(problems)
    if release.release_state(args.tag, run) != "draft":
        raise Refused(["%s is no longer a draft, so the statement cannot be added to it" % args.tag])
    os.makedirs(args.out, exist_ok=True)
    named = os.path.join(args.out, release.sbom_bundle_name(args.tag[1:]))
    shutil.copyfile(args.bundle, named)
    done = release.gh(["release", "upload", args.tag, named, "--clobber"], run)
    if done.returncode != 0:
        raise Refused(["gh could not add the statement to the draft of %s:\n%s" % (args.tag, done.stderr.strip())])
    say("every archive verifies against the statement, which is of the type README.md asks for, and "
        "%s is on the draft of %s" % (os.path.basename(named), args.tag))
    return 0


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    commands = parser.add_subparsers(dest="command", required=True)
    fetch = commands.add_parser("fetch", help="fetch the draft and check it against the digest")
    fetch.add_argument("--tag", required=True)
    fetch.add_argument("--digest", required=True)
    fetch.add_argument("--dir", required=True)
    fetch.add_argument("--subjects", required=True)
    fetch.add_argument("--output", default=os.environ.get("GITHUB_OUTPUT"))
    publish = commands.add_parser("publish", help="check the statement and add it to the draft")
    publish.add_argument("--tag", required=True)
    publish.add_argument("--dir", required=True)
    publish.add_argument("--bundle", required=True)
    publish.add_argument("--out", required=True)
    args = parser.parse_args(argv)
    try:
        return {"fetch": command_fetch, "publish": command_publish}[args.command](args)
    except Refused as refusal:
        print("attest_release: refused:", file=sys.stderr)
        for problem in refusal.problems:
            print("  - " + problem.replace("\n", "\n    "), file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())

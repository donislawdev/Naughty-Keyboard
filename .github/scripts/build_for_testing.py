#!/usr/bin/env python3
"""A test build: the programs of one commit, built the way a release builds
them, for a person who reported a problem to try the fix on before it is
released. What .github/workflows/test-build.yml asks of this repository.

    python3 .github/scripts/build_for_testing.py plan --run-number 7 --system windows
    python3 .github/scripts/build_for_testing.py tested --sha <commit> --ref <branch>
    python3 .github/scripts/build_for_testing.py links --tag v0.1.0-test.7 --system windows --run-id <id> --sha <commit>

What it is and what it is not
-----------------------------
The archives are built, packed, unpacked and run by the steps of release.py a
release takes, so a test build differs from a release only in what happens
after: nothing signs it, nothing attests it, and nothing puts it on the
release page. It stays an artifact of its run, behind links that need a GitHub
account, for as long as the workflow keeps it.

Its version is the workspace version with `-test.<run number>`. Not the `-dev`
that release.yml gives a run by hand: each workflow counts its own runs, so
the two would give two different builds one name. The programs still print
the workspace version, because nothing tells them another one, and the name of
the archive is what says which test build a person has.

The links are written once CI passed on the commit. Any run of CI on it
counts, whatever started it: on a pull request CI tested the commit merged
into main, on main or by hand the commit itself. The newest run decides.
"""
import argparse
import dataclasses
import json
import os
import re
import subprocess
import sys
import time

import contents
import release
from contents import Refused

CI_WORKFLOW = "ci.yml"
SUFFIX = "test"
ALL = "all"
# CI on a pull request takes about six minutes from cold and a release waits up
# to this long for its workflows, so a test build waits the same.
WAIT_SECONDS = release.WAIT_SECONDS
# A branch pushed a moment ago may not have its run of CI listed yet. One that
# has no run after this long never will, because nothing started one.
GRACE_SECONDS = 5 * 60
POLL_SECONDS = release.POLL_SECONDS
SERVER = "https://github.com"


def say(text):
    print(text, flush=True)


# --------------------------------------------------------------------------- #
# what is being built
# --------------------------------------------------------------------------- #

def tag_of(run_number, workspace):
    """The tag a test build is named by. No release carries it: a release is a
    pushed tag, and nothing here pushes one."""
    if not re.fullmatch(r"\d+", str(run_number)):
        raise Refused(["the run number %r is not a number" % run_number])
    tag = "v%s-%s.%s" % (workspace.version, SUFFIX, run_number)
    contents.version_of_tag(tag, workspace)
    return tag


def systems_of(archives):
    """The words the form of the workflow offers, in the order archives.toml
    lists the platforms."""
    return [ALL] + [platform["os"] for platform in release.platforms_of(archives)]


def chosen(system, archives):
    """The archives of the system asked for, or of every system."""
    if system == ALL:
        return list(archives)
    found = [a for a in archives if a.os == system]
    if not found:
        raise Refused(["%r is not a system archives.toml lists. Choose one of: %s"
                       % (system, ", ".join(systems_of(archives)))])
    return found


def command_plan(args):
    workspace = contents.load_workspace()
    archives = chosen(args.system, contents.load_archives())
    tag = tag_of(args.run_number, workspace)
    version = tag[1:]
    outputs = {
        "tag": tag,
        "version": version,
        "platforms": json.dumps(release.platforms_of(archives), separators=(",", ":")),
        "archives": json.dumps([{"os": a.os, "name": a.name(version)} for a in archives],
                               separators=(",", ":")),
    }
    for key, value in outputs.items():
        say("%s = %s" % (key, value))
    if args.output:
        release.write_outputs(outputs, args.output)
    return 0


# --------------------------------------------------------------------------- #
# CI passed on the commit
# --------------------------------------------------------------------------- #

def ci_verdict(runs):
    """(done, answer, address) of CI on one commit. The answer is None while no
    run is listed at all, which is a different thing from a run that failed."""
    if not runs:
        return False, None, ""
    newest = max(runs, key=lambda r: r.get("createdAt", ""))
    address = newest.get("url", "")
    if newest.get("status") != "completed":
        return False, "still %s" % newest.get("status"), address
    return True, newest.get("conclusion") or "no conclusion", address


def wait_for_ci(sha, ref="", run=subprocess.run, sleep=time.sleep, clock=time.monotonic,
                limit=WAIT_SECONDS, grace=GRACE_SECONDS):
    """The address of the run of CI that passed on this commit, waiting while it
    runs. Refused when it failed, when it is still running at the limit, and
    when it never ran."""
    started = clock()
    while True:
        done, answer, address = ci_verdict(release.runs_of(CI_WORKFLOW, sha, run))
        if done:
            break
        waited = clock() - started
        if answer is None:
            say("  %s has no run on %s yet" % (CI_WORKFLOW, sha))
            if waited > grace:
                raise Refused(["%s never ran on %s, and a test build is shared once CI passed on its "
                               "commit. Open a pull request from the branch, or start CI on it with "
                               "gh workflow run %s --ref %s, then run this again."
                               % (CI_WORKFLOW, sha, CI_WORKFLOW, ref or "<branch>")])
        else:
            say("  %s %s: %s" % (CI_WORKFLOW, answer, address))
            if waited > limit:
                raise Refused(["%s is still running on %s after %d minutes. Re-run this job when it ends."
                               % (CI_WORKFLOW, sha, limit // 60)])
        sleep(POLL_SECONDS)
    if answer != "success":
        raise Refused(["a test build is shared once CI passed on its commit, and on %s %s says %s: %s"
                       % (sha, CI_WORKFLOW, answer, address)])
    return address


def command_tested(args):
    address = wait_for_ci(args.sha, args.ref)
    say("%s passed on %s: %s" % (CI_WORKFLOW, args.sha, address))
    if args.output:
        release.write_outputs({"ci-run": address}, args.output)
    return 0


# --------------------------------------------------------------------------- #
# the links, and the words to send with them
# --------------------------------------------------------------------------- #

@dataclasses.dataclass(frozen=True)
class Link:
    name: str
    os: str
    address: str
    size: int
    digest: str      # the SHA-256 GitHub keeps of the file, without its sha256: prefix
    until: str       # the day the link stops working, YYYY-MM-DD


def listed_artifacts(repository, run_id, run=subprocess.run):
    """Every artifact of this run, in one page. A run that holds more than one
    page lists is refused rather than read in part."""
    done = release.gh(["api", "repos/%s/actions/runs/%s/artifacts?per_page=100" % (repository, run_id)], run)
    if done.returncode != 0:
        raise Refused(["gh could not list the artifacts of run %s:\n%s" % (run_id, done.stderr.strip())])
    page = json.loads(done.stdout or "{}")
    artifacts = page.get("artifacts", [])
    if page.get("total_count", len(artifacts)) != len(artifacts):
        raise Refused(["run %s holds %s artifacts and one page lists %d"
                       % (run_id, page.get("total_count"), len(artifacts))])
    return artifacts


def links_of(wanted, artifacts, repository, run_id, server=SERVER):
    """(links, missing): one link for each archive the run was to share, in the
    order wanted lists them, and the names of those it does not hold."""
    held = {a.get("name"): a for a in artifacts if not a.get("expired")}
    links, missing = [], []
    for archive_os, name in wanted:
        artifact = held.get(name)
        if artifact is None:
            missing.append(name)
            continue
        digest = artifact.get("digest") or ""
        links.append(Link(name, archive_os,
                          "%s/%s/actions/runs/%s/artifacts/%s" % (server, repository, run_id, artifact["id"]),
                          int(artifact.get("size_in_bytes") or 0),
                          digest.split(":", 1)[-1] if digest.startswith("sha256:") else "",
                          (artifact.get("expires_at") or "")[:10]))
    return links, missing


def megabytes(size):
    return "%.1f MB" % (size / 1e6)


# What to do with an unsigned download, per system, in the order archives.toml
# lists the platforms. A paragraph is sent only for a system that was built.
UNSIGNED = {
    "windows": ("Windows: unpack the zip. If SmartScreen says \"Windows protected your PC\", choose "
                "\"More info\", then \"Run anyway\". If Smart App Control blocks it, Windows offers no "
                "way to allow this one program, so let me know."),
    "macos": ("macOS: unpack the archive into a folder of its own, then run "
              "xattr -dr com.apple.quarantine on that folder in Terminal. macOS refuses to start a "
              "downloaded program that is not notarised, and this removes the mark the download left "
              "on it."),
    "linux": "Linux: unpack the archive and run the program from where it was unpacked.",
}


def message(links, sha, source):
    """The words for the person who reported the problem, ready to paste."""
    lines = ["Here is a test build of Naughty Keyboard with the fix, made from commit %s:" % sha[:12], ""]
    lines += ["%s: %s" % (link.name, link.address) for link in links]
    until = min(link.until for link in links)
    lines += ["",
              "The links need a GitHub account and work until %s. This is not a release, so it is "
              "not signed." % until,
              ""]
    built = {link.os for link in links}
    lines += [UNSIGNED[name] for name in UNSIGNED if name in built]
    lines += ["", "The source of this build: %s" % source]
    return "\n".join(lines)


def summary(tag, base, sha, ci_run, links, missing, repository, pull_request, server=SERVER):
    """The summary of the run, in Markdown: what was built and from what, the
    links, what is missing, and the words to send."""
    source = "%s/%s/tree/%s" % (server, repository, sha)
    lines = ["## Test build %s" % tag, ""]
    if pull_request:
        lines += ["A run on a pull request, which proves this workflow. Its files are not for sending.", ""]
    lines += ["Built from [%s](%s), after [CI passed](%s) on it. This is not a release: nothing signed it, "
              "and the release page never carries it. The programs print %s when asked their version, "
              "so the name of the archive is what says which test build a person has."
              % (sha[:12], source, ci_run, base),
              ""]
    if links:
        lines += ["| Archive | Size | SHA-256 | Link works until |", "|---|---|---|---|"]
        lines += ["| [%s](%s) | %s | `%s` | %s |" % (link.name, link.address, megabytes(link.size),
                                                     link.digest or "not given", link.until)
                  for link in links]
        lines.append("")
    if missing:
        lines += ["**Not shared**, because its build or its upload failed. The log of its job says why:", ""]
        lines += ["- %s" % name for name in missing]
        lines.append("")
    if links:
        lines += ["### To send", "", "```", message(links, sha, source), "```", ""]
    return "\n".join(lines)


def command_links(args):
    if not args.repo:
        raise Refused(["no repository: give --repo owner/name, or run this where GITHUB_REPOSITORY is set"])
    workspace = contents.load_workspace()
    version = contents.version_of_tag(args.tag, workspace)
    wanted = [(a.os, a.name(version)) for a in chosen(args.system, contents.load_archives())]
    artifacts = listed_artifacts(args.repo, args.run_id)
    links, missing = links_of(wanted, artifacts, args.repo, args.run_id)
    text = summary(args.tag, workspace.version, args.sha, args.ci_run, links, missing, args.repo,
                   args.event == "pull_request")
    say(text)
    if args.summary:
        with open(args.summary, "a", encoding="utf-8", newline="\n") as handle:
            handle.write(text + "\n")
    if missing:
        raise Refused(["the run was to share %d archive(s) and does not hold: %s"
                       % (len(wanted), ", ".join(missing))])
    return 0


# --------------------------------------------------------------------------- #
# the command line
# --------------------------------------------------------------------------- #

def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    commands = parser.add_subparsers(dest="command", required=True)
    plan = commands.add_parser("plan", help="what this run builds, as step outputs")
    plan.add_argument("--run-number", required=True)
    plan.add_argument("--system", required=True)
    plan.add_argument("--output", default=os.environ.get("GITHUB_OUTPUT"))
    tested = commands.add_parser("tested", help="wait until CI passed on this commit")
    tested.add_argument("--sha", required=True)
    tested.add_argument("--ref", default="")
    tested.add_argument("--output", default=os.environ.get("GITHUB_OUTPUT"))
    links = commands.add_parser("links", help="the links of the shared archives and the words to send")
    links.add_argument("--tag", required=True)
    links.add_argument("--system", required=True)
    links.add_argument("--run-id", required=True)
    links.add_argument("--sha", required=True)
    links.add_argument("--ci-run", default="")
    links.add_argument("--event", default="workflow_dispatch")
    links.add_argument("--repo", default=os.environ.get("GITHUB_REPOSITORY"))
    links.add_argument("--summary", default=os.environ.get("GITHUB_STEP_SUMMARY"))
    args = parser.parse_args(argv)
    handlers = {"plan": command_plan, "tested": command_tested, "links": command_links}
    try:
        return handlers[args.command](args)
    except Refused as refusal:
        print("build_for_testing: %s refused:" % args.command, file=sys.stderr)
        for problem in refusal.problems:
            print("  - " + problem.replace("\n", "\n    "), file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())

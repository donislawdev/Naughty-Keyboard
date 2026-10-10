---
title: Verifying a download
seoTitle: Verify a download - checksums, signatures and attestations
description: How to check that a download is the file the release published - checksums, Authenticode and notarised signatures, and signed attestations.
lead: "Every release carries what you need to check a download without trusting this page, or the person who made the release."
---

## What does a release carry for checking?

The four files whose names start with `verify-`, together at the end of the list of files:

- `verify-SHA256SUMS.txt`, the SHA-256 of every archive,
- the bill of materials of both programs in SPDX, ending in `.spdx.json`,
- a signed statement of how the unsigned build was made, ending in `.provenance.sigstore.json`,
- a signed statement of what the signed archives hold, ending in `.sbom.sigstore.json`.

The two statements are Sigstore bundles, so they can be checked offline.

## Which commands check it?

These need the [GitHub CLI](https://cli.github.com/). Put the version in place of `VERSION` and the name of your archive in place of the example one:

{{< verify-commands >}}

- **The first** asks GitHub whether the file is one the release published. It works for every file of a release, because a published release cannot change.
- **The next two** check the statement of what an archive holds, the bill of materials. They work for all six archives, the first through GitHub and the second offline, with nothing but the file next to it. Both need `--predicate-type`: without it, `gh` asks for a statement of how the file was built, a signed archive has none, and the answer "no attestation found" looks like a broken release.
- **The last two** check how an archive was built: by which workflow, from which commit, on which runner. They work for the two Linux archives and the bill of materials, the files nothing signed. A Windows or macOS archive was signed after the build, on the machine that holds the key, so its bytes are not the bytes the build made, and its signature answers for it instead.

These are the same commands the release notes carry and the same ones a workflow runs word for word on every published release.

## What is signed, and with what?

- **Windows.** `nkb.exe` and `nkb-gui.exe` carry an Authenticode signature with an RFC 3161 timestamp, so the signature stays valid after the certificate expires. The certificate is issued by Certum, and its key is on a cryptographic card that cannot give it away.
- **macOS.** `nkb.app` and `nkb-gui.app` are signed with an Apple Developer ID Application certificate, with the hardened runtime and a timestamp, and notarised by Apple. The ticket is stapled into the bundle, so macOS checks it without asking the network.
- **Linux.** The programs are not signed. The statement of how they were built answers for them.

The SHA-256 of both certificates is pinned in the repository. The signing refuses a file signed by any other certificate, and so does the check of every published release:

{{< fingerprints >}}

To compare a signed program with the pin yourself, in PowerShell 7:

```powershell
(Get-AuthenticodeSignature .\nkb.exe).SignerCertificate.GetCertHashString('SHA256')
```

and on macOS:

```console
$ codesign -d --extract-certificates nkb.app
$ shasum -a 256 codesign0
```

On macOS, `spctl -a -vv` and `xcrun stapler validate` on the bundle say whether Gatekeeper accepts it and whether the ticket is stapled.

## Can a release be replaced?

No. Releases of this project are immutable: a published file cannot change, and a release cannot be given a new file. A broken release is marked as a pre-release, which takes it off the latest-release link, with a sentence at the top of its notes, and the fix comes in a new release.

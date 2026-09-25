# Verifying release artifacts

Each release archive has a GitHub keyless provenance attestation. Verification
checks the archive digest and binds it to this repository, the release workflow,
the exact version tag and a GitHub-hosted runner. It does not prove that the
software is bug-free or safe.

The installer always checks the archive against the release's `SHA256SUMS`
before extraction. This detects corruption, but **does not authenticate the
source** if both the archive and checksum manifest are replaced together.

With a capable GitHub CLI (`gh`), both installers verify signed provenance
automatically. Without gh, both report skipped verification. **Standard** also
explicitly skips an older gh lacking the required flags; **Guided** stops on an
installed but incompatible gh. Both accept `--require-provenance`, which makes
missing/incompatible gh a blocking error. Once verification is enabled, a failed
attestation check or unavailable bundle stops installation, never falls back.

To enable provenance verification, install GitHub CLI from a trusted package
source (for example, your distribution) and rerun the installer.
No GitHub account, login or token is required when the matching local attestation
bundle is supplied with `--bundle`.

Download an archive and its adjacent `.sigstore.json` file from the same release.
For example, to verify version `1.1.0`:

```sh
tag=v1.1.0
archive=gnome-clip-notes-1.1.0-x86_64.tar.gz

gh attestation verify "$archive" \
  --bundle "$archive.sigstore.json" \
  --repo OleksiyM/GnomeClipNotes \
  --signer-workflow OleksiyM/GnomeClipNotes/.github/workflows/release.yml \
  --source-ref "refs/tags/$tag" \
  --cert-oidc-issuer https://token.actions.githubusercontent.com \
  --deny-self-hosted-runners \
  --predicate-type https://slsa.dev/provenance/v1
```

The same command verifies `SHA256SUMS` with
`SHA256SUMS.sigstore.json`. After that succeeds, check every downloaded archive
listed in the manifest:

```sh
gh attestation verify SHA256SUMS \
  --bundle SHA256SUMS.sigstore.json \
  --repo OleksiyM/GnomeClipNotes \
  --signer-workflow OleksiyM/GnomeClipNotes/.github/workflows/release.yml \
  --source-ref "refs/tags/v1.1.0" \
  --cert-oidc-issuer https://token.actions.githubusercontent.com \
  --deny-self-hosted-runners \
  --predicate-type https://slsa.dev/provenance/v1
sha256sum --check SHA256SUMS
```

Use the exact tag shown by the release. Do not weaken the repository, workflow,
source-ref, issuer or runner checks. Supplying `--bundle` avoids the authenticated
GitHub attestations API, although `gh` may still use the network to update
Sigstore trust material. A fully offline verifier additionally needs recently
obtained trusted-root metadata; see GitHub's offline verification documentation.

## Bootstrap boundary

The public one-command installer uses `curl ... | bash`. The shell script begins
executing before it can verify itself, so HTTPS delivery of that small bootstrap
script remains part of the trust boundary. Provenance protects the downloaded
release archive from substitution before extraction or installation when enabled; it cannot
retroactively authenticate commands already executed by the bootstrap script.

For a reviewable path, download the installer without executing it, inspect it,
and run the saved file only when satisfied. Never download an unverified copy of
`gh` or another verifier and then treat that same download as the root of trust.

The 1.0.0 release archive was built on Fedora 44 x86_64. That label does not establish
compatibility with every Fedora desktop configuration. The published 1.0.0
bootstrap was verified end-to-end with SHA256 and required provenance checks,
without GitHub login, followed by installation and basic Library launch in an
isolated session. Normal Wayland Shell activation is a separate step. The
maintainer also reports successful install/uninstall and application use on Ubuntu
26.04.1 with their shell scripts and provenance verification. The revised Standard
scripts also passed a live Fedora install/uninstall cycle with public 1.0.0, both
without gh and with required provenance, preserving the isolated test database.
Clipboard capture after login was confirmed on Fedora; Ubuntu retesting remains pending.
The 1.1.0 release workflow includes native ARM64 builds, but desktop validation
remains pending; the original 1.0.0 release has no ARM archive.

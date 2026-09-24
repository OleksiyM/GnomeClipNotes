# Verifying release artifacts

Each release archive has a GitHub keyless provenance attestation. Verification
checks the archive digest and binds it to this repository, the release workflow,
the exact version tag and a GitHub-hosted runner. It does not prove that the
software is bug-free or safe.

The installer always checks the archive against the release's `SHA256SUMS`
before extraction. This detects corruption, but **does not authenticate the
source** if both the archive and checksum manifest are replaced together.

If GitHub CLI (`gh`) is installed, the installer additionally verifies signed
provenance automatically. If `gh` is absent, it reports that only integrity was
checked and proceeds. `--require-provenance` makes absence of `gh` a blocking
error. An installed but incompatible `gh`, a failed attestation check, or a
missing bundle never silently falls back to checksums alone.

To enable provenance verification, install GitHub CLI from a trusted package
source (for example, your distribution) and rerun the installer.
No GitHub account, login or token is required when the matching local attestation
bundle is supplied with `--bundle`.

Download an archive and its adjacent `.sigstore.json` file from the same release.
For example, to verify version `1.0.0`:

```sh
tag=v1.0.0
archive=gnome-clip-notes-1.0.0-x86_64.tar.gz

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
  --source-ref "refs/tags/v1.0.0" \
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

The release archive is built on Fedora 44 x86_64. That label does not establish
compatibility with every Fedora desktop configuration. The public release
download and installation path have not yet been verified. ARM64 is not a
supported release architecture yet. Ubuntu has no prebuilt release target;
desktop compatibility is unverified.

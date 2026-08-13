# Signed updates

Sterna checks for updates only when the user chooses **Help > Check for
Updates**. The check downloads `latest.json` and `latest.json.sig` from the
latest GitHub release. Both the manifest and the selected AppImage or NSIS
installer must carry valid Ed25519 signatures from the key compiled into the
application. HTTPS is still required, but a compromised release page alone
cannot make an installed Sterna execute an unsigned file.

The release key is a permanent root of trust. `keygen.py` creates it once:

```sh
./packaging/update/keygen.py
```

The encrypted private key and its random password are written under the
ignored `.private/` directory. The public key and a test signature are written
beside this file and committed. Back up both private files together. Losing
them means existing installations cannot trust a replacement key and need one
manual update; exposing them means an attacker can sign an update every
existing installation will trust.

The committed key's SHA-256 fingerprint is
`9c778689a41f12ed5ac286138b912373825ca0535f80ff19d7d48d10dbdf278f`.
`create.py` derives the public half from the supplied private key and refuses
to produce a release if it does not match this committed root of trust. It
also fixes both artifact names from the workspace version, so the signed URLs
cannot quietly point at a differently named build.

Push the release commit and its matching `vX.Y.Z` tag. The `release-build`
workflow builds both packages on clean GitHub runners, runs the updater lock
regression on native Windows, and creates a draft containing the AppImage,
zsync metadata and NSIS installer. It never receives the updater private key.

Finish the draft locally:

```sh
./packaging/release.sh v0.1.4
```

That command refuses a non-draft or mismatched tag, downloads the exact three
GitHub-built files, creates `latest.json`, `latest.json.sig` and `SHA256SUMS`,
uploads them, byte-checks the uploaded metadata, requires the exact six-asset
set, then publishes the release as latest. The manifest uses immutable tag
URLs, not the moving `latest` URL. Until publication, an installed copy's
explicit check reports that no signed update is available. The zsync file is
for external AppImage tools; it does not replace the signed manifest used by
Sterna itself.

`create.py` remains available as the lower-level signing command and requires
`--linux`, `--zsync` and `--windows` paths.

To override the default local key files, provide `STERNA_UPDATE_PRIVATE_KEY` as
a path to the encrypted PEM and `STERNA_UPDATE_KEY_PASSWORD` as its password.
Those are secrets; `public-key.txt` is not. Never add either secret to GitHub.

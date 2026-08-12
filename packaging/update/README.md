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

After building both release artifacts, sign them and create the two assets the
updater reads:

```sh
./packaging/update/create.py \
  --linux packaging/appimage/build/sterna-x86_64.AppImage \
  --windows packaging/windows/build/sterna-0.1.0-x86_64-setup.exe
```

Upload both programs plus `release-update/latest.json` and
`release-update/latest.json.sig` to the matching `vX.Y.Z` GitHub release. The
manifest uses immutable tag URLs, not the moving `latest` URL. Publish the
release only when all four assets are present; until then an installed copy's
explicit check reports that no signed update is available.

For an automated release, provide `STERNA_UPDATE_PRIVATE_KEY` as a path to the
encrypted PEM and `STERNA_UPDATE_KEY_PASSWORD` as its password. Those are
secrets; `public-key.txt` is not.

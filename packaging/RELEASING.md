# Releasing Sterna

GitHub builds the artifacts; the updater signing key never leaves this machine.
A `vX.Y.Z` tag starts [`release.yml`](../.github/workflows/release.yml), which
produces a **draft** holding three unsigned files.
[`release.sh`](release.sh) signs their metadata here and publishes it.

Two scripts own the mechanical half. Neither commits, tags or pushes — those
stay deliberate.

| | |
|---|---|
| `./packaging/bump-version.sh X.Y.Z` | move the version, in all six files |
| `./packaging/bump-version.sh --check` | every file agrees (CI and preflight run this) |
| `./packaging/release.sh vX.Y.Z` | sign the draft's bytes and publish it |

## The order

1. **Land the work.** `main` clean, every branch merged, worktrees removed.
2. **Write the release notes** under `## [Unreleased]` in `CHANGELOG.md`. That
   section is what the release page shows, and `bump-version.sh` refuses to
   move a version with nothing under it — so the notes cannot be forgotten by
   getting as far as the tag without them.
3. **Run the gates** below, and read what they say.
4. **`./packaging/bump-version.sh X.Y.Z`**, then read the diff. Seven files:
   the six sites and `crates/Cargo.lock`.
5. **Commit** as `release: version X.Y.Z`, with nothing else in it.
6. **`git push origin main`** and let CI go green.
7. **`git tag -a vX.Y.Z -m "Sterna X.Y.Z"`**, then `git push origin vX.Y.Z`.
8. **Wait** — about seven minutes. `gh run watch` or
   `gh run view --json status`.
9. **`./packaging/release.sh vX.Y.Z`**. It downloads the draft's exact bytes,
   signs their metadata, uploads the manifest, re-downloads its own uploads to
   compare them, and only then flips the draft.

## Which number

Patch for a fix or anything a user would call the same program. Minor for a
feature, a new setting, or a deviation joining `docs/deviations.md`. Nothing
here is 1.0 yet, so major is not a question this file answers.

## The gates

The core and the shell are separate questions, and a release should ask both.
`AGENTS.md` has the full list; these are the ones a release turns on.

```sh
./run_diff.sh                       # THE gate — and it cannot run from a worktree
cd crates && cargo test --workspace && cargo clippy --all-targets -- -D warnings
```

...and the shell, in `sterna-fedora`, because this container's Qt is 6.4.2 and
the desktop's is 6.11.1:

```sh
distrobox-host-exec distrobox enter sterna-fedora --no-tty -- bash -lc '
  cd ~/Projects/Sterna/shell && cmake -S . -B build -G Ninja && cmake --build build
  export QT_QPA_PLATFORM=offscreen   # no display reaches a nested exec
  for t in render highlight gutter buttons tabs find log connect print plugin \
           cmdline control pty; do ./build/${t}_test || echo "FAILED $t"; done'
```

A change that touched neither the core nor the shell — packaging, docs — still
wants `--check` and CI, which is what steps 6 and 7 already wait for.

## What the scripts refuse, and why

`bump-version.sh` stops on a dirty tree, a version that is not `X.Y.Z`, one
that is not higher than the current one, a leading `v`, and an empty
`## [Unreleased]`. After rewriting it re-checks its own work and reports a
half-moved tree rather than leaving one.

`release.sh` stops on a tag that does not match `crates/Cargo.toml`, a release
that is not a draft, and an asset set that is not exactly six names. It cannot
be pointed at a published release by accident.

## Traps

- **The version is written in six files and the two obvious ones are not the
  problem.** `crates/Cargo.toml` and `shell/CMakeLists.txt` were the only two
  the preflight ever checked; `packaging/update/README.md`,
  `packaging/windows/README.md` (twice) and `CHANGELOG.md`'s two link lines
  could name a version nobody could download, for releases at a time, with
  nothing saying so. `bump-version.sh --check` is now the one list, and both
  workflows call it rather than keeping a copy — the same rule
  `packaging/appimage/toolchain.env` exists for.
- **`PLAN.md`'s commit count is the release commit's own number**, not the
  count before it: `5d33a2f` is commit 817 and says 817. The script computes
  it; setting it by hand is how 0.5.2 shipped saying 820 when it was 819.
  `--check` treats the count as advisory, because an ordinary merge moves it
  and that is not a release failure.
- **The `CHANGELOG` is narrative prose, not ASD-STE100.** `AGENTS.md` rule 9
  governs what a user reads *inside the program* — labels, messages, tooltips,
  the manual. No changelog entry has ever been STE, including 0.5.0's, which is
  the entry announcing that Sterna had adopted STE. Match the file.
- **A release that takes fifty minutes instead of seven is the Qt asset**, not
  a slow runner — `packaging/README.md` has the whole story, and
  `packaging/appimage/toolchain.env` pins what it fetches.
- **`./run_diff.sh` cannot run from a git worktree** — the oracle compiles
  `../teraterm` relative to the checkout. Release from the main checkout.
- **The draft is deletable and the tag is not, in practice.** A bad draft can
  be thrown away and the tag re-pushed; a *published* release is in the
  updater's manifest and somebody's client may already have it. `release.sh`
  is the last reversible moment.

## If it goes wrong

A failed build leaves the tag and no draft: fix `main`, delete the tag locally
and remotely, and push it again. A bad draft: `gh release delete vX.Y.Z` and
the same. Once `release.sh` has published, the way out is forward — a new
patch version, because the updater has already been told this one exists.

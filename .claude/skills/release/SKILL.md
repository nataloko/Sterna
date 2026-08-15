---
name: release
description: Cut a Sterna release — pick the version, write the changelog entry, run the gates, bump, tag, and publish the signed draft. Use when asked to release, cut a release, ship a version, tag a release, or publish X.Y.Z.
---

# Releasing Sterna

**The procedure lives in [`packaging/RELEASING.md`](../../../packaging/RELEASING.md).
Read it and follow it.** This file exists only to make it a slash command; it
holds nothing of its own, for the reason `CLAUDE.md` holds nothing that
`AGENTS.md` does not — a rule written where only one agent looks is a rule half
the agents on this repository never see.

Two things worth knowing before you open it, because they change what you do
first:

- **The release notes come before the version.** Write them under
  `## [Unreleased]` in `CHANGELOG.md`. `bump-version.sh` refuses to move a
  version with nothing under that heading, so writing them late means doing
  step 4 twice.
- **`./packaging/bump-version.sh X.Y.Z` does the whole rewrite.** Do not edit
  the version by hand — it is in six files and the release workflow used to
  check two of them. Read its diff instead.

Nothing here commits, tags or pushes on its own. Those stay deliberate, and
publishing is the last reversible moment.

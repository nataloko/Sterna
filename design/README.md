# design

Logo and mascot drafts for **Sterna** — the name chosen 2026-08-08 to replace
the working name `termitta`, which is taken in the wild.

Nothing here is final art, and none of it is wired into the build yet. The
rename itself has not happened: crates, binaries and docs still say `termitta`.

## The name

*Sterna* is the genus terns belong to. It contains "tern" whole, and
*Sterna paradisaea* — the arctic tern — makes the longest migration of any
animal, pole to pole, which is a better story for something that reaches
distant machines than a termite is.

Checked 2026-08-08: free on crates.io. `tern` alone is **taken** there by an
actively maintained crate, so the bird's own name was not available. Also free
and considered: `ternal`, `kittiwake` (a seabird that happens to contain the
`tt` from tty, but nine letters). One unrelated, inactive `94Peter/sterna` on
GitHub.

The `tt` thread lives in the command rather than the name: `sterna` the
project, `tt` the binary.

## Regenerating

```sh
./logos.py          # needs rsvg-convert for the PNGs; SVGs need nothing
```

Every SVG in `logos/` is generated. Edit `logos.py`, never the output.

## What is here

`logos/marks.png` — four candidate marks, each on light, on dark, and at
48/32/16 px:

- **tern** — the flight silhouette, which is already a prompt chevron. Swept
  wings, forked tail, coral bill at the point. The recommended one: it survives
  to 16 px and means two things at once.
- **swallowtail** — a block cursor with the tail notch bitten out. Cleanest at
  favicon size, but it is a flag; it needs the mascot beside it to carry the
  bird.
- **dive** — a plunge-dive read as a caret. Idea is sound, execution is not
  there; it currently reads as a tree.
- **tt** — two `t`s whose joined crossbar sweeps into wings. Mushy below 32 px.
  An earlier pass at this looked like an insect, which is the one thing the
  rename exists to avoid; check any revision against that.

`logos/mascot.png` — the tern as a character in four states (idle, wing up,
sleepy, in flight) and a strip showing it inside a terminal. The states are the
point: idle on the prompt, wing up while connected, eyes shut on a quiet
session, in flight during a transfer.

## Palette

| | | |
|---|---|---|
| ink | `#14181d` | near-black, slightly blue |
| paper | `#f7f5f0` | warm off-white |
| coral | `#e35336` | the bill and legs; also the cursor |
| grey | `#b9c2cc` | wings |
| light ink | `#eceff3` | the mark on a dark ground |

Coral is the only colour that survives on both grounds, so it is the accent in
both themes.

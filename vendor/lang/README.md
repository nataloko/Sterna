# Tera Term language files

The 14 `.lng` files in this directory are copied verbatim from Tera Term
revision `827a35b050c974b0fdf2a77ef73ed882301eb6c4`
(`v5.6.0-496-g827a35b05`, 2026-08-06):

```text
installer/release/lang_utf8/*.lng
```

They are UTF-8 INI files with a byte-order mark. Sterna keeps the format and
the bytes unchanged so existing translator work remains useful and a file can
be opened by either program.

Run `./sync.sh --check` to compare this directory with the sibling Tera Term
checkout. Run `./sync.sh` only when deliberately updating the vendored
revision, then read the complete diff and update the revision above and in
`ATTRIBUTION.md` before committing.

The files carry no per-file notice. Tera Term's project-wide 3-clause BSD
licence covers them; the copyright and redistribution notice are recorded in
the repository's `ATTRIBUTION.md` and `LICENSE`.

# `tt-i18n`

Loads Tera Term `.lng` files without converting them to Qt `.ts` catalogs.
They are INI files, so the crate deliberately reuses `tt-config`'s measured
`GetPrivateProfile*` behavior: first duplicate section and key win, matched
quotes are removed, and an empty value remains different from a missing one.

After lookup, the four escapes handled by upstream's `RestoreNewLineW` are
restored: `\\`, `\n`, `\t` and `\0`. Embedded NULs remain in the Rust string
because file-dialog filters depend on them.

The 14 catalogs live in `vendor/lang/`; `vendor/lang/sync.sh --check` verifies
that they remain byte-identical to the named upstream revision.

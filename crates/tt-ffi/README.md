# tt-ffi

The flat C ABI over `tt-session`, and the whole of the core/frontend seam.

`PLAN.md` puts the boundary here on purpose: the frontend is replaceable
because it only ever sees POD structs and functions — never a Rust type, a
trait object or an allocator — and nothing Win32- or Qt-shaped comes back the
other way. No `HWND`, no `QWidget`, no fonts, no glyphs, and no pixels beyond
`cell_w`/`cell_h`.

Builds as `libtermitta.so` / `termitta.a` (and an rlib), with the header at
`include/termitta.h`.

```sh
cargo build -p tt-ffi     # also regenerates include/termitta.h
./run_abi.sh              # compile the header and drive it from C and C++
```

## The header is generated, committed, and gated

`build.rs` runs cbindgen on every build and writes `include/termitta.h`. The
generated file is **committed**, and CI fails if regenerating it produces a
diff.

That is not belt-and-braces. The C ABI takes `TtKey`, `TtParity`, `TtCell` and
friends *straight from the core crates* — `#[repr(u32)]` on `tt_vt::Key` rather
than a second copy of the list here — so there is exactly one list of key names
and no way for a mapping table to be quietly wrong. The price is that
reordering one of those Rust enums is an ABI break, and the header diff is the
only place it shows up. Hence the gate.

Two things about how it is generated, both learned the hard way:

- **`with_src`, never `with_crate`.** `with_crate` shells out to `cargo
  metadata` from inside a build script, which can block on the package cache
  lock; and passing both parses `lib.rs` twice, which emits every declaration
  twice. `build.rs` lists the dependency source files explicitly, which also
  makes "what crosses the ABI" an explicit list rather than a transitive
  closure.
- **cbindgen parses files, not crates, so it cannot see privacy.** `tt-vt`'s
  private `locator_flag` module put `PIXEL`, `ONE_SHOT` and `FILTERED` into the
  public header until they were excluded by name in `cbindgen.toml`. Constants
  that *should* be public are renamed there too — `ATTR_BOLD` becomes
  `TT_ATTR_BOLD` — because a header that lands `DEFAULT_FG` in every
  translation unit that includes it is a header people work around.

## What crosses

| | |
|---|---|
| Lifecycle | `tt_session_new` / `_free`, `tt_config_default` |
| Screen | `tt_session_row` (borrowed, zero-copy), `_cols`, `_rows`, `_cursor`, `_title`, `_reverse_video`, `tt_palette_rgb` |
| Input | `_send_key`, `_send_text`, `_paste`, `_mouse`, `_focus`, `_resize`, `_set_cell_pixels`, `_send_break`, `_feed` |
| Connection | `_connect_serial`, `_disconnect`, `_is_connected`, `_describe`, `_pump`, `_drain_events` |
| Ports | `tt_serial_enumerate`, `tt_port_list_len` / `_at` / `_free` |

Deliberately absent, and each for a reason rather than for lack of time:

- **The settings surface.** `TtConfig` carries six fields, not `tt_vt::Config`'s
  thirty. Every one of those thirty is a `TERATERM.INI` key, which makes them
  the generated settings schema's job in Stage 2 — hand-transcribing them into
  a C struct now would be work done twice, the second time as a deletion.
- **Scrollback and selection.** `tt-session` has no viewport onto the
  scrollback yet, so there is nothing here to expose. Selection is a frontend
  concept the core only has to support.
- **SSH, telnet and pty connects.** One `connect` per transport, added as each
  transport lands. A generic `connect(url)` would have to grow a parser and a
  prompt protocol before either exists.

## Things a frontend will get wrong, so they are documented in the header

- **A cell's `fg`/`bg` mean a palette index only when `TT_ATTR2_FORE` /
  `TT_ATTR2_BACK` is set.** Without the bit the cell is asking for the
  *configured* default text colour, which the frontend owns. Painting index 0
  there gives a black-on-black screen — and it will look like a parser bug.
- **A wide character occupies two cells**, and the second has
  `width_class == TT_WIDTH_PAD` and no text. Painting per cell without skipping
  the pad draws the glyph twice.
- **Everything handed back is borrowed**, and each function says until when.
  Rows die at the next call that can change the grid; the event array and the
  strings in it die at the next drain. Only `tt_session_free` and
  `tt_port_list_free` are frees.
- **Open a port by its `open_path`, not its `device`.** `/dev/ttyUSB<n>` is
  assigned in attach order, so a saved profile naming one can reattach to a
  different physical port after a replug. `open_path` is the `by-path` name
  where the bus has one.
- **Null is handled at every entry point** and the C test proves it, so a
  frontend does not need a null check before every call. What is left is the
  contract no ABI escapes: a non-null pointer has to be real, and a session
  must not have been freed.

## `run_abi.sh` is the only test that means anything here

A Rust test calling these functions proves the logic and nothing about the
seam: it never compiles the header, never links the shared library, and cannot
notice that a struct the frontend must fill in is unreachable without a Rust
type. `tests/abi.c` is written the way the Qt shell will be — no helpers, just
the header — and the script also compiles the header as **C++**, which is what
will actually include it.

Both compile with `-Wall -Wextra -Werror -pedantic`, because a warning in a
header is the header's bug: a frontend that has to silence warnings to include
us will end up silencing the warnings.

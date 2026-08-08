# tt-ffi

The flat C ABI over `tt-session`, and the whole of the core/frontend seam.

`PLAN.md` puts the boundary here on purpose: the frontend is replaceable
because it only ever sees POD structs and functions — never a Rust type, a
trait object or an allocator — and nothing Win32- or Qt-shaped comes back the
other way. No `HWND`, no `QWidget`, no fonts, no glyphs, and no pixels beyond
`cell_w`/`cell_h`.

Builds as `libsterna.so` / `sterna.a` (and an rlib), with the header at
`include/sterna.h`.

```sh
cargo build -p tt-ffi     # also regenerates include/sterna.h
./run_abi.sh              # compile the header and drive it from C and C++
```

## The header is generated, committed, and gated

`build.rs` runs cbindgen on every build and writes `include/sterna.h`. The
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
| Viewport | `_scrollback_len`, `_view_offset`, `_set_view_offset`, `_cursor_view_row`, `_line_at`, `_top_line`, `_line` |
| Input | `_send_key`, `_send_text`, `_paste`, `_mouse`, `_focus`, `_resize`, `_set_cell_pixels`, `_send_break`, `_feed` |
| Connection | `_connect_serial`, `_connect_telnet`, `_connect_pty`, `_disconnect`, `_is_connected`, `_describe`, `_close_note`, `_pump`, `_drain_events` |
| SSH | `tt_ssh_params_default`, `tt_ssh_connect`, `_poll`, `_poll_fd`, `_host_key`, `_auth`, `_answer_host_key`, `_answer_auth`, `_free` |
| Ports | `tt_serial_enumerate`, `tt_port_list_len` / `_at` / `_free`, `tt_ssh_config_aliases` + `tt_string_list_*` |
| Logging | `tt_log_options_default`, `tt_session_log_start` / `_stop` / `_path` / `_bytes` |
| Settings | `tt_settings_field_count` / `_field` / `tt_settings_choice`, `tt_session_setting` / `_set_setting` / `_settings_load` / `_settings_save` |

Deliberately absent, and each for a reason rather than for lack of time:

- **A C struct of settings**, which is what `TtConfig` would have grown into.
  There is none: settings cross **by name**, and the *schema* crosses as data.
  `tt_settings_field` hands out a row per setting — name, page, INI section and
  key, kind, bounds, the `.lng` label, and the citation for the default — and a
  dialog builds itself from that table. A C struct would have had to be
  regenerated and rebuilt on both sides of the seam for every new setting; a
  table costs a line in `schema/settings.txt`. The strings it hands out live
  for the life of the process, which is the one exception to rule 2 above:
  they describe the schema rather than any session's values.
- **Selection.** A frontend concept the core only has to *support*, and what
  it supports is naming a line. `tt_session_row` is the painter's call and
  moves with the output; `tt_session_line_at` says which line a row is showing
  and `tt_session_line` reads that line back whether or not it is still in
  view. Which cells are highlighted, and what a double click means, stay the
  window's business.
- **A generic `connect(url)`.** There is one `connect` per transport instead —
  serial, telnet, pty, and SSH's polling variant — because a single entry point
  would have to grow a URL parser *and* a prompt protocol before either was
  needed. The four have genuinely different shapes: only SSH asks questions.
- **The pty's environment.** `TtPtyParams` carries argv, cwd, `TERM` and the
  login-shell flag; `PtyParams` in Rust also takes arbitrary environment
  variables. Those are settings, and settings are Stage 2's generated schema.

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
- **SSH connects by polling, not by returning.** `tt_ssh_connect` returns
  before anything has happened; `tt_ssh_connect_poll` then reports what the
  connection needs — a host-key decision, a password, a keyboard-interactive
  challenge — and attaches the transport to the session on `TT_SSH_READY`. A
  callback would have to be thread-safe and would fire on a worker thread,
  which is exactly where a Qt frontend cannot raise a dialog.
- **`tt_ssh_connect_poll_fd` is the same descriptor `tt_session_poll_fd`
  returns afterwards.** Register the notifier once, before connecting, and
  keep it: swapping it at the moment output starts is a race with the first
  screenful.
- **`tt_session_pump` returns as soon as the line is quiet**, which is the
  point of it — so waiting belongs on the descriptor, not in a pump loop.
  Pumping in a bare loop spins through a thousand iterations in a millisecond
  and concludes the far end never answered. The C test does it the right way.
- **A null string in `TtSshParams` means "take it from `~/.ssh/config`"**,
  which is not the same as an empty one. A dialog whose user field is blank
  must send null, or the config's `User` is overridden with nothing.
- **`tt_session_close_note` is what a disconnect *means*, and it is null for
  most transports.** An unplugged adapter and a closed socket are what they look
  like; a local shell is not, and "bash exited with status 1" is the difference
  between a window that explains itself and one that just goes quiet. Read it
  after `TT_EVENT_KIND_DISCONNECTED` and fall back to the generic wording.

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

# Crash artifacts

Inputs libFuzzer stopped on, kept after the bug was fixed. They are replayed
on **stable** by `cargo test -p tt-fuzz --test corpus`, so a fixed bug coming
back is an ordinary test failure rather than something only a fuzz run would
notice.

Kept deliberately small. `cargo fuzz tmin` leaves a chain of intermediate
`minimized-from-*` files behind, all reproducing the same thing; only the last
one is worth carrying, and it is worth renaming to say what it is.

| File | What it found |
|---|---|
| `vt_stream/deccra-over-a-wide-character` | A wide character, then `CSI 3;;;1 $ v`. Left an orphaned padding cell — and then turned out to be **upstream's** behaviour, since `BuffCopyBox` is a bare `memcpyW`. What it changed was the property, not the engine: see `Grid::check_wide_pairs`. |

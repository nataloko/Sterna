# `vte` bug report

**Status: drafted, not filed.** Filing needs a GitHub account; post to
<https://github.com/alacritty/vte/issues>. The text below is ready to paste.
Found against `vte 0.15.0`; re-check against the current release before filing.

Separate from `upstream-bugs.md`, which is Tera Term — that is this project's
*specification*, and a bug there is a behaviour to reproduce or to patch around
in the oracle. This is a *dependency*, and a bug in it is ours to work around
until it is fixed. Which it is: `tt-vt` now holds a partial UTF-8 sequence back
rather than handing it to `vte`, so `advance_partial_utf8` is never reached.
See `Vt::held`.

---

# `advance_partial_utf8` silently drops characters after a resumed sequence

## Summary

When `Parser::advance` resumes a UTF-8 sequence that was split across calls, it
prints only the **first** character it decoded but reports `valid_up_to()` as
the number of bytes consumed. Any complete character between that first one and
the incomplete tail is discarded without a trace — no replacement character, no
error, nothing.

## Reproduction

```rust
use vte::{Parser, Perform};

#[derive(Default)]
struct Collect(String);

impl Perform for Collect {
    fn print(&mut self, c: char) {
        self.0.push(c);
    }
}

fn feed(chunks: &[&[u8]]) -> String {
    let mut parser = Parser::new();
    let mut out = Collect::default();
    for c in chunks {
        parser.advance(&mut out, c);
    }
    out.0
}

fn main() {
    // "éa一" — the é cut in half by the chunk boundary.
    let whole = feed(&[b"\xc3\xa9a\xe4\xb8\x80"]);
    let split = feed(&[b"\xc3", b"\xa9a\xe4\xb8\x80"]);
    assert_eq!(whole, "éa一");
    assert_eq!(split, whole, "the 'a' was dropped");
}
```

```
assertion `left == right` failed: the 'a' was dropped
  left: "é一"
 right: "éa一"
```

## Cause

`advance_partial_utf8`, `src/lib.rs:687` in 0.15.0:

```rust
Err(err) => {
    let valid_bytes = err.valid_up_to();
    // If we have any valid bytes, that means we partially copied another
    // utf8 character into `partial_utf8`. Since we only care about the
    // first character, we just ignore the rest.
    if valid_bytes > 0 {
        let c = /* first char of partial_utf8[..valid_bytes] */;
        performer.print(c);
        self.partial_utf8_len = 0;
        return valid_bytes - old_bytes;
    }
```

`partial_utf8` is four bytes. With one byte held over (`old_bytes == 1`), three
more are copied in, so the buffer can hold the resumed two-byte character, a
complete one-byte character, and the first byte of a third. `from_utf8` then
returns `Err` with `valid_up_to() == 3`.

The comment is right that only the first character should be *printed* here.
The bug is the return value: `valid_bytes - old_bytes` tells `advance` that all
three bytes were consumed, so the main loop resumes past the `a` and it is never
seen by anything.

The neighbouring `Ok` branch already gets this right — it returns
`c.len_utf8() - old_bytes` and lets the main loop re-read the rest. That is why
`[b"\xc3", b"\xa9ab\xe4\xb8\x80"]` (two ASCII bytes instead of one) is correct:
the buffer is entirely valid, the `Ok` branch runs, and nothing is lost.

## Suggested fix

Return `c.len_utf8() - old_bytes` in the `Err` branch too, matching `Ok`. The
remaining bytes are then reprocessed by `advance`'s main loop, which is where
they would have been handled had the boundary fallen anywhere else.

## Why it matters

The trigger is not exotic. It needs a two-byte character cut by a read
boundary, then exactly one ASCII byte, then another multi-byte lead — which on
any UTF-8 terminal handling accented or CJK text is ordinary traffic. Where the
boundaries fall is the kernel's decision, so the same byte stream is fine one
run and lossy the next.

It is also invisible to most test suites, because a test that feeds a whole
buffer never enters this path at all. We found it with a property asserting that
the chunking of the input must not change the result.

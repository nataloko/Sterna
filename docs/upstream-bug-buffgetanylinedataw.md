# Upstream bug report — `BuffGetAnyLineDataW()` truncates lines at the first full-width character

**Status: drafted, not filed.** Filing needs a GitHub account; post to
<https://github.com/TeraTermProject/teraterm/issues>. The text below is ready to
paste. The patch is `oracle/patches/0001-buffgetanylinedataw-padding.patch`.

Before filing, re-check against current `main` — this was found against
`827a35b05` (v5.6.0-496) and may since have been fixed.

---

## Title

Session logging truncates any line at its first full-width character
(`BuffGetAnyLineDataW` does not advance past padding cells)

## Body

### Summary

`BuffGetAnyLineDataW()` in `teraterm/teraterm/buffer.c` walks a line cell by
cell and advances its cursor `b` only on the non-padding path. When it reaches
the padding cell that follows a full-width character it does `continue` **without
advancing `b`**, so `b` stays parked on that padding cell. Every subsequent
iteration sees padding as well, and the rest of the line is silently dropped.

### Impact

The only caller is `filesys_log.cpp:443`, so the user-visible effect is that
**session logging truncates any line at its first full-width character**. For a
terminal with Tera Term's CJK support that is silent data loss in a feature
people use to keep records.

### Location

`teraterm/teraterm/buffer.c`, in `BuffGetAnyLineDataW()` (around line 5832):

```c
for (i = 0; i < copysize; i++) {
    BOOL too_small;
    size_t len;
    if (IsBuffPadding(b)) {
        continue;          /* <-- b is never advanced */
    }
    len = expand_wchar(b, &buf[idx], left, &too_small);
    ...
}
```

### Reproduction

Log a session containing any full-width text. Everything from the first
full-width character onward is missing from the log file.

Reduced, running your `vtterm.c` + `buffer.c` unmodified and headless on Linux,
feeding the same bytes to a build with and without the one-line fix. Both
outputs below are actual program output at `827a35b05`, not reconstructed:

```
input:  ASCII 你好 world

before: |ASCII 你                      |
after:  |ASCII 你好 world              |
```

The harness reaches this through `BuffGetAnyLineDataW(PageStart + y, ...)`,
which is the same entry point `filesys_log.cpp` uses.

### Suggested fix

Advance the cursor on the padding path, as every other loop in this file does:

```diff
     if (IsBuffPadding(b)) {
+        b++;
         continue;
     }
```

### How this was found

While building a differential-test harness that compiles `vtterm.c` and
`buffer.c` unmodified on Linux, to validate a reimplementation against Tera
Term's real behaviour. Tera Term's own sources are the reference; this bug
showed up as a disagreement that turned out to be upstream's, not ours.

Happy to open a PR if the one-line fix above is the shape you want.

# Upstream bug reports

**Status: drafted, not filed.** Filing needs a GitHub account; post to
<https://github.com/TeraTermProject/teraterm/issues>. The text below is ready to
paste. Before filing, re-check against current `main` — all three were found
against `827a35b05` (v5.6.0-496) and may since have been fixed.

Three bugs, all found the same way: by feeding identical bytes to Tera Term's
real `vtterm.c`/`buffer.c` and to a reimplementation, and investigating every
disagreement. Two of the three turned up in Tera Term's *own* test scripts,
which had presumably been run many times by eye without anyone diffing the
buffer afterwards.

| # | Bug | Impact | Patch |
|---|---|---|---|
| 1 | `BuffGetAnyLineDataW` does not advance past padding cells | Session log truncated at the first full-width character | `oracle/patches/0001-buffgetanylinedataw-padding.patch` |
| 2 | `BuffGetAnyLineDataW` budgets output units with a column count | Session log truncated on any line with combining marks | `oracle/patches/0002-buffgetanylinedataw-left.patch` |
| 3 | `BuffEraseCharsInLine` writes `Count` cells from the cursor after clamping `Count` to the terminal width | **Out-of-bounds write** driven by an escape sequence | `oracle/patches/0003-bufferasecharsinline-overrun.patch` |

**File #3 first, and consider whether it warrants a private report** rather than
a public issue: the parameter is attacker-controlled, arrives over the network,
and the write leaves the buffer entirely on the last line.

---

# 1. `BuffGetAnyLineDataW()` truncates lines at the first full-width character

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

---

# 2. `BuffGetAnyLineDataW()` truncates any line containing combining characters

## Title

Session logging truncates lines with combining characters
(`BuffGetAnyLineDataW` budgets output units with a column count)

## Body

### Summary

`BuffGetAnyLineDataW()` keeps two counters that measure different things and
seeds them from the same value:

```c
copysize = min(NumOfColumns, bufsize - 1);   /* how many CELLS to walk   */
...
left     = copysize;                          /* how much BUFFER is left */
```

`left` is handed straight to `expand_wchar()` as its `buf_size`, which counts
`wchar_t` **units**, and is decremented by the number of units written. A cell
holding a base character plus a combining mark writes two units; a surrogate
pair writes two more. So on a line carrying combining marks the unit budget
runs out roughly twice as fast as the cell loop advances, `expand_wchar()`
reports `too_small`, the loop breaks, and the rest of the line is dropped.

### Impact

Same sole caller as bug 1, so the same consequence: **session logging silently
truncates**, this time at roughly half the terminal width on any line with
combining characters. Bug 1 and this one are independent — fixing either leaves
the other.

### Reproduction

Eight copies of `A` + U+3099 + space into a 12-column terminal. The cursor
arithmetic proves the buffer holds six of them on the first row, but only four
come back:

```
before: # cursor 4,1
        0 |A゙ A゙ A゙ A゙     |     <- four, and the row is not full
        1 |A゙ A゙         |

after:  # cursor 4,1
        0 |A゙ A゙ A゙ A゙ A゙ A゙ |     <- six, filling all twelve columns
        1 |A゙ A゙         |
```

Your own `tests/unicodebuf-combining1.sh` shows it directly.

### Suggested fix

Seed `left` with the real size of the output buffer. The cell loop is already
bounded by `copysize`, so the two limits stay independent:

```diff
     idx = 0;
-    left = copysize;
+    left = bufsize - 1;
```

---

# 3. ECH writes past the end of the line (out-of-bounds write)

## Title

`CSI Ps X` (ECH) writes past the end of the line; out-of-bounds heap write on
the last line

## Body

### Summary

ECH erases `Ps` characters starting at the cursor. `Ps` is clamped to the
terminal **width**, and then that many cells are written starting **at the
cursor**, so the write overshoots the line by exactly the cursor's column:

```c
/* vtterm.c:CSEraseCharacter() */
CheckParamVal(Param[1], NumOfColumns);      /* clamp to width      */
BuffEraseChars(Param[1]);                   /* -> XStart = CursorX */

/* buffer.c:BuffEraseCharsInLine() */
memsetW(&(CodeLineW[XStart]), 0x20, ..., Count);
```

`CodeBuffW` is a single contiguous allocation, so the overflow lands in the
following line and rewrites its first `CursorX` cells — text, attributes and
colours. On the last line of the buffer there is no following line and the
write leaves the allocation.

### Impact

The parameter is attacker-controlled: it arrives in the byte stream, and a
terminal reads that stream from the network or the serial line. With the cursor
at column 40, `printf '\033[999X'` is a 40-cell out-of-bounds write. `Count` is
also used afterwards to compute the redraw range, so that is overshot too.

### Reproduction

```
printf 'ECH line\033[999X' | oracle --cols 40 --rows 6 --attrs
```

Row 4 is never written to by the input, yet:

```
before:  4 |fffffffffff.............................|
after:   4 |........................................|
```

The eleven flagged cells on row 4 are the ones that overflowed from row 3.
Your own `tests/bcetest.sh` reaches this, via `CSI 999X`.

### Suggested fix

Clamp inside `BuffEraseCharsInLine()` rather than at the call site, so every
caller is covered and `Count` stays consistent with the redraw range derived
from it:

```diff
+    if (Count > NumOfColumns - XStart) {
+        Count = NumOfColumns - XStart;
+    }
     NewLine(PageStart+CursorY);
     memsetW(&(CodeLineW[XStart]),0x20, ..., Count);
```

`CSI Ps @` (ICH) and `CSI Ps P` (DCH) clamp correctly in
`BuffInsertSpace()`/`BuffDeleteChars()`; ECH is the one that does not, so the
fix is to bring it into line rather than to change a convention.

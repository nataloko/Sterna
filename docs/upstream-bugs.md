# Upstream bug reports

**Status: drafted, not filed.** Filing needs a GitHub account; post to
<https://github.com/TeraTermProject/teraterm/issues>. The text below is ready to
paste. Before filing, re-check against current `main` — all five were found
against `827a35b05` (v5.6.0-496) and may since have been fixed.

Five bugs, all found the same way: by feeding identical bytes to Tera Term's
real `vtterm.c`/`buffer.c` and to a reimplementation, and investigating every
disagreement. Two of them turned up in Tera Term's *own* test scripts, which had
presumably been run many times by eye without anyone diffing the buffer
afterwards.

| # | Bug | Impact | Patch |
|---|---|---|---|
| 1 | `BuffGetAnyLineDataW` does not advance past padding cells | Session log truncated at the first full-width character | `oracle/patches/0001-buffgetanylinedataw-padding.patch` |
| 2 | `BuffGetAnyLineDataW` budgets output units with a column count | Session log truncated on any line with combining marks | `oracle/patches/0002-buffgetanylinedataw-left.patch` |
| 3 | `BuffEraseCharsInLine` writes `Count` cells from the cursor after clamping `Count` to the terminal width | **Out-of-bounds write** driven by an escape sequence | `oracle/patches/0003-bufferasecharsinline-overrun.patch` |
| 4 | `BuffSelectedErase*` index a line-relative pointer with an absolute buffer offset | **Out-of-bounds read and write** driven by an escape sequence, and DECSED erases the wrong cells | `oracle/patches/0004-buffselectederase-wrong-base.patch` |
| 5 | `MakeMouseReportStr` builds the row's UTF-8 lead byte from the column | `DECSET 1005` mouse reports carry the wrong row, or invalid UTF-8, past row 96 | `oracle/patches/0005-mousereport-utf8-row.patch` |

**File #3 and #4 first, and consider whether they warrant a private report**
rather than public issues. Both are memory-safety bugs reachable from the byte
stream a terminal reads from the network: #3 leaves the allocation on the last
line, and #4 does so once the page has scrolled into the second half of the ring
buffer — confirmed under AddressSanitizer, not inferred.

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

---

# 4. DECSED indexes a line-relative pointer with an absolute buffer offset

## Title

`CSI ? Ps J` (DECSED) reads and writes outside the screen buffer, and erases
protected characters

## Body

### Summary

`BuffSelectedEraseCurToEnd()` and `BuffSelectedEraseHomeToCur()` — the two
halves of DECSED — both take a pointer to the **cursor's line**:

```c
buff_char_t *CodeLineW = &CodeBuffW[LinePtr];
```

and then index it with `j`, which is an **absolute buffer offset**:

```c
/* buffer.c:BuffSelectedEraseCurToEnd() */
TmpPtr = GetLinePtr(PageStart+CursorY);
for (i = CursorY ; i <= YEnd ; i++) {
    for (j = TmpPtr + offset; j < TmpPtr + NumOfColumns - offset; j++) {
        if (!(CodeLineW[j].attr2 & Attr2Protect)) {     /* wrong cell */
            BuffSetChar(&CodeBuffW[j], 0x20, 'H');      /* right cell */
            CodeLineW[j].attr &= AttrSgrMask;           /* wrong cell */
        }
    }
    offset = 0;
    TmpPtr = NextLinePtr(TmpPtr);
}
```

`CodeLineW[j]` is `CodeBuffW[LinePtr + j]` — roughly twice as far into the
buffer as intended. `BuffSelectedEraseHomeToCur()` has the same wrong base on
its protect test; its writes already use `CodeBuffW[j]`.

There is a second, independent defect in the same loop: `offset` is the start
column and is subtracted from the **end** bound as well, so on the cursor's own
line the loop covers `NumOfColumns - 2*CursorX` cells instead of
`NumOfColumns - CursorX` — and nothing at all once the cursor is past the
middle of the screen. Only the first line is affected; `offset` is zeroed for
the rest.

### Impact

Three things follow from the wrong base:

1. The protect bit is read from the wrong cell, so DECSED erases protected
   characters and preserves unprotected ones — the inverse of what the sequence
   exists to do.
2. `CodeLineW[j].attr &= AttrSgrMask` **modifies** that wrong cell, so
   unrelated scrollback silently loses its attributes.
3. `LinePtr` and `TmpPtr` each range up to `BufferSize - NumOfColumns`, so
   their sum reaches nearly twice `BufferSize`. Once the page has scrolled into
   the second half of the ring, both the read and the read-modify-write are
   **outside the allocation**. Both indices come off the wire: CUP places the
   cursor, and any output scrolls the page.

### Reproduction

Four lines of `aaaaPPPPcccc`, where `PPPP` is written under DECSCA 1
(`CSI 1 " q`), then DECSED 0 from row 2 column 7:

```
before:                       after:
  0 |aaaaPPPPcccc|              0 |aaaaPPPPcccc|
  1 |aaaaPPPPcccc|              1 |aaaaPPPP    |
  2 |    PPPP    |              2 |    PPPP    |
  3 |            |              3 |    PPPP    |
```

Row 1 is the cursor's own row and is left untouched — that is the `- offset`
defect. Row 3's protected run is erased, because its protect bits were read
from a line that is off the screen entirely.

Under AddressSanitizer, with the page scrolled down the ring first (200 line
feeds, then `CSI 20;40H`, then `CSI ? 0 J`):

```
==737980==ERROR: AddressSanitizer: heap-buffer-overflow
READ of size 1 at 0x7f56375fee33 thread T0
    #0 BuffSelectedEraseCurToEnd buffer.c:5491
    #1 CSQSelScreenErase vtterm.c:1773
    #2 CSQuest vtterm.c:3245
    #3 ParseCS vtterm.c:4127
0x7f56375fee33 is located 51 bytes after 448000-byte region
allocated by ChangeBuffer buffer.c:522
```

`CSI ? 1 J` reaches the same overflow at other cursor positions.

### Suggested fix

Index `CodeBuffW` directly — `CodeLineW` is not the right base for an absolute
offset — and drop the `- offset` from the end bound:

```diff
-        for (j = TmpPtr + offset; j < TmpPtr + NumOfColumns - offset; j++) {
-            if (!(CodeLineW[j].attr2 & Attr2Protect)) {
+        for (j = TmpPtr + offset; j < TmpPtr + NumOfColumns; j++) {
+            if (!(CodeBuffW[j].attr2 & Attr2Protect)) {
                 BuffSetChar(&CodeBuffW[j], 0x20, 'H');
-                CodeLineW[j].attr &= AttrSgrMask;
+                CodeBuffW[j].attr &= AttrSgrMask;
             }
         }
```

and in `BuffSelectedEraseHomeToCur()`:

```diff
         for (j = TmpPtr; j < TmpPtr + offset; j++) {
-            if (!(CodeLineW[j].attr2 & Attr2Protect)) {
+            if (!(CodeBuffW[j].attr2 & Attr2Protect)) {
```

`BuffSelectedEraseCharsInLine()` — DECSEL, `CSI ? Ps K` — is correct: it indexes
`CodeLineW` with a column, which is what that base is for.

### How this was found

By implementing DECSED against Tera Term's own behaviour and diffing the two
grids. The disagreement was not subtle once DECSCA was in the picture: the
reimplementation preserved the protected run and Tera Term erased it.

---

# 5. The UTF-8 mouse report builds the row's lead byte from the column

## Title

`DECSET 1005` mouse reports encode the row from the column above row 96

## Body

### Summary

`MakeMouseReportStr()` encodes the two coordinates of an extended mouse report
when UTF-8 mouse tracking (`CSI ? 1005 h`) is active. Values above 127 take a
two-byte form, and the row's branch computes its lead byte from `x`:

```c
/* vtterm.c:MakeMouseReportStr(), case IdMouseTrackExtUTF8 */
if (y < 128) {
    tmpy[0] = y;
    tmpy[1] = 0;
}
else {
    tmpy[0] = ((x >> 6) & 0x1f) | 0xc0;   /* <- x, should be y */
    tmpy[1] = (y & 0x3f) | 0x80;
    tmpy[2] = 0;
}
```

The column's branch four lines above is correct, so the typo only shows on a
terminal tall enough for the row to pass 96 — the wire value is the row plus
32. The continuation byte still comes from `y`, so the low six bits survive and
the row is wrong by a multiple of 64. When the column is small enough that
`(x >> 6) & 0x1f` is zero the report contains the byte `0xC0`, which is not a
valid UTF-8 lead byte at all.

### Reproduction

A press at window pixel (8, 1600) on a 300x200 terminal is column 2, row 101.
101 + 32 = 133, so the row needs two bytes:

```
before:  ESC [ M  SP  "  C0 85
after:   ESC [ M  SP  "  C2 85
```

`C2 85` decodes to U+0085 = 133. `C0 85` is an overlong form, rejected by any
UTF-8 decoder written since 2003.

### Patch

```diff
         else {
-            tmpy[0] = ((x >> 6) & 0x1f) | 0xc0;
+            tmpy[0] = ((y >> 6) & 0x1f) | 0xc0;
             tmpy[1] = (y & 0x3f) | 0x80;
             tmpy[2] = 0;
         }
```

### A related question, deliberately not patched

The button byte on the `_snprintf_s_l` line below is emitted with `%c` rather
than through the same encoder, so it too becomes a lone byte above 127 once
enough modifiers are held (`3 | 4 | 8 | 16 | 32 | 64` plus the 32 offset is
159). A host reading the report as UTF-8 — which is what mode 1005 asks it to
do — cannot decode that either. Changing it would change the wire format for
existing clients, so it wants a maintainer's decision rather than a patch from
outside.

### How this was found

By injecting mouse events into a headless build of `vtterm.c` and diffing its
reports against a reimplementation, across every combination of tracking mode
and encoding.

# Sending a file

Pasting a configuration into a console is the commonest thing anybody does to a
switch, and the commonest way it goes wrong is that the switch drops half of it.
A console port with no flow control has a small buffer and a slow parser: it
reads a line, echoes it, works out what it means — and everything that arrived
in the meantime is gone. What comes back is a configuration with eleven of its
forty lines missing and no error anywhere.

**File > Send file line by line…** feeds a text file to the far end at a pace
the far end can take. Two ways of deciding that pace, and they can be combined:

- **an interval** — wait a fixed time after each line, which is what Tera Term
  has always had;
- **a gate** — wait until the device *says* it is ready, which is the half a
  fixed interval can only guess at.

It is not a file transfer. Nothing is framed and nothing is acknowledged: the
far end sees exactly what somebody typing the file would have sent, which is why
this works on a device that has never heard of XMODEM. File > Send file… is the
other thing.

## Using it

The dialog asks two questions and then a file picker asks the third.

| | |
|---|---|
| **Wait for** | Nothing, a prompt, the echo of the line, or a quiet line |
| **When to wait** | Nothing, or after each character, line, or group of bytes |
| **Send the bytes of the file with no change** | Off sends text, with the line ending the terminal is set to. On sends the file exactly as it is on disk |
| **Show the sent text on the screen** | A local echo, for a device that does not send one back |

While it runs, a small panel says how far it has got and whether it is waiting
for the device. **Hold** stops the clock; **Stop** ends the send where it is.

**The keyboard is quiet while a send is running.** A line typed into the middle
of a configuration is a line the device runs in the wrong place, so anything
typed is dropped until the file is done — which is what Tera Term does too.

## The four ways of knowing the device is ready

**Nothing** is the interval on its own: send a line, wait, send the next.

**A prompt** is the one to reach for. Give it a regular expression — `[#>$] $`
covers most network equipment — and each line goes out when the device prints
its prompt again. It is faster than an interval on a quick device and safer on a
slow one, because it is measuring the thing that actually matters.

**The echo of the line** is for a device with no fixed prompt that echoes what
it is given. Sterna waits until the line it just sent comes back. It matches as
a substring, so a device that prints its prompt and the echo on one line still
counts.

**A quiet line** is for a device that answers with something different every
time. Sterna waits until nothing has arrived for a set number of milliseconds.

Every one of them has a **timeout**, and the timeout sends the line anyway. A
gate that stopped at the first unanswered line would leave half a configuration
in the device, which is the thing this feature exists to prevent. The panel
counts how many lines went out unanswered and says so at the end: a number there
almost always means the prompt pattern does not match what the device prints.

## What the prompt is matched against

The same text a macro's `wait` sees: the characters the terminal *printed*, with
no escape sequences in them. See [`macro/command/wait.md`](macro/command/wait.md)
for the full rule.

Two things about it are worth knowing.

**A prompt is not a line.** It is the one thing a console prints without a line
ending, so Sterna tests the pattern after every line feed *and* again against
whatever has arrived since the last one. `Switch#` with no newline after it
matches straight away.

**A completed line still has its CR on it**, which is `waitregex`'s rule and is
kept so that one pattern means one thing in both places. The practical
consequence: **`$` does not match at the end of a line** from a device that
sends CRLF. Match the prompt itself rather than anchoring to the end of a line.

The engine is the one the highlight rules and Find use — the same syntax, and
the same guarantee that a pattern cannot take exponential time. `waitregex` uses
a different engine with backreferences and lookaround; for everything short of
those two, the patterns are interchangeable. The dialog says so as you type if
a pattern will not compile.

## A button that does it

A quick button can carry a file. Set its kind to **Send file line by line**,
give it the path, and the button is one click. Its **Wait for** and **Prompt**
belong to the button rather than to the settings, so a page of buttons for a
switch and a page for a boot loader can each wait for the right thing. The
intervals stay in Setup: those say how long this program is prepared to wait,
not how a particular device shows that it is ready.

See [`buttons.md`](buttons.md).

## A macro that does it

`sendfile <file> <binary flag>` sends a file the same way, at whatever pace the
settings describe, and does not return until the last byte has gone. It is the
one of the sixteen transfer commands that is not a protocol.

Its manual page says text mode strips control characters. That was true of Tera
Term 4's sender; Tera Term 5 replaced it and strips nothing, and neither does
this. See [`upstream-bugs.md`](upstream-bugs.md).

## Two other places these intervals apply

**A paste is paced too.** `PasteDelayPerLine` ships at 10 ms, so a multi-line
paste already goes a line at a time on a fresh install. It has always been in
the settings file; it has not always done anything.

**A serial port has its own governor**, separate from all of the above and
applying to *everything* it sends — a keystroke, a paste, a macro's `send`.
`DelayPerChar` and `DelayPerLine`, on Setup > Serial port, and they are
suppressed for the length of a file transfer so a protocol is never paced.

## The file

The pace and the two-way switches are Tera Term's own keys, so a settings file
shared with one keeps them:

```ini
[Tera Term]
SendfileDelayType=PerLine
SendfileDelayTick=50
SendfileSize=4096
TransBin=off
PasteDelayPerLine=10
DelayPerChar=0
DelayPerLine=0
```

The gate is Sterna's, because upstream's sender cannot hear the far end at all
and so has no key to be compatible with:

```ini
[Sterna]
SendGate=prompt
SendGatePattern=[#>$] $
SendGateTimeout=500
SendQuietTime=300
```

`SendGate` is `none`, `prompt`, `echo` or `quiet`. `SendGateTimeout` is how long
to wait for the answer before sending anyway, in milliseconds. `SendQuietTime`
is how long the line has to be silent for the silence to count as an answer.

`[Sterna]` is this program's own section; no real Tera Term reads it, and a
settings file shared with one still opens correctly in both. See
[`deviations.md`](deviations.md) for why the gate exists and why the pacing half
is not a deviation at all.

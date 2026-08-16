# Counters

The board is connected, the cable is in, and the screen shows nothing. The first
question is whether *anything* is coming out of it — and until now the way to
answer that was to start a session log, wait, stop it, and look at the size of
the file.

Each terminal's status line has a counter field. It gives the time since the
connection opened, and how fast data moves in each direction:

```
router1        REC 4.01 MiB    0:44:00 ↓1.2k ↑8      ssh router1.example.net
```

The two arrows are the data rates in bytes each second: **↓** is what comes in
and **↑** is what goes out. A line that sends nothing for two seconds shows `0`.

**Click the field** to open the other counts.

## The counts

| | |
|---|---|
| **Connected** | The time since the connection opened |
| **Received** / **Sent** | All the data this connection has moved |
| **Receive rate** / **Send rate** | The same two numbers as the field, in full |
| **Lines** | The line endings received |
| **Breaks** | The breaks received |
| **Send queue** | Data that waits because flow control holds the line |

On a serial port there is one more row: **CTS**, **DSR**, **CD** and **RI**, the
four control lines. Green is on and gray is off. This frequently answers the
next question — if CTS is off, flow control stops the data, and the cable or the
far end is the cause.

Sterna reads the control lines only while these counts are on the screen. When
you close them, nothing reads the port.

## Each connection counts from zero

The counters are about **one connection**. A new connection starts them again at
zero, and the connection time beside them says which connection the totals are
for. This is different from the screen, which keeps its text through a
reconnection because that text usually explains why the connection stopped.

When a connection stops, the counters do not go away. They keep the totals, the
rates go to `0`, and the clock stops. The field becomes gray to show this. "How
much did that session move before it stopped" is a question you ask *after* the
line stops, so the answer stays on the screen.

## What is counted

**Received** is the data the cable carried, before Sterna does anything with it.
A file transfer is included, which the session log is not: the log records a
terminal session, but a counter that answers "is anything moving" must not show
zero during a 4 MB download.

**Sent** is the data the far end accepted. If flow control lets only some of it
out, the counter gives what went and **Send queue** gives what waits.

**Lines** counts the line endings in the data: a `CR`, an `LF`, or a `CR LF`
together, each one line. Sterna counts all three because some equipment ends its
lines with only a `CR`, and a counter that looked for `LF` would stay at zero
while the screen filled with text. Escape sequences that contain a `CR` are
counted also.

## The file

One key, in this program's own section:

```ini
[Sterna]
Counters=on
```

`View > Show counters` writes it, and so does the Window page of
Setup > Preferences. It is `on` when Sterna is installed.

`[Sterna]` is Sterna's own section. No Tera Term reads it, and a settings file
that you use with both programs opens correctly in both. See
[`deviations.md`](deviations.md) for why the feature exists, why the clock and
the rates are in the status line and the totals are behind a click, and why the
serial control lines are read only while you look at them.

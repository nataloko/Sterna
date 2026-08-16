# Reopening a serial port

Power-cycle the board and its USB adapter goes with it. The node leaves `/dev`,
the session ends, and a few seconds later the adapter is back and the terminal
is still sitting there disconnected. Every terminal on Linux handles this by
making you reconnect by hand, which is fine once and tiresome by the fourth time
in an afternoon.

Sterna waits for the port instead, and opens it again when it comes back. The
screen and the scrollback stay exactly as they were, so the output that explains
why the board went down is still in front of you.

## Using it

It is on by default. The switch is **Open the port again automatically**, in
the Serial half of the New connection dialog, beside the port you are choosing;
the same setting is on the Serial page of Setup as `serial.auto_reconnect`.

| | |
|---|---|
| **A line that ends on its own** | Starts the wait. The status line says which port it is waiting for |
| **The adapter comes back** | The port is opened again and the status line says so |
| **Disconnect** | Stops waiting. It is the only command that does |
| **Connect** | Also stops waiting — the tab is about something else now |
| **Unticking the box** | Stops a wait that is already running, not only the next one |
| **Closing the tab** | Stops waiting, because there is nothing left to wait for |

Disconnecting yourself never starts a wait: you asked for the line to end.

**The wait has no time limit.** A board switched off overnight is reconnected in
the morning. What is limited is the *opening*: once the port is back, Sterna
tries four times and then gives up and says why. The two are separate on
purpose — looking for a device that is not there costs nothing, and failing to
open one that is there usually means something is wrong that waiting will not
mend.

## What decides the timing

Five settings, all of them Tera Term's own, in `[Tera Term]`:

```ini
AutoComPortReconnect=on
AutoComPortReconnectDelayNormal=500
AutoComPortReconnectDelayIllegal=2000
AutoComPortReconnectRetryInterval=1000
AutoComPortReconnectRetryCount=3
```

`RetryCount` counts the tries **after** the first one, so three is four tries in
total. A try made in the moment after the port has gone away again counts as one
of them, and opens nothing.

The two delays are how long the port is left alone after it appears and before
it is opened, and which of them applies depends on the name the port was opened
with:

- A **`/dev/serial/by-path/…`** name — what the port picker gives you — is the
  socket on the hub, so the device that came back is the device that left, and
  the port is set up and ready. That takes `DelayNormal`.
- A **`/dev/ttyUSB0`** name is assigned in the order adapters attach, so it can
  be a different adapter; and the node appears as soon as the driver binds,
  before the system has finished preparing it. That takes the longer
  `DelayIllegal`.

On Windows every port is a `COM<n>`, and it always takes the shorter wait.

## What it does not do

It does not probe the port to find out whether it is back. Opening a serial port
raises DTR for as long as it is open and drops it again on closing, which reboots
an Arduino-style board and drops a modem's carrier — once per interval, for as
long as the wait lasted. Sterna asks whether the device node is there and opens
nothing until it is time to connect.

It reopens with the settings the **port** was using, not the ones in the file.
If a macro raised the speed to 921600 before the cable came out, the port comes
back at 921600.

It applies to serial ports only. A network connection that dropped is not the
same question: the host may have gone somewhere else, and reconnecting to it is
a decision rather than a repair.

And it changes nothing about the terminal. The session log stays open across the
reopen and keeps writing to the same file, the scrollback is not cleared, and
nothing is sent to the far end that you did not send.

See [`deviations.md`](deviations.md) for the three decisions inside this that
differ from Tera Term: why a reopen that gives up writes on the status line
instead of opening a dialog, why the longer delay is chosen by the port's name,
and why a port that flickers on the way back does not end the wait.

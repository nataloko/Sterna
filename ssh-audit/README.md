# ssh-audit — Stage 0 spike 5

Can `russh` negotiate what old equipment offers?

```sh
./servers.sh start && cargo run && ./servers.sh stop
```
```
30 negotiated, 9 not offered by the server, 0 genuine failures
```

## What this does and does not answer

`PLAN.md` listed spike 5 as "blocked: needs old gear". **Nobody involved has any
old gear** — the oldest machine available is a rolling-release Linux laptop. So
this spike was re-scoped to the part that is testable, and the untestable part
was accepted explicitly rather than left looking open.

**Tested:** whether every pre-2020 algorithm can actually be negotiated
end-to-end, against two independent server implementations, plus the auth
methods old devices actually use.

**Not tested, and not testable here:** real-device *behaviour*. Non-RFC banners,
devices that hang up on an unexpected packet, key exchange that takes 30 seconds
on an underpowered CPU, keyboard-interactive prompts that don't follow the spec.
A container running old algorithms models the *cryptography* faithfully and the
*weirdness* not at all.

That residual risk is real and stays open. Its mitigation is the one already in
the plan: keep SSH behind a trait so `libssh2` can be swapped in if a device
refuses to talk. What changed is that the trait seam is now the *plan* rather
than insurance.

## The servers

| | |
|---|---|
| `:2222` | OpenSSH 9.6 with the legacy algorithms explicitly re-enabled — `ssh-rsa`, SHA-1 kex, CBC ciphers, `hmac-sha1`. Modern OpenSSH still implements all of it, just disabled by default, and `+` in the config re-enables rather than replaces. |
| `:2223` | Dropbear 2022.83 — a genuinely different codebase, and the implementation actually found on console servers and embedded kit. |

Both bind `127.0.0.1` only. `servers.sh start` also creates a throwaway
`termitta-test` account for the password cases; `stop` removes it.

## Results

Everything the servers offered, russh negotiated:

- **KEX** — `diffie-hellman-group1-sha1`, `group14-sha1`, `group-exchange-sha1`,
  `group14-sha256`, `curve25519`
- **Host keys** — `ssh-rsa` (SHA-1 signatures), `rsa-sha2-256/512`, `ssh-ed25519`
- **Ciphers** — `3des-cbc`, `aes128/256-cbc`, `aes128-ctr`, `aes256-gcm`,
  `chacha20-poly1305`
- **MACs** — `hmac-sha1`, `hmac-sha2-256/512`
- **Auth** — publickey, password (accepted *and* correctly rejected),
  keyboard-interactive

## Two findings that shape `tt-conn`

**1. Legacy algorithms are opt-in, and that is a feature.** None of the SHA-1
kex, CBC ciphers or `ssh-rsa` host keys are in russh's default preference list —
correct security posture. But it means `tt-conn` must expose a deliberate
"legacy mode" per connection, the way PuTTY and SecureCRT do, or the tool simply
will not talk to old equipment. This is a UI and settings decision, not a
library limitation.

**2. Embedded servers offer very little.** Dropbear's default build declined 9
of the algorithms tested — no 3DES, no CBC modes, no `dh-group1-sha1`, no
ed25519 host key, no `hmac-sha2-512`. Real console servers will be at least as
narrow, and narrow in *different* directions. The client must therefore offer a
broad set and let the server choose; hard-coding a modern-only list is what
breaks against this hardware.

## Reading the output

Three outcomes, and the distinction matters:

- `ok` — negotiated, authenticated, command ran, output verified.
- `n/a` — the **server** does not offer it. Each case offers exactly *one*
  algorithm in the dimension under test, so "No common X algorithm"
  unambiguously means the server lacks it. Not a russh gap.
- `FAIL` — the server offered it and russh could not complete. This is the only
  outcome that is a finding. There were none.

## Gaps

- `hmac-md5` is absent from russh entirely. Some pre-2005 gear requires it.
  Nothing to test against, and no fix short of patching russh.
- `ssh-dss` host keys are available behind the `dsa` feature but untested —
  neither server offers them.
- SSH-1 is not covered and never will be; `PLAN.md` drops it deliberately.
- No rekeying, no compression, no proxy-jump, no agent forwarding.

//! The other half of the command line, which upstream keeps in a plugin.
//!
//! `TTXParseParam` (`ttxssh.c:1476`) hooks `_ParseParam` and runs **first**,
//! and it does something a plain parser does not: it **blanks the options it
//! consumed out of the line** before Tera Term sees it, and rewrites two forms
//! into something Tera Term will understand. `/ssh`, `/auth=`, `/user=`,
//! `/passwd=`, `/keyfile=` and the `ssh://` URL scheme all live here rather
//! than in `ttset.c` — so a port that reads only `ttset.c` has a command line
//! that cannot open an SSH session, which is most of them.
//!
//! [`parse`] returns what it took *and the line it left behind*, because the
//! two halves compose through that string and not through a shared struct. An
//! `ssh://user@host/` becomes a bare `host:22` token for
//! [`super::CommandLine::parse`] to find; a `user@host` has its user part
//! blanked in place and the host stays where it was, which upstream notes is
//! fine because "the following TTX and Tera Term skip spaces".
//!
//! Three things here are easy to get wrong by reading the code quickly:
//!
//! - **`-` works as well as `/`.** `option[0] == '-' || option[0] == '/'`
//!   (`:1498`). Tera Term's own parser accepts only `/`, so `-ssh` is an SSH
//!   session and `-nolog` is nothing at all.
//! - **`ssh` is matched case-sensitively** (`wcsncmp`, not `_wcsnicmp`), so
//!   `/SSH` falls through to the "not a ttssh option" arm, is left in the line,
//!   and is then ignored by Tera Term too. It does nothing, silently.
//! - **`/t=2` is consumed and `/t=0` is not.** TTSSH reads `/t=2` as its own
//!   ("SSH") and deletes it; for any other value it sets `Enabled = 0` and
//!   deliberately leaves the option in place for Tera Term to read as telnet.

use super::{after_ci, eq_ci, lower, token_spans};
use crate::services::scanf_int;

/// The four icons `/ssh-icon=` can name (`ttxssh.c:1604`), which are resource
/// ids upstream and names here — this port has no icon table to resolve them
/// against, and inventing one would be a second list to keep in step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    /// `old`, `yellow`, `securett_yellow`.
    Yellow,
    /// `green`, `securett_green`.
    Green,
    /// `flat`, `securett_flat`.
    Flat,
    /// Anything else, including a name nobody recognises.
    Default,
}

/// `pvar->ssh2_authmethod` — what `/auth=` asked for.
///
/// `challenge` and `keyboard-interactive` are one method under two names, and a
/// value that is neither of the five leaves the method unset **while still
/// switching automatic login on** — the `else` arm is a bare `// TODO:`
/// (`ttxssh.c:1713`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthMethod {
    Password,
    /// `keyboard-interactive`, or `challenge`.
    KeyboardInteractive,
    PublicKey,
    /// Pageant, which is PuTTY's agent. This port speaks to `ssh-agent`
    /// instead; the option is recorded rather than mapped.
    Pageant,
}

/// A settings file `/ssh-f=` or `/ssh-consume=` named.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OptionsFile {
    pub path: Vec<u8>,
    /// `/ssh-consume=`, which **deletes the file** after reading it
    /// (`DeleteFileW`, `ttxssh.c:1507`) — it is how a launcher passes a
    /// password on disk without leaving it there.
    pub consume: bool,
}

/// What TTSSH took out of a command line.
///
/// Every field is `Option` or empty where upstream would have left its own
/// setting alone, so applying this over a settings file changes only what was
/// asked for.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SshOptions {
    /// `settings.Enabled` — `/ssh`, `/ssh1`, `/ssh2`, `/t=2` and an `ssh://`
    /// URL turn it on; `/nossh` and `/telnet` turn it off, as does a `/t=`
    /// with anything but 2.
    pub enabled: Option<bool>,
    /// `settings.ssh_protocol_version`. This port speaks SSH-2 only, so a 1
    /// here is a refusal to make somewhere better than the parser.
    pub protocol_version: Option<u8>,
    /// `settings.DefaultForwarding`, one entry per spec, each keeping the
    /// `L`/`R`/`D`/`X` letter that says which kind it is. Upstream joins them
    /// with `;` into one string; keeping them apart is the same information.
    pub forwarding: Vec<Vec<u8>>,
    /// `settings.X11Display` — whatever followed `/ssh-X`, which upstream
    /// copies unconditionally because it tests `option+6 != 0` (a pointer that
    /// is never null) where it meant `*(option+6) != 0`.
    pub x11_display: Option<Vec<u8>>,
    /// `/ssh-v`.
    pub verbose: bool,
    /// `settings.TryDefaultAuth` — `/ssh-autologin`, spelled `-autologon` too.
    pub try_default_auth: bool,
    /// `/ssh-A` and `/ssh-a`.
    pub forward_agent: Option<bool>,
    /// `/ssh-agentconfirm=`, where `off`, `no`, `false`, `0` and `n` are off and
    /// **everything else is on** — including `/ssh-agentconfirm=` with nothing
    /// after it.
    pub forward_agent_confirm: Option<bool>,
    /// `/ssh-C=n`, clamped to 0..=9; `/ssh-C` is 6 and `/ssh-c` is 0.
    pub compression_level: Option<u8>,
    pub icon: Option<Icon>,
    /// `/ssh-subsystem=`, which also sets `use_subsystem`.
    pub subsystem: Option<Vec<u8>>,
    /// `/ssh-N` — no shell, just the forwardings.
    pub no_session: bool,
    /// `pvar->ssh2_autologin`, set by `/auth=` and cleared again by
    /// `/ask4passwd` whichever order they come in.
    pub auto_login: bool,
    pub auth_method: Option<AuthMethod>,
    /// `/user=`, or the user part of a URL or a `user@host`.
    pub username: Option<Vec<u8>>,
    /// `/passwd=`, or the password in a URL — which is percent-decoded, where
    /// `/passwd=` is not.
    pub password: Option<Vec<u8>>,
    /// `/keyfile=`.
    pub key_file: Option<Vec<u8>>,
    /// `/ask4passwd`.
    pub ask_password: bool,
    /// `/nosecuritywarning` — skip the `known_hosts` check. Upstream calls it a
    /// hidden option because it lowers security; it is recorded here and
    /// deliberately hard to reach.
    pub no_known_hosts_check: bool,
    /// `/telnet`, which switches SSH off *and* sets Tera Term's own telnet flag
    /// (`ttxssh.c:1679`) even though the option is consumed.
    pub force_telnet: bool,
    /// `/ssh-f=` and `/ssh-consume=`, in the order they appeared. Reading them
    /// is not implemented: they are INI files holding a `[TTSSH]` section this
    /// schema does not have yet.
    pub options_files: Vec<OptionsFile>,
    /// Options that begin with `/ssh` and match nothing, which upstream reports
    /// in a message box — the only diagnostic anywhere in either parser. Kept
    /// so a frontend can show it rather than swallowing a typo.
    pub unknown: Vec<Vec<u8>>,
}

/// What TTSSH did with one token — `action`, which is `OPTION_NONE`,
/// `OPTION_CLEAR` or `OPTION_REPLACE` (`ttxssh.h`).
#[derive(Clone, Debug, PartialEq, Eq)]
enum Action {
    /// Not TTSSH's, or deliberately left for Tera Term to read.
    Keep,
    /// Consumed: blanked out of the line.
    Clear,
    /// Rewritten in place — an `ssh://` URL or a `user@host`.
    ///
    /// The payload is the option buffer **as upstream leaves it**, which is
    /// always as long as the token was: the URL arm space-fills the tail and the
    /// `user@host` arm turns the user part into leading spaces. The line keeps
    /// that padding, since it is what the next parser reads; the token path
    /// trims it, since there is no line for it to sit in.
    Replace(Vec<u8>),
}

/// `TTXParseParam` — the options, and the line with them taken out.
///
/// Two passes, like `_ParseParam` and for the same reason: the first reads a
/// settings file so the second can be applied over it.
pub fn parse(line: &[u8]) -> (SshOptions, Vec<u8>) {
    let mut opts = SshOptions::default();
    let mut line = line.to_vec();

    for pass in [Pass::One, Pass::Two] {
        let edits: Vec<_> = token_spans(&line, line.len() + 1)
            .into_iter()
            .filter_map(|(span, tok)| match opts.token(&tok, pass) {
                Action::Keep => None,
                Action::Clear => Some((span, None)),
                Action::Replace(text) => Some((span, Some(text))),
            })
            .collect();
        apply_edits(&mut line, edits);
    }

    (opts, line)
}

/// The same over arguments the platform has already split — Unix `argv` with
/// `argv[0]` dropped — returning the tokens that survived.
///
/// **There is no line to blank here, so the two halves compose through the
/// token list instead**, which is the same thing one step earlier: a cleared
/// token is one that vanishes from re-tokenising, and a replaced one is a token
/// whose leading spaces the next parser would have skipped. Tokenising a joined
/// `argv` instead would quote-process everything twice and turn `/W=My Session`
/// into two options — the trap `tt-ttl`'s `CmdLine::from_args` already carries.
pub fn parse_args<I, S>(args: I) -> (SshOptions, Vec<Vec<u8>>)
where
    I: IntoIterator<Item = S>,
    S: AsRef<[u8]>,
{
    let mut opts = SshOptions::default();
    let mut toks: Vec<Vec<u8>> = args.into_iter().map(|a| a.as_ref().to_vec()).collect();
    for pass in [Pass::One, Pass::Two] {
        toks = toks
            .into_iter()
            .filter_map(|tok| match opts.token(&tok, pass) {
                Action::Keep => Some(tok),
                Action::Clear => None,
                Action::Replace(text) => Some(text.trim_ascii().to_vec()),
            })
            .collect();
    }
    (opts, toks)
}

/// Which of `TTXParseParam`'s two loops is running. The first reads a settings
/// file and consumes nothing else; the second is everything.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pass {
    One,
    Two,
}

impl SshOptions {
    /// One token, in one of the two passes.
    fn token(&mut self, tok: &[u8], pass: Pass) -> Action {
        if pass == Pass::One {
            // `/ssh-f=`, `/ssh-consume=` and `/f=`. The first two are consumed;
            // `/f=` is left alone because "Tera Term側でも解釈する必要がある" —
            // it is Tera Term's option, read here as well so that one `/F=`
            // brings both halves of the same file.
            let Some(rest) = switch_body(tok) else {
                return Action::Keep;
            };
            for (prefix, consume) in [(&b"ssh-f="[..], false), (b"ssh-consume=", true)] {
                if let Some(path) = after_cs(rest, prefix) {
                    self.options_files.push(OptionsFile {
                        path: path.to_vec(),
                        consume,
                    });
                    return Action::Clear;
                }
            }
            if let Some(path) = after_ci(rest, b"f=") {
                self.options_files.push(OptionsFile {
                    path: path.to_vec(),
                    consume: false,
                });
            }
            return Action::Keep;
        }

        if let Some(rest) = switch_body(tok) {
            // `action = OPTION_CLEAR` is set before the chain runs, so a switch
            // TTSSH recognises is consumed unless an arm says otherwise.
            let consumed = self.switch_arm(rest);
            // "パスワードを聞く場合は自動ログインが無効になる" — and this runs
            // for every switch, not only for `/ask4passwd`, so the two options
            // may come in either order.
            if self.ask_password {
                self.auto_login = false;
            }
            return match consumed {
                true => Action::Clear,
                false => Action::Keep,
            };
        }
        if let Some(rewritten) = self.url(tok) {
            return Action::Replace(rewritten);
        }
        if let Some(rewritten) = self.user_at_host(tok) {
            return Action::Replace(rewritten);
        }
        Action::Keep
    }

    /// One `/`- or `-`-led token. Returns whether TTSSH consumed it.
    fn switch_arm(&mut self, rest: &[u8]) -> bool {
        // `wcsncmp`, so the case matters and `/SSH` is not this.
        if let Some(tail) = after_cs(rest, b"ssh") {
            self.ssh_arm(tail);
            return true;
        }
        if let Some(v) = after_ci(rest, b"t=") {
            // `/t=2` is TTSSH's own extension and is deleted; anything else is
            // Tera Term's telnet switch and is left for it to read.
            self.enabled = Some(v == b"2");
            return v == b"2";
        }
        match rest {
            // `/1` and `/2` after a `/ssh`, which set only the version.
            b"1" => self.protocol_version = Some(1),
            b"2" => self.protocol_version = Some(2),
            b"nossh" => self.enabled = Some(false),
            b"telnet" => {
                self.enabled = Some(false);
                self.force_telnet = true;
            }
            b"ask4passwd" => self.ask_password = true,
            b"nosecuritywarning" => self.no_known_hosts_check = true,
            _ => {
                if let Some(v) = after_cs(rest, b"auth=") {
                    self.auto_login = true;
                    self.auth_method = match lower(v).as_slice() {
                        b"password" => Some(AuthMethod::Password),
                        b"keyboard-interactive" | b"challenge" => {
                            Some(AuthMethod::KeyboardInteractive)
                        }
                        b"publickey" => Some(AuthMethod::PublicKey),
                        b"pageant" => Some(AuthMethod::Pageant),
                        // The `// TODO:` arm: no method, but automatic login is
                        // on regardless.
                        _ => None,
                    };
                } else if let Some(v) = after_cs(rest, b"user=") {
                    self.username = Some(v.to_vec());
                } else if let Some(v) = after_cs(rest, b"passwd=") {
                    self.password = Some(v.to_vec());
                } else if let Some(v) = after_cs(rest, b"keyfile=") {
                    self.key_file = Some(v.to_vec());
                } else {
                    // Not a TTSSH option: leave it in the line.
                    return false;
                }
            }
        }
        true
    }

    /// The `/ssh…` family, whose tail is everything after those three letters.
    fn ssh_arm(&mut self, tail: &[u8]) {
        if tail.is_empty() {
            self.enabled = Some(true);
            return;
        }
        // `/ssh1` and `/ssh2` set the version *and* switch SSH on, where a bare
        // `/1` sets only the version.
        if tail == b"1" || tail == b"2" {
            self.enabled = Some(true);
            self.protocol_version = Some(tail[0] - b'0');
            return;
        }
        if let Some(spec) = strip_cs(tail, b"-L")
            .or_else(|| strip_cs(tail, b"-R"))
            .or_else(|| strip_cs(tail, b"-D"))
        {
            // The letter stays on the front of every spec, and one option may
            // carry several separated by `;` or `,`.
            let letter = tail[1];
            for part in spec.split(|&b| b == b';' || b == b',') {
                if part.is_empty() {
                    continue;
                }
                let mut e = vec![letter];
                e.extend_from_slice(part);
                self.forwarding.push(e);
            }
            return;
        }
        if let Some(display) = strip_cs(tail, b"-X") {
            self.forwarding.push(b"X".to_vec());
            self.x11_display = Some(display.to_vec());
            return;
        }
        if tail == b"-v" {
            self.verbose = true;
        } else if eq_ci(tail, b"-autologin") || eq_ci(tail, b"-autologon") {
            self.try_default_auth = true;
        } else if let Some(v) = after_ci(tail, b"-agentconfirm=") {
            self.forward_agent_confirm = Some(!matches!(
                lower(v).as_slice(),
                b"off" | b"no" | b"false" | b"0" | b"n"
            ));
        } else if tail == b"-a" {
            self.forward_agent = Some(false);
        } else if tail == b"-A" {
            self.forward_agent = Some(true);
        } else if let Some(v) = strip_cs(tail, b"-C=") {
            self.compression_level = Some(scanf_int(v).unwrap_or(0).clamp(0, 9) as u8);
        } else if tail == b"-C" {
            self.compression_level = Some(6);
        } else if tail == b"-c" {
            self.compression_level = Some(0);
        } else if let Some(v) = after_ci(tail, b"-icon=") {
            self.icon = Some(match lower(v).as_slice() {
                b"old" | b"yellow" | b"securett_yellow" => Icon::Yellow,
                b"green" | b"securett_green" => Icon::Green,
                b"flat" | b"securett_flat" => Icon::Flat,
                _ => Icon::Default,
            });
        } else if let Some(v) = strip_cs(tail, b"-subsystem=") {
            self.subsystem = Some(v.to_vec());
        } else if tail == b"-N" {
            self.no_session = true;
        } else {
            // The message box, which is the only diagnostic in either parser.
            let mut o = b"/ssh".to_vec();
            o.extend_from_slice(tail);
            self.unknown.push(o);
        }
    }

    /// `ssh://user:password@host:port/`, and the five other schemes that mean
    /// the same thing (`ttxssh.c:1749`).
    ///
    /// The token is rewritten to just `host:port`, with `:22` appended when
    /// there was none, so that Tera Term's own parser finds an ordinary host
    /// name. A digit immediately before the `://` is the protocol version,
    /// which is how `ssh1://` and `slogin2://` work.
    fn url(&mut self, tok: &[u8]) -> Option<Vec<u8>> {
        const SCHEMES: [&[u8]; 6] = [
            b"ssh://",
            b"ssh1://",
            b"ssh2://",
            b"slogin://",
            b"slogin1://",
            b"slogin2://",
        ];
        if !SCHEMES.iter().any(|s| after_ci(tok, s).is_some()) {
            return None;
        }
        let colon = tok.iter().position(|&b| b == b':')?;
        match tok[colon - 1] {
            b'1' => self.protocol_version = Some(1),
            b'2' => self.protocol_version = Some(2),
            _ => {}
        }
        let mut authority = &tok[colon + 3..];
        // The path part is thrown away rather than parsed.
        if let Some(slash) = authority.iter().position(|&b| b == b'/') {
            authority = &authority[..slash];
        }
        if let Some(at) = authority.iter().rposition(|&b| b == b'@') {
            let user = &authority[..at];
            match user.iter().position(|&b| b == b':') {
                Some(c) => {
                    self.username = Some(percent_decode(&user[..c]));
                    self.password = Some(percent_decode(&user[c + 1..]));
                }
                None => self.username = Some(percent_decode(user)),
            }
            authority = &authority[at + 1..];
        }
        let mut host = authority.to_vec();
        let bracketed = host.first() == Some(&b'[') && host.last() == Some(&b']');
        if bracketed || (host.first() != Some(&b'[') && !host.contains(&b':')) {
            host.extend_from_slice(b":22");
        }
        self.enabled = Some(true);
        // The rewrite happens *inside* the token's own buffer, so what replaces
        // it is exactly as long as it was: `wmemset(option+hostlen, ' ', …)`
        // fills the rest. Only the trailing spaces differ from returning the
        // host alone, but the string is what the next parser reads, so it is
        // reproduced.
        Some(padded(host, tok.len()))
    }

    /// `user@host` with no scheme — the user part is taken and blanked, and the
    /// host is left exactly where it was.
    ///
    /// TTSSH does this even for a session that will not be SSH, because a
    /// telnet one has nowhere to put a user name until Tera Term learns the
    /// telnet authentication option.
    ///
    /// The user part becomes **leading spaces** rather than disappearing:
    /// `wmemset(option, ' ', p-option+1)` covers the name and the `@`, so
    /// `me@myhost` is replaced by `   myhost` and the host name does not move.
    /// Upstream says as much — "後続のTTXやTera Term本体で解釈する時にはスペース
    /// を読み飛ばすので、ホスト名を先頭に詰める必要は無い".
    fn user_at_host(&mut self, tok: &[u8]) -> Option<Vec<u8>> {
        let at = tok.iter().position(|&b| b == b'@')?;
        self.username = Some(tok[..at].to_vec());
        let mut out = vec![b' '; at + 1];
        out.extend_from_slice(&tok[at + 1..]);
        Some(out)
    }
}

/// A rewritten token, space-filled to the length of the one it replaces —
/// upstream rewrites in place, so the replacement always has the original's
/// length and the span always has room for it.
fn padded(mut v: Vec<u8>, len: usize) -> Vec<u8> {
    if v.len() < len {
        v.resize(len, b' ');
    }
    v
}

/// `option[0] == '-' || option[0] == '/'` — and the rest of the token.
///
/// Both leaders, which is TTSSH's alone: Tera Term's own parser tests for `/`
/// only, so `-nolog` reaches nobody.
fn switch_body(tok: &[u8]) -> Option<&[u8]> {
    match tok.first() {
        Some(b'-') | Some(b'/') => Some(&tok[1..]),
        _ => None,
    }
}

/// `wcsncmp(a, b, n) == 0` — a case-**sensitive** prefix, which is what most of
/// this file uses and is the reason `/SSH` does nothing.
fn after_cs<'a>(t: &'a [u8], prefix: &[u8]) -> Option<&'a [u8]> {
    t.strip_prefix(prefix)
}

fn strip_cs<'a>(t: &'a [u8], prefix: &[u8]) -> Option<&'a [u8]> {
    t.strip_prefix(prefix)
}

/// `percent_decode` (`ttxssh.c:1438`) — `%` and two hex digits, and anything
/// else copied through. Only the URL form decodes; `/user=` and `/passwd=` do
/// not, so the same password is written two different ways.
fn percent_decode(src: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(src.len());
    let mut i = 0;
    while i < src.len() {
        let hex = |b: u8| b.is_ascii_hexdigit().then(|| hex_val(b));
        match (src.get(i), src.get(i + 1).copied(), src.get(i + 2).copied()) {
            (Some(b'%'), Some(h), Some(l)) => match (hex(h), hex(l)) {
                (Some(h), Some(l)) => {
                    out.push(h << 4 | l);
                    i += 3;
                }
                _ => {
                    out.push(src[i]);
                    i += 1;
                }
            },
            _ => {
                out.push(src[i]);
                i += 1;
            }
        }
    }
    out
}

fn hex_val(b: u8) -> u8 {
    match b.is_ascii_alphabetic() {
        true => (b | 0x20) - b'a' + 10,
        false => b - b'0',
    }
}

/// `OPTION_CLEAR` and `OPTION_REPLACE`, applied back to front so the earlier
/// spans keep their offsets.
///
/// Clearing fills the whole span — separator included — with spaces. Replacing
/// fills it and then writes the new text one character in, which is upstream's
/// `wmemcpy(cur+1, option, …)` and is why the span always has room: `cur` is
/// where the previous token ended, so at least one separator is inside it.
fn apply_edits(line: &mut [u8], mut edits: Vec<(std::ops::Range<usize>, Option<Vec<u8>>)>) {
    edits.sort_by_key(|(s, _)| s.start);
    for (span, replacement) in edits.into_iter().rev() {
        line[span.clone()].fill(b' ');
        if let Some(text) = replacement {
            let at = span.start + 1;
            let end = (at + text.len()).min(line.len());
            if at < end {
                line[at..end].copy_from_slice(&text[..end - at]);
            }
        }
    }
}

/// Both halves, in the order upstream runs them: the plugin first, then the
/// terminal over what it left.
///
/// This is what a frontend or a macro's `connect` wants — the two parsers are
/// not independent, since an `ssh://` URL only becomes a host name because
/// TTSSH rewrote it into one.
pub fn parse_both(line: &[u8], max_com_port: u16) -> (super::CommandLine, SshOptions) {
    let (opts, rest) = parse(line);
    (super::CommandLine::parse(&rest, max_com_port), opts)
}

/// [`parse_both`] for the argument of a macro's `connect`, which has no program
/// name in front of it.
pub fn parse_both_argument(arg: &[u8], max_com_port: u16) -> (super::CommandLine, SshOptions) {
    let mut line = b"a ".to_vec();
    line.extend_from_slice(arg);
    let (opts, rest) = parse(&line);
    let mut cmd = super::CommandLine::parse_argument(&rest[2..], max_com_port);
    cmd.raw = line;
    (cmd, opts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmdline::{CommandLine, PortType, DEFAULT_MAX_COM_PORT};

    fn ssh(line: &str) -> (SshOptions, String) {
        let (o, rest) = parse(line.as_bytes());
        (o, String::from_utf8_lossy(&rest).into_owned())
    }

    fn both(line: &str) -> (CommandLine, SshOptions) {
        parse_both(line.as_bytes(), DEFAULT_MAX_COM_PORT)
    }

    fn text(v: &Option<Vec<u8>>) -> String {
        String::from_utf8_lossy(v.as_deref().unwrap_or_default()).into_owned()
    }

    /// The option is taken out of the line, not just read out of it — that is
    /// the whole reason this is a separate pass.
    #[test]
    fn a_consumed_option_is_blanked_out_of_the_line() {
        let (o, rest) = ssh("ttermpro /ssh myhost");
        assert_eq!(o.enabled, Some(true));
        assert_eq!(rest, "ttermpro      myhost");
        // ...and what is left parses as an ordinary Tera Term command line.
        let (cmd, _) = both("ttermpro /ssh myhost");
        assert_eq!(cmd.port_type, Some(PortType::TcpIp));
        assert_eq!(cmd.host_name, b"myhost");
    }

    /// `-` leads a switch here and nowhere else.
    #[test]
    fn a_dash_is_a_switch_for_ttssh_only() {
        assert_eq!(ssh("tt -ssh h").0.enabled, Some(true));
        // Tera Term's own parser never sees a `-`, so this reaches nobody.
        let (cmd, _) = both("tt -nolog h");
        assert!(!cmd.no_log);
    }

    /// `wcsncmp` rather than `_wcsnicmp`: `/SSH` is not an option at all, and
    /// nothing anywhere says so.
    #[test]
    fn the_ssh_prefix_is_case_sensitive_and_fails_silently() {
        let (o, rest) = ssh("tt /SSH myhost");
        assert_eq!(o.enabled, None);
        assert!(o.unknown.is_empty());
        // Left in the line, where Tera Term ignores it too.
        assert_eq!(rest, "tt /SSH myhost");
        // The tails, though, are matched case-insensitively where upstream used
        // `_wcsicmp` — so `/ssh-AUTOLOGIN` works and `/ssh-V` does not.
        assert!(ssh("tt /ssh-AUTOLOGIN").0.try_default_auth);
        assert_eq!(ssh("tt /ssh-V").0.unknown.len(), 1);
    }

    /// `/t=2` is TTSSH's and is deleted; every other `/t=` is Tera Term's and
    /// survives.
    #[test]
    fn t_equals_2_is_consumed_and_t_equals_0_is_not() {
        let (o, rest) = ssh("tt /t=2 h");
        assert_eq!(o.enabled, Some(true));
        assert_eq!(rest, "tt      h");

        let (o, rest) = ssh("tt /t=0 h");
        assert_eq!(o.enabled, Some(false));
        assert_eq!(rest, "tt /t=0 h");
        // ...and Tera Term then reads it, which is the point.
        let (cmd, _) = both("tt /t=0 h");
        assert_eq!(cmd.telnet, Some(false));
    }

    /// `/telnet` switches SSH off and telnet on — through TTSSH, even though
    /// Tera Term never sees the option.
    #[test]
    fn telnet_is_handled_entirely_by_the_plugin() {
        let (o, rest) = ssh("tt /telnet h");
        assert_eq!(o.enabled, Some(false));
        assert!(o.force_telnet);
        assert_eq!(rest, "tt         h");
        let (cmd, _) = both("tt /telnet h");
        assert_eq!(cmd.telnet, None);
        assert_eq!(ssh("tt /nossh h").0.enabled, Some(false));
    }

    /// An `ssh://` URL is rewritten into a host name, which is the only reason
    /// Tera Term can open one.
    #[test]
    fn a_url_becomes_a_host_and_a_port() {
        let (cmd, o) = both("tt ssh://me:secret@myhost/");
        assert_eq!(cmd.host_name, b"myhost");
        assert_eq!(cmd.tcp_port, Some(22));
        assert_eq!(text(&o.username), "me");
        assert_eq!(text(&o.password), "secret");
        assert_eq!(o.enabled, Some(true));

        // An explicit port is kept rather than replaced with 22.
        let (cmd, _) = both("tt ssh://myhost:2222/");
        assert_eq!(
            (cmd.host_name.clone(), cmd.tcp_port),
            (b"myhost".to_vec(), Some(2222))
        );

        // The digit before `://` is the protocol version.
        assert_eq!(both("tt ssh1://h/").1.protocol_version, Some(1));
        assert_eq!(both("tt slogin2://h/").1.protocol_version, Some(2));
        assert_eq!(both("tt slogin://h/").1.protocol_version, None);

        // A bracketed IPv6 literal gets its `:22` outside the brackets, and
        // Tera Term's own parser then takes them off.
        let (cmd, _) = both("tt ssh://[3ffe::1]/");
        assert_eq!(
            (cmd.host_name.clone(), cmd.tcp_port),
            (b"3ffe::1".to_vec(), Some(22))
        );
        let (cmd, _) = both("tt ssh://[3ffe::1]:22/");
        assert_eq!(cmd.host_name, b"3ffe::1");

        // A percent-encoded password, which is the only way to put a `@` or a
        // space in one.
        assert_eq!(text(&both("tt ssh://u:a%20b%40c@h/").1.password), "a b@c");
        // A stray `%` is copied through rather than eaten.
        assert_eq!(text(&both("tt ssh://u:100%@h/").1.password), "100%");
    }

    /// `user@host` with no scheme: the user part is blanked and the host stays
    /// put, which is why the line has a hole in the middle of it.
    #[test]
    fn user_at_host_leaves_the_host_where_it_was() {
        let (o, rest) = ssh("tt me@myhost");
        assert_eq!(text(&o.username), "me");
        // The name and its `@` become spaces where they stood; the host does
        // not move up.
        assert_eq!(rest, "tt    myhost");
        // No `/ssh`, so this is still a telnet session with a user name
        // attached — upstream takes the name anyway.
        let (cmd, o) = both("tt me@myhost");
        assert_eq!(cmd.host_name, b"myhost");
        assert_eq!(o.enabled, None);
    }

    /// The automatic-login family, which is what a `connect` in a macro uses.
    #[test]
    fn the_documented_autologin_command_line() {
        let (cmd, o) = both(r#"tt /ssh /auth=password /user=nike /passwd="a b""c" myhost"#);
        assert_eq!(cmd.host_name, b"myhost");
        assert_eq!(o.enabled, Some(true));
        assert!(o.auto_login);
        assert_eq!(o.auth_method, Some(AuthMethod::Password));
        assert_eq!(text(&o.username), "nike");
        // The doubled quote is one literal quote, out of the tokeniser.
        assert_eq!(text(&o.password), r#"a b"c"#);

        let (_, o) = both(r"tt /ssh /auth=publickey /user=foo /keyfile=d:\tmp\id_rsa myhost");
        assert_eq!(o.auth_method, Some(AuthMethod::PublicKey));
        assert_eq!(text(&o.key_file), r"d:\tmp\id_rsa");

        // `challenge` and `keyboard-interactive` are one method.
        assert_eq!(
            both("tt /auth=challenge").1.auth_method,
            Some(AuthMethod::KeyboardInteractive)
        );
        // An unrecognised method leaves the method unset and automatic login
        // **on**, which is the bare `// TODO:` arm.
        let (_, o) = both("tt /auth=magic");
        assert_eq!(o.auth_method, None);
        assert!(o.auto_login);

        // `/ask4passwd` cancels automatic login whichever side of `/auth=` it
        // falls, because the test runs after every switch.
        assert!(!both("tt /auth=password /ask4passwd").1.auto_login);
        assert!(!both("tt /ask4passwd /auth=password").1.auto_login);
    }

    /// Port forwarding, where one option can carry several specs and each keeps
    /// its letter.
    #[test]
    fn forwarding_specs_keep_their_letter_and_may_be_a_list() {
        let (o, _) = ssh("tt /ssh-L1234:localhost:5678");
        assert_eq!(o.forwarding, [b"L1234:localhost:5678".to_vec()]);
        // **The letter is given once and applies to every spec** —
        // `option2[0]` is written before the loop and the index resets to 1
        // rather than 0, which is also what `ttssh.html` documents. Repeating it
        // by hand gets it doubled, silently.
        let (o, _) = ssh("tt /ssh-R110:mail:110,25:mail:25");
        assert_eq!(
            o.forwarding,
            [b"R110:mail:110".to_vec(), b"R25:mail:25".to_vec()]
        );
        assert_eq!(
            ssh("tt /ssh-R110:mail:110,R25:mail:25").0.forwarding[1],
            b"RR25:mail:25"
        );
        // A `;` separates as well as a `,`, but **only inside quotes**: an
        // unquoted `;` is where the tokeniser stops reading the line, so
        // without them the second spec is a comment.
        let (o, _) = ssh(r#"tt "/ssh-D1080;1081""#);
        assert_eq!(o.forwarding, [b"D1080".to_vec(), b"D1081".to_vec()]);
        let (o, _) = ssh("tt /ssh-D1080;1081");
        assert_eq!(o.forwarding, [b"D1080".to_vec()]);
        // `/ssh-X` adds an `X` entry and takes a display after it.
        let (o, _) = ssh("tt /ssh-Xlocalhost:0");
        assert_eq!(o.forwarding, [b"X".to_vec()]);
        assert_eq!(text(&o.x11_display), "localhost:0");
        // ...and with nothing after it the display is empty rather than absent,
        // because upstream tests a pointer where it meant the character.
        let (o, _) = ssh("tt /ssh-X");
        assert_eq!(o.x11_display, Some(Vec::new()));
    }

    /// The odds and ends, each with the default its absence means.
    #[test]
    fn the_rest_of_the_ssh_switches() {
        let (o, _) = ssh("tt /ssh-v /ssh-A /ssh-N /ssh-C=9 /ssh-subsystem=sftp /ssh2");
        assert!(o.verbose && o.no_session);
        assert_eq!(o.forward_agent, Some(true));
        assert_eq!(o.compression_level, Some(9));
        assert_eq!(text(&o.subsystem), "sftp");
        assert_eq!((o.enabled, o.protocol_version), (Some(true), Some(2)));

        assert_eq!(ssh("tt /ssh-a").0.forward_agent, Some(false));
        assert_eq!(ssh("tt /ssh-C").0.compression_level, Some(6));
        assert_eq!(ssh("tt /ssh-c").0.compression_level, Some(0));
        // Clamped rather than refused.
        assert_eq!(ssh("tt /ssh-C=99").0.compression_level, Some(9));
        assert_eq!(ssh("tt /ssh-C=-5").0.compression_level, Some(0));

        // `agentconfirm` is off for five spellings and on for everything else,
        // including nothing at all.
        for off in ["off", "no", "false", "0", "n", "OFF"] {
            assert_eq!(
                ssh(&format!("tt /ssh-agentconfirm={off}"))
                    .0
                    .forward_agent_confirm,
                Some(false)
            );
        }
        assert_eq!(
            ssh("tt /ssh-agentconfirm=yes").0.forward_agent_confirm,
            Some(true)
        );
        assert_eq!(
            ssh("tt /ssh-agentconfirm=").0.forward_agent_confirm,
            Some(true)
        );

        assert_eq!(ssh("tt /ssh-icon=green").0.icon, Some(Icon::Green));
        assert_eq!(ssh("tt /ssh-icon=securett_flat").0.icon, Some(Icon::Flat));
        assert_eq!(ssh("tt /ssh-icon=old").0.icon, Some(Icon::Yellow));
        assert_eq!(ssh("tt /ssh-icon=nonsense").0.icon, Some(Icon::Default));

        // A bare `/1` sets the version without switching SSH on; `/ssh1` does
        // both.
        let (o, _) = ssh("tt /1");
        assert_eq!((o.enabled, o.protocol_version), (None, Some(1)));

        assert!(ssh("tt /nosecuritywarning").0.no_known_hosts_check);
    }

    /// An unknown `/ssh…` is the one thing either parser complains about.
    #[test]
    fn an_unknown_ssh_option_is_reported_rather_than_ignored() {
        let (o, rest) = ssh("tt /ssh-nonsense h");
        assert_eq!(o.unknown, [b"/ssh-nonsense".to_vec()]);
        // Consumed all the same, so Tera Term never sees it either.
        assert_eq!(rest, "tt               h");
        // A non-SSH switch is not reported and not consumed.
        let (o, rest) = ssh("tt /nolog h");
        assert!(o.unknown.is_empty());
        assert_eq!(rest, "tt /nolog h");
    }

    /// The split-`argv` path, where there is no line to blank and the two
    /// halves compose through the token list instead.
    #[test]
    fn parse_args_composes_through_tokens_rather_than_a_line() {
        let (o, left) = parse_args(["/ssh", "/user=me", "myhost"]);
        assert_eq!(o.enabled, Some(true));
        assert_eq!(text(&o.username), "me");
        // Consumed tokens are gone rather than blanked.
        assert_eq!(left, [b"myhost".to_vec()]);

        // A rewritten one arrives trimmed, because there is no line for the
        // padding to sit in.
        let (o, left) = parse_args(["me@myhost"]);
        assert_eq!(text(&o.username), "me");
        assert_eq!(left, [b"myhost".to_vec()]);
        let (_, left) = parse_args(["ssh://u:p@myhost/"]);
        assert_eq!(left, [b"myhost:22".to_vec()]);

        // ...and what is left is what Tera Term's own split parser then reads.
        let (o, left) = parse_args(["/ssh", "/t=0", "myhost"]);
        let cmd = CommandLine::from_args(&left, DEFAULT_MAX_COM_PORT);
        assert_eq!(o.enabled, Some(false), "the later /t=0 wins, as in a line");
        assert_eq!(cmd.telnet, Some(false), "and /t=0 survived for Tera Term");
        assert_eq!(cmd.host_name, b"myhost");
    }

    /// The two paths agree about everything except the whitespace they cannot
    /// share, which is the claim that makes the token path safe.
    #[test]
    fn the_line_and_the_token_paths_find_the_same_options() {
        for line in [
            "tt /ssh /auth=publickey /user=me /keyfile=k myhost",
            "tt ssh://me@myhost:2222/",
            "tt me@myhost /nossh",
            "tt /ssh-L1:h:2,3:h:4 /ssh-C=9 /ssh-N h",
            "tt /ssh-f=my.ini /F=my.ini h",
        ] {
            let (a, rest) = ssh(line);
            let split: Vec<&str> = line.split(' ').skip(1).collect();
            let (b, toks) = parse_args(&split);
            assert_eq!(a, b, "{line}");
            let joined: Vec<String> = toks
                .iter()
                .map(|t| String::from_utf8_lossy(t).into_owned())
                .collect();
            assert_eq!(
                rest.split_whitespace().skip(1).collect::<Vec<_>>(),
                joined,
                "{line}"
            );
        }
    }

    /// The first pass, which is a settings file and one option that is read by
    /// both halves.
    #[test]
    fn the_options_file_pass_runs_first() {
        let (o, rest) = ssh("tt /ssh-f=my.ini h");
        assert_eq!(
            o.options_files,
            [OptionsFile {
                path: b"my.ini".to_vec(),
                consume: false
            }]
        );
        assert_eq!(rest, "tt               h");

        // `/ssh-consume=` deletes the file after reading it.
        let (o, _) = ssh("tt /ssh-consume=tmp.ini");
        assert!(o.options_files[0].consume);

        // `/F=` is read by TTSSH *and* left in the line, because the same file
        // holds both halves' settings.
        let (cmd, o) = both("tt /F=my.ini h");
        assert_eq!(
            text(&o.options_files.first().map(|f| f.path.clone())),
            "my.ini"
        );
        assert_eq!(text(&cmd.setup_file), "my.ini");
    }

    /// `connect`'s argument, through both parsers — the form a macro uses to
    /// open an SSH session.
    #[test]
    fn a_connect_argument_goes_through_both_halves() {
        let (cmd, o) = parse_both_argument(
            b"myhost:22 /ssh /auth=password /user=me /passwd=pw",
            DEFAULT_MAX_COM_PORT,
        );
        assert_eq!(cmd.host_name, b"myhost");
        assert_eq!(cmd.tcp_port, Some(22));
        assert_eq!(cmd.port_type, Some(PortType::TcpIp));
        assert_eq!(o.enabled, Some(true));
        assert_eq!(text(&o.username), "me");
        assert_eq!(text(&o.password), "pw");
        // The raw line keeps the dummy program name, which is what upstream
        // hands the parser.
        assert!(cmd.raw.starts_with(b"a myhost"));
    }
}

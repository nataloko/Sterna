//! `~/.ssh/config` — so that typing an alias the user already has works.
//!
//! This is the adoption lever `PLAN.md` names. A Linux user's `~/.ssh/config`
//! already says which key, which account and which port go with `myrouter`;
//! Tera Term keeps that in its own store and its own dialogs, so switching
//! means entering everything twice and keeping it in step. Reading OpenSSH's
//! file is the whole of the difference.
//!
//! **The trap is that the first value wins, not the last.** OpenSSH says so in
//! one sentence in `ssh_config(5)` and nearly every other config format in
//! existence does the opposite. A `Host *` block at the *top* of a file
//! overrides everything below it, which is why the convention is to put it at
//! the bottom. Getting this backwards does not fail loudly — it silently
//! applies the wrong user or the wrong key to hosts that had a perfectly good
//! specific block, and the user's setup "just doesn't work" for no visible
//! reason.
//!
//! `IdentityFile` is the exception: it accumulates in file order rather than
//! taking the first, because a user with three keys means all three.
//!
//! What is read and acted on is what Stage 1 connects with. Everything else a
//! real config contains is *recorded* rather than dropped — see
//! [`Resolved::unsupported`] — because a silently ignored `ProxyJump` is a
//! connection to the wrong machine, and a silently ignored `ProxyCommand` is a
//! connection that fails with no clue why.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::pattern::matches_list;
use crate::error::Result;

/// One `Host` or `Match` block, with the options that follow it.
#[derive(Clone, Debug)]
struct Block {
    criteria: Criteria,
    options: Vec<(String, String)>,
}

#[derive(Clone, Debug)]
enum Criteria {
    /// Options before any `Host` line. OpenSSH applies these to everything.
    Global,
    Host(Vec<String>),
    /// `Match all`, and `Match host <patterns>` / `Match user <patterns>` —
    /// the subset that can be decided without running anything.
    Match(Vec<MatchTerm>),
    /// A `Match` this cannot evaluate: `exec`, `canonical`, `localnetwork`.
    /// Never matches, and says so.
    ///
    /// `Match exec` in particular is a decision rather than a default:
    /// resolving a config would run an arbitrary shell command, every time,
    /// merely because the user opened the connect dialog. OpenSSH does that
    /// knowingly; a GUI that enumerates hosts to fill a dropdown must not.
    Unevaluatable(String),
}

#[derive(Clone, Debug)]
enum MatchTerm {
    All,
    Host(Vec<String>),
    OriginalHost(Vec<String>),
    User(Vec<String>),
    LocalUser(Vec<String>),
}

/// A parsed `ssh_config`, ready to be asked about a host.
#[derive(Clone, Debug, Default)]
pub struct SshConfig {
    blocks: Vec<Block>,
}

/// What the config says about one host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolved {
    /// What to actually connect to — `HostName` if there was one, otherwise
    /// the alias as typed.
    pub host_name: String,
    /// The alias as typed, which is what `%n` expands to and what
    /// `known_hosts` is keyed on when `HostKeyAlias` is absent.
    pub original_host: String,
    pub user: Option<String>,
    pub port: Option<u16>,
    /// In file order, all of them. `IdentityFile` accumulates.
    pub identity_files: Vec<PathBuf>,
    /// `IdentitiesOnly yes` — do not offer whatever the agent happens to hold.
    pub identities_only: bool,
    /// `IdentityAgent none` means do not talk to an agent at all.
    pub use_agent: bool,
    pub user_known_hosts_files: Vec<PathBuf>,
    pub hash_known_hosts: bool,
    pub connect_timeout: Option<Duration>,
    pub server_alive_interval: Option<Duration>,
    pub strict_host_key_checking: Option<StrictHostKeyChecking>,
    /// The effective first `ProxyCommand` or `ProxyJump` asks for a relay this
    /// client cannot run. False for their explicit `none` spelling.
    pub requires_proxy: bool,
    /// True when the config names a pre-2020 algorithm anywhere — a `+`-form
    /// `KexAlgorithms`, `Ciphers`, `MACs`, `HostKeyAlgorithms` or
    /// `PubkeyAcceptedAlgorithms` mentioning SHA-1, CBC or `ssh-dss`.
    ///
    /// A user who has written that has already told OpenSSH they are talking
    /// to old equipment, and making them find our own switch as well is
    /// exactly the "configured twice" problem this module exists to remove.
    /// It only ever turns legacy *on*: nothing in a config file should be able
    /// to quietly narrow what we offer.
    pub legacy: bool,
    /// Keywords that were present and are not acted on, lowercased and
    /// deduplicated. Shown rather than dropped.
    pub unsupported: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StrictHostKeyChecking {
    /// Refuse anything not already recorded.
    Yes,
    /// Record a new host without asking; refuse a changed one. OpenSSH's
    /// default since 8.5, and the sensible one for a console tool.
    AcceptNew,
    /// Ask. What a GUI does anyway.
    Ask,
    /// Accept anything. Present because configs contain it, and a client that
    /// ignores it prompts about a host the user told it not to care about.
    No,
}

impl Default for Resolved {
    fn default() -> Resolved {
        Resolved {
            host_name: String::new(),
            original_host: String::new(),
            user: None,
            port: None,
            identity_files: Vec::new(),
            identities_only: false,
            use_agent: true,
            user_known_hosts_files: Vec::new(),
            hash_known_hosts: false,
            connect_timeout: None,
            server_alive_interval: None,
            strict_host_key_checking: None,
            requires_proxy: false,
            legacy: false,
            unsupported: Vec::new(),
        }
    }
}

/// Keywords this understands. Anything else that appears is reported through
/// [`Resolved::unsupported`] rather than silently dropped.
const KNOWN: &[&str] = &[
    "hostname",
    "user",
    "port",
    "identityfile",
    "identitiesonly",
    "identityagent",
    "userknownhostsfile",
    "hashknownhosts",
    "connecttimeout",
    "serveraliveinterval",
    "stricthostkeychecking",
    "ciphers",
    "kexalgorithms",
    "macs",
    "hostkeyalgorithms",
    "pubkeyacceptedalgorithms",
    "pubkeyacceptedkeytypes",
    // Present, understood, and simply not interesting to a terminal: reading
    // them without acting is correct, so they are not "unsupported".
    "addkeystoagent",
    "compression",
    "loglevel",
    "batchmode",
    "checkhostip",
    "serveralivecountmax",
    "tcpkeepalive",
    "visualhostkey",
    "sendenv",
    "controlmaster",
    "controlpath",
    "controlpersist",
];

impl SshConfig {
    /// Read `~/.ssh/config` and then `/etc/ssh/ssh_config`, in that order.
    ///
    /// A missing file is not an error — most machines have no system config
    /// worth the name, and a first-time user has no personal one.
    pub fn user_default() -> Result<SshConfig> {
        let mut config = SshConfig::default();
        if let Some(home) = std::env::home_dir() {
            config.read_into(&home.join(".ssh").join("config"), 0)?;
        }
        config.read_into(Path::new("/etc/ssh/ssh_config"), 0)?;
        Ok(config)
    }

    pub fn from_files(paths: &[PathBuf]) -> Result<SshConfig> {
        let mut config = SshConfig::default();
        for p in paths {
            config.read_into(p, 0)?;
        }
        Ok(config)
    }

    /// Parse `text` as if it were a file at `origin`, which is only used to
    /// resolve relative `Include` paths.
    pub fn parse(text: &str, origin: &Path) -> SshConfig {
        let mut config = SshConfig::default();
        config.parse_into(text, origin, 0);
        config
    }

    fn read_into(&mut self, path: &Path, depth: usize) -> Result<()> {
        match fs::read_to_string(path) {
            Ok(text) => {
                self.parse_into(&text, path, depth);
                Ok(())
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            // A config that exists and cannot be read is worth reporting: the
            // alternative is connecting as the wrong user to the wrong port
            // and blaming the server.
            Err(e) => Err(e.into()),
        }
    }

    fn parse_into(&mut self, text: &str, origin: &Path, depth: usize) {
        let mut current = Block {
            criteria: Criteria::Global,
            options: Vec::new(),
        };
        for line in text.lines() {
            let Some((keyword, value)) = split_option(line) else {
                continue;
            };
            match keyword.as_str() {
                "host" => {
                    self.blocks.push(std::mem::replace(
                        &mut current,
                        Block {
                            criteria: Criteria::Host(split_args(&value)),
                            options: Vec::new(),
                        },
                    ));
                }
                "match" => {
                    self.blocks.push(std::mem::replace(
                        &mut current,
                        Block {
                            criteria: parse_match(&value),
                            options: Vec::new(),
                        },
                    ));
                }
                "include" => {
                    // Processed where it appears, so the first-value-wins rule
                    // sees the included options in the right place. Depth is
                    // capped the way OpenSSH caps it, because a file that
                    // includes itself is otherwise an infinite loop at
                    // dialog-open time.
                    if depth < 16 {
                        self.blocks.push(std::mem::replace(
                            &mut current,
                            Block {
                                criteria: Criteria::Global,
                                options: Vec::new(),
                            },
                        ));
                        // The include inherits the enclosing block's criteria,
                        // which is what makes `Host x` / `Include extra` work.
                        let inherited = self
                            .blocks
                            .last()
                            .map(|b| b.criteria.clone())
                            .unwrap_or(Criteria::Global);
                        for path in include_paths(&value, origin) {
                            let before = self.blocks.len();
                            let _ = self.read_into(&path, depth + 1);
                            // An included file's leading `Global` block is
                            // really the including block's, so it takes those
                            // criteria rather than applying to every host.
                            if let Some(b) = self.blocks.get_mut(before) {
                                if matches!(b.criteria, Criteria::Global) {
                                    b.criteria = inherited.clone();
                                }
                            }
                        }
                        current.criteria = inherited;
                    }
                }
                _ => current.options.push((keyword, value)),
            }
        }
        self.blocks.push(current);
    }

    /// What the config says about `alias`, with the tokens expanded.
    ///
    /// `user` overrides anything the file says, because a `user@host` typed
    /// into a dialog is more specific than a pattern in a file — and it has to
    /// be known before `Match user` can be evaluated, which is why it is an
    /// argument rather than something read out afterwards.
    pub fn resolve(&self, alias: &str, user: Option<&str>) -> Resolved {
        let local_user = std::env::var("USER").unwrap_or_default();
        let mut values: HashMap<String, String> = HashMap::new();
        let mut identity_files: Vec<String> = Vec::new();
        let mut known_hosts_files: Vec<String> = Vec::new();
        let mut unsupported: Vec<String> = Vec::new();
        // The two proxy directives compete: whichever is obtained first makes
        // later instances of either irrelevant. `none` is a real value and
        // means a direct connection, not an unsupported relay.
        let mut requires_proxy: Option<bool> = None;
        let mut legacy = false;

        // The effective host name changes as blocks are read — a `HostName`
        // in one block is what a later `Match host` is matched against — so it
        // is tracked rather than computed at the end.
        let mut host_name = alias.to_string();

        for block in &self.blocks {
            let applies = match &block.criteria {
                Criteria::Global => true,
                Criteria::Host(patterns) => {
                    matches_list(patterns.iter().map(String::as_str), alias)
                }
                Criteria::Match(terms) => terms.iter().all(|t| match t {
                    MatchTerm::All => true,
                    MatchTerm::Host(p) => matches_list(p.iter().map(String::as_str), &host_name),
                    MatchTerm::OriginalHost(p) => matches_list(p.iter().map(String::as_str), alias),
                    MatchTerm::User(p) => matches_list(
                        p.iter().map(String::as_str),
                        &effective_user(user, &values, &local_user),
                    ),
                    MatchTerm::LocalUser(p) => {
                        matches_list(p.iter().map(String::as_str), &local_user)
                    }
                }),
                Criteria::Unevaluatable(what) => {
                    push_once(&mut unsupported, &format!("Match {what}"));
                    false
                }
            };
            if !applies {
                continue;
            }
            for (keyword, value) in &block.options {
                match keyword.as_str() {
                    // The two that accumulate rather than take the first.
                    "identityfile" => identity_files.extend(split_args(value)),
                    "userknownhostsfile" => known_hosts_files.extend(split_args(value)),
                    "proxycommand" | "proxyjump" => {
                        if requires_proxy.is_none() {
                            let required = !value.eq_ignore_ascii_case("none");
                            requires_proxy = Some(required);
                            if required {
                                push_once(&mut unsupported, keyword);
                            }
                        }
                    }
                    "ciphers"
                    | "kexalgorithms"
                    | "macs"
                    | "hostkeyalgorithms"
                    | "pubkeyacceptedalgorithms"
                    | "pubkeyacceptedkeytypes" => {
                        legacy |= names_legacy_algorithms(value);
                    }
                    _ => {
                        if !KNOWN.contains(&keyword.as_str()) {
                            push_once(&mut unsupported, keyword);
                            continue;
                        }
                        // First wins. `entry` rather than `insert`, and that
                        // one word is the whole semantic difference from every
                        // other config format.
                        values
                            .entry(keyword.clone())
                            .or_insert_with(|| value.clone());
                    }
                }
                if keyword == "hostname" {
                    host_name = values.get("hostname").cloned().unwrap_or(host_name);
                }
            }
        }

        let port = values.get("port").and_then(|p| p.parse::<u16>().ok());
        let resolved_user = effective_user(user, &values, &local_user);

        // Inside `HostName` itself, `%h` is the name that was *typed* — there
        // is no resolved one yet. Everywhere else it is the result. Expanding
        // in one pass with a single meaning gets `HostName %h.example.com`,
        // the most common use of a token in a real config, wrong.
        let host_name = expand_tokens(
            &host_name,
            alias,
            alias,
            port.unwrap_or(22),
            &resolved_user,
            &local_user,
        );
        let expand = |s: &str| {
            expand_tokens(
                s,
                alias,
                &host_name,
                port.unwrap_or(22),
                &resolved_user,
                &local_user,
            )
        };

        Resolved {
            host_name: host_name.clone(),
            original_host: alias.to_string(),
            user: user
                .map(|u| u.to_string())
                .or_else(|| values.get("user").cloned()),
            port,
            identity_files: identity_files
                .iter()
                .map(|f| expand_path(&expand(f)))
                .collect(),
            identities_only: yes(values.get("identitiesonly")),
            use_agent: values
                .get("identityagent")
                .map(|a| a != "none")
                .unwrap_or(true),
            user_known_hosts_files: known_hosts_files
                .iter()
                .map(|f| expand_path(&expand(f)))
                .collect(),
            hash_known_hosts: yes(values.get("hashknownhosts")),
            connect_timeout: values
                .get("connecttimeout")
                .and_then(|v| v.parse().ok())
                .map(Duration::from_secs),
            server_alive_interval: values
                .get("serveraliveinterval")
                .and_then(|v| v.parse().ok())
                .filter(|s| *s > 0)
                .map(Duration::from_secs),
            strict_host_key_checking: values.get("stricthostkeychecking").and_then(|v| {
                match v.to_ascii_lowercase().as_str() {
                    "yes" => Some(StrictHostKeyChecking::Yes),
                    "accept-new" => Some(StrictHostKeyChecking::AcceptNew),
                    "ask" => Some(StrictHostKeyChecking::Ask),
                    "no" | "off" => Some(StrictHostKeyChecking::No),
                    _ => None,
                }
            }),
            requires_proxy: requires_proxy.unwrap_or(false),
            legacy,
            unsupported,
        }
    }

    /// Every `Host` pattern with no wildcard in it — the aliases a user could
    /// sensibly be offered in a dropdown.
    ///
    /// Wildcards are excluded because `*` is not somewhere to connect, and
    /// negations because `!bastion` names a host the block is *about* rather
    /// than one it configures.
    ///
    /// So is anything that would have to be dialled through a `ProxyCommand`
    /// or a `ProxyJump`, because this program cannot honour either. The list
    /// is drawn from the system config as well as the user's, and a Linux
    /// desktop has entries of exactly that shape in it whether or not anybody
    /// asked: systemd ships `Host .host machine/.host` with a `ProxyCommand`
    /// onto an `AF_UNIX` socket, and neither name has a wildcard to catch it.
    /// Offered, they would resolve as ordinary DNS names and fail.
    pub fn aliases(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for block in &self.blocks {
            let Criteria::Host(patterns) = &block.criteria else {
                continue;
            };
            for p in patterns {
                if p.contains('*') || p.contains('?') || p.starts_with('!') {
                    continue;
                }
                if !out.iter().any(|a| a == p) {
                    out.push(p.clone());
                }
            }
        }
        out.retain(|a| !self.needs_a_proxy(a));
        out
    }

    /// Whether reaching `alias` means running something this program does not
    /// run. Asked of the *resolved* config rather than of the block that names
    /// it, so a `Host *` carrying the `ProxyCommand` counts too.
    fn needs_a_proxy(&self, alias: &str) -> bool {
        self.resolve(alias, None).requires_proxy
    }
}

/// Who the connection will actually be for: what was typed, else what the
/// file says, else whoever is running this. Needed *during* resolution, not
/// after, because `Match user` is decided against it.
fn effective_user(
    typed: Option<&str>,
    values: &HashMap<String, String>,
    local_user: &str,
) -> String {
    typed
        .map(|u| u.to_string())
        .or_else(|| values.get("user").cloned())
        .unwrap_or_else(|| local_user.to_string())
}

fn push_once(v: &mut Vec<String>, s: &str) {
    if !v.iter().any(|x| x == s) {
        v.push(s.to_string());
    }
}

fn yes(v: Option<&String>) -> bool {
    matches!(v.map(|s| s.to_ascii_lowercase()).as_deref(), Some("yes"))
}

/// `keyword value`, `keyword=value`, or nothing.
///
/// The keyword is lowercased because OpenSSH matches it case-insensitively;
/// the value is not, because host names, paths and user names are not.
fn split_option(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    // `Host=x` is as legal as `Host x`, and configs written by tools use it.
    let cut = line
        .find(|c: char| c.is_whitespace() || c == '=')
        .unwrap_or(line.len());
    let (keyword, rest) = line.split_at(cut);
    let value = rest.trim_start_matches([' ', '\t', '=']).trim_end();
    Some((keyword.to_ascii_lowercase(), value.to_string()))
}

/// Whitespace-separated, with double quotes grouping anything containing
/// spaces — which is how a path under `~/My Keys/` gets written.
fn split_args(value: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut any = false;
    for c in value.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                any = true;
            }
            c if c.is_whitespace() && !quoted => {
                if any {
                    out.push(std::mem::take(&mut current));
                    any = false;
                }
            }
            c => {
                current.push(c);
                any = true;
            }
        }
    }
    if any {
        out.push(current);
    }
    out
}

fn parse_match(value: &str) -> Criteria {
    let args = split_args(value);
    let mut terms = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let keyword = args[i].to_ascii_lowercase();
        let patterns = || args.get(i + 1).map(|a| comma_list(a)).unwrap_or_default();
        match keyword.as_str() {
            "all" => {
                terms.push(MatchTerm::All);
                i += 1;
            }
            "host" => {
                terms.push(MatchTerm::Host(patterns()));
                i += 2;
            }
            "originalhost" => {
                terms.push(MatchTerm::OriginalHost(patterns()));
                i += 2;
            }
            "user" => {
                terms.push(MatchTerm::User(patterns()));
                i += 2;
            }
            "localuser" => {
                terms.push(MatchTerm::LocalUser(patterns()));
                i += 2;
            }
            // `final` re-evaluates after everything else is resolved. Treating
            // it as `all` is close enough for a single pass and much closer
            // than ignoring the block.
            "final" | "canonical" => {
                terms.push(MatchTerm::All);
                i += 1;
            }
            other => return Criteria::Unevaluatable(other.to_string()),
        }
    }
    if terms.is_empty() {
        Criteria::Unevaluatable("(empty)".to_string())
    } else {
        Criteria::Match(terms)
    }
}

fn comma_list(s: &str) -> Vec<String> {
    s.split(',')
        .filter(|p| !p.is_empty())
        .map(|p| p.to_string())
        .collect()
}

/// Relative includes resolve against `~/.ssh` for a user config and `/etc/ssh`
/// for a system one — the directory the including file is in, which is the
/// same rule stated more simply.
fn include_paths(value: &str, origin: &Path) -> Vec<PathBuf> {
    let base = origin.parent().unwrap_or(Path::new("."));
    let mut out = Vec::new();
    for arg in split_args(value) {
        let expanded = expand_path(&arg);
        let path = if expanded.is_absolute() {
            expanded
        } else {
            base.join(expanded)
        };
        // `Include conf.d/*` is the common shape, and a literal path with no
        // glob characters must still work when the file is absent.
        if let Some(matches) = glob_dir(&path) {
            out.extend(matches);
        } else {
            out.push(path);
        }
    }
    out
}

/// Expand a single-level `*`/`?` in the final component. Deliberately not a
/// full recursive glob: `Include conf.d/*.conf` is what configs contain, and
/// walking a tree from a config parse is a surprise nobody asked for.
fn glob_dir(path: &Path) -> Option<Vec<PathBuf>> {
    let name = path.file_name()?.to_str()?;
    if !name.contains('*') && !name.contains('?') {
        return None;
    }
    let dir = path.parent()?;
    let mut out: Vec<PathBuf> = fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_str()
                .is_some_and(|n| super::pattern::matches_glob(name, n))
        })
        .map(|e| e.path())
        .collect();
    // `read_dir` order is the filesystem's, and first-value-wins makes the
    // order load-bearing. Sorting is what OpenSSH does and what a user reading
    // `ls conf.d/` would expect.
    out.sort();
    Some(out)
}

fn expand_path(s: &str) -> PathBuf {
    if let Some(rest) = s.strip_prefix("~/") {
        if let Some(home) = std::env::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(s)
}

/// `%h %n %p %r %u %d %L %l %%`, which is what appears in real configs.
fn expand_tokens(
    s: &str,
    original_host: &str,
    host_name: &str,
    port: u16,
    user: &str,
    local_user: &str,
) -> String {
    if !s.contains('%') {
        return s.to_string();
    }
    let home = std::env::home_dir()
        .map(|h| h.display().to_string())
        .unwrap_or_default();
    let hostname = local_hostname();
    let short = hostname.split('.').next().unwrap_or(&hostname).to_string();

    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('%') => out.push('%'),
            Some('h') => out.push_str(host_name),
            Some('n') => out.push_str(original_host),
            Some('p') => out.push_str(&port.to_string()),
            Some('r') => out.push_str(user),
            Some('u') => out.push_str(local_user),
            Some('d') => out.push_str(&home),
            Some('L') => out.push_str(&short),
            Some('l') => out.push_str(&hostname),
            // An unknown token is left alone rather than eaten: a path
            // silently missing two characters is harder to diagnose than one
            // that still shows what was written.
            Some(other) => {
                out.push('%');
                out.push(other);
            }
            None => out.push('%'),
        }
    }
    out
}

fn local_hostname() -> String {
    fs::read_to_string("/proc/sys/kernel/hostname")
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// Whether an algorithm list names something from before 2020.
///
/// Substring matching on purpose: the list may be `+diffie-hellman-group14-sha1`
/// or a full replacement, and either way the presence of the name is the
/// signal. False positives cost one wider offer; false negatives cost a device
/// that will not answer.
fn names_legacy_algorithms(value: &str) -> bool {
    let v = value.to_ascii_lowercase();
    ["sha1", "-cbc", "ssh-dss", "ssh-rsa"]
        .iter()
        .any(|needle| v.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> SshConfig {
        SshConfig::parse(text, Path::new("/home/nobody/.ssh/config"))
    }

    #[test]
    fn the_first_value_wins_not_the_last() {
        // The one that breaks every other config format's intuition, and the
        // reason `Host *` conventionally goes at the bottom.
        let c =
            parse("Host *\n  User first\n\nHost web\n  User second\n  HostName web.example.com\n");
        let r = c.resolve("web", None);
        assert_eq!(r.user.as_deref(), Some("first"));
        assert_eq!(r.host_name, "web.example.com");
    }

    #[test]
    fn a_specific_block_wins_when_it_comes_first() {
        let c = parse("Host web\n  User me\n\nHost *\n  User fallback\n");
        assert_eq!(c.resolve("web", None).user.as_deref(), Some("me"));
        assert_eq!(c.resolve("other", None).user.as_deref(), Some("fallback"));
    }

    #[test]
    fn identity_files_accumulate_rather_than_taking_the_first() {
        let c = parse("Host *\n  IdentityFile /a\n  IdentityFile /b\nHost x\n  IdentityFile /c\n");
        let r = c.resolve("x", None);
        assert_eq!(
            r.identity_files,
            vec![
                PathBuf::from("/a"),
                PathBuf::from("/b"),
                PathBuf::from("/c")
            ]
        );
    }

    #[test]
    fn a_typed_user_beats_the_file() {
        let c = parse("Host web\n  User configured\n");
        assert_eq!(
            c.resolve("web", Some("typed")).user.as_deref(),
            Some("typed")
        );
    }

    #[test]
    fn keywords_are_case_insensitive_and_may_use_equals() {
        let c = parse("HOST web\n  hostname=web.example.com\n  PoRt = 2222\n");
        let r = c.resolve("web", None);
        assert_eq!(r.host_name, "web.example.com");
        assert_eq!(r.port, Some(2222));
    }

    #[test]
    fn options_before_any_host_line_apply_to_everything() {
        let c = parse("Port 2222\n\nHost web\n  HostName web.example.com\n");
        assert_eq!(c.resolve("anything", None).port, Some(2222));
    }

    #[test]
    fn patterns_negate() {
        let c = parse("Host *.example.com !secret.example.com\n  User shared\n");
        assert_eq!(
            c.resolve("web.example.com", None).user.as_deref(),
            Some("shared")
        );
        assert_eq!(c.resolve("secret.example.com", None).user, None);
    }

    #[test]
    fn match_host_sees_the_hostname_a_previous_block_set() {
        // This is why the effective host name is tracked while resolving
        // rather than computed at the end.
        let c = parse(
            "Host web\n  HostName web.internal\n\nMatch host web.internal\n  User internal\n",
        );
        assert_eq!(c.resolve("web", None).user.as_deref(), Some("internal"));
    }

    #[test]
    fn match_originalhost_sees_the_alias_as_typed() {
        let c = parse("Host web\n  HostName 10.0.0.1\n\nMatch originalhost web\n  Port 2200\n");
        assert_eq!(c.resolve("web", None).port, Some(2200));
    }

    #[test]
    fn match_user_uses_the_user_that_will_actually_be_used() {
        let c = parse("Match user admin\n  Port 2201\n");
        assert_eq!(c.resolve("h", Some("admin")).port, Some(2201));
        assert_eq!(c.resolve("h", Some("nobody")).port, None);
    }

    #[test]
    fn match_exec_never_matches_and_says_so() {
        // Resolving a config must not run a shell command. A GUI resolves
        // every alias just to fill a dropdown.
        let c = parse("Match exec \"true\"\n  User surprising\n");
        let r = c.resolve("h", None);
        assert_eq!(r.user, None);
        assert!(
            r.unsupported.iter().any(|u| u == "Match exec"),
            "{:?}",
            r.unsupported
        );
    }

    #[test]
    fn an_unsupported_keyword_is_reported_rather_than_dropped() {
        // A silently ignored ProxyJump connects to the wrong machine.
        let c = parse("Host web\n  ProxyJump bastion\n  ProxyCommand nc %h %p\n");
        let r = c.resolve("web", None);
        assert!(r.unsupported.iter().any(|u| u == "proxyjump"));
        assert!(r.requires_proxy);
        // ProxyJump came first, so OpenSSH ignores the competing command too.
        assert!(!r.unsupported.iter().any(|u| u == "proxycommand"));
    }

    #[test]
    fn proxy_none_is_a_direct_connection() {
        for keyword in ["ProxyCommand", "ProxyJump"] {
            let c = parse(&format!(
                "Host direct\n  {keyword} none\n  ProxyCommand nc %h %p\n"
            ));
            let r = c.resolve("direct", None);
            assert!(!r.requires_proxy, "{keyword}");
            assert!(r.unsupported.is_empty(), "{keyword}: {:?}", r.unsupported);
            assert_eq!(c.aliases(), vec!["direct".to_string()]);
        }
    }

    #[test]
    fn a_legacy_algorithm_list_turns_the_legacy_switch_on() {
        // The user has already told OpenSSH this is old equipment; making them
        // find our switch as well is the "configured twice" problem again.
        let c = parse("Host old\n  KexAlgorithms +diffie-hellman-group14-sha1\n");
        assert!(c.resolve("old", None).legacy);
        // A modern-only list must not turn it on: the switch only ever widens
        // what is offered, never narrows it.
        assert!(
            !parse("Host m\n  Ciphers aes256-ctr\n")
                .resolve("m", None)
                .legacy
        );
        assert!(
            parse("Host o\n  HostKeyAlgorithms +ssh-rsa\n")
                .resolve("o", None)
                .legacy
        );
    }

    #[test]
    fn tokens_expand() {
        let c = parse("Host web\n  HostName %n.example.com\n  IdentityFile /keys/%h-%p\n");
        let r = c.resolve("web", Some("nata"));
        assert_eq!(r.host_name, "web.example.com");
        // %h is the resolved host name, %p the port — 22 when none was set.
        assert_eq!(
            r.identity_files,
            vec![PathBuf::from("/keys/web.example.com-22")]
        );
    }

    #[test]
    fn an_unknown_token_is_left_alone() {
        let c = parse("Host web\n  IdentityFile /keys/%z\n");
        assert_eq!(
            c.resolve("web", None).identity_files,
            vec![PathBuf::from("/keys/%z")]
        );
    }

    #[test]
    fn quoted_arguments_keep_their_spaces() {
        let c = parse("Host web\n  IdentityFile \"/my keys/id\"\n");
        assert_eq!(
            c.resolve("web", None).identity_files,
            vec![PathBuf::from("/my keys/id")]
        );
    }

    #[test]
    fn identityagent_none_turns_the_agent_off() {
        assert!(
            !parse("Host w\n  IdentityAgent none\n")
                .resolve("w", None)
                .use_agent
        );
        assert!(parse("Host w\n  User x\n").resolve("w", None).use_agent);
    }

    #[test]
    fn strict_host_key_checking_is_read() {
        let c = parse(
            "Host a\n  StrictHostKeyChecking accept-new\nHost b\n  StrictHostKeyChecking no\n",
        );
        assert_eq!(
            c.resolve("a", None).strict_host_key_checking,
            Some(StrictHostKeyChecking::AcceptNew)
        );
        assert_eq!(
            c.resolve("b", None).strict_host_key_checking,
            Some(StrictHostKeyChecking::No)
        );
    }

    #[test]
    fn aliases_leaves_out_the_wildcards() {
        let c = parse("Host web db\n  User x\nHost *.example.com\n  User y\nHost !secret\n");
        assert_eq!(c.aliases(), vec!["web".to_string(), "db".to_string()]);
    }

    #[test]
    fn aliases_leaves_out_what_needs_a_proxy() {
        // systemd's `20-systemd-ssh-proxy.conf`, verbatim enough: two names
        // with no wildcard between them, reachable only by running a command
        // this program does not run. `machine/.host` is caught twice over —
        // by its own block and by the `machine/*` one below it.
        let c = parse(concat!(
            "Host .host machine/.host\n",
            "  ProxyCommand /usr/lib/systemd/systemd-ssh-proxy unix/... %p\n",
            "Host machine/* vsock/*\n",
            "  ProxyCommand /usr/lib/systemd/systemd-ssh-proxy %h %p\n",
            "Host web\n  User x\n",
            "Host behind\n  ProxyJump bastion\n",
        ));
        assert_eq!(c.aliases(), vec!["web".to_string()]);
    }

    #[test]
    fn a_wildcard_proxy_hides_the_host_it_covers() {
        // The reason the filter asks the resolver rather than the block: the
        // name is configured in one place and made unreachable in another.
        let c = parse("Host web\n  User x\nHost *\n  ProxyCommand nc %h %p\n");
        assert!(c.aliases().is_empty(), "{:?}", c.aliases());
    }

    /// A scratch `~/.ssh`-shaped directory, for the tests that need real files.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Scratch {
            let dir = std::env::temp_dir().join(format!("tt-cfg-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).expect("scratch dir");
            Scratch(dir)
        }

        fn write(&self, name: &str, body: &str) -> PathBuf {
            let path = self.0.join(name);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&path, body).unwrap();
            path
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn an_include_is_read_where_it_appears() {
        // Position matters because the first value wins: an Include at the top
        // of a file outranks everything below it, and one at the bottom is
        // outranked by everything above.
        let s = Scratch::new("include");
        s.write("extra", "Host web\n  User included\n");
        let main = s.write("config", "Include extra\n\nHost web\n  User later\n");
        let c = SshConfig::from_files(&[main]).unwrap();
        assert_eq!(c.resolve("web", None).user.as_deref(), Some("included"));

        let main = s.write("config", "Host web\n  User earlier\n\nInclude extra\n");
        let c = SshConfig::from_files(&[main]).unwrap();
        assert_eq!(c.resolve("web", None).user.as_deref(), Some("earlier"));
    }

    #[test]
    fn an_include_inside_a_host_block_stays_inside_it() {
        // `Host web` / `Include tuning` must not apply the included options to
        // every host, which is what treating the include as a fresh global
        // block would do.
        let s = Scratch::new("include-scope");
        s.write("tuning", "Port 2222\n");
        let main = s.write("config", "Host web\n  Include tuning\n");
        let c = SshConfig::from_files(&[main]).unwrap();
        assert_eq!(c.resolve("web", None).port, Some(2222));
        assert_eq!(c.resolve("other", None).port, None);
    }

    #[test]
    fn an_include_glob_is_read_in_sorted_order() {
        // `read_dir` order is the filesystem's, and first-value-wins makes the
        // order load-bearing.
        let s = Scratch::new("include-glob");
        s.write("conf.d/20-late.conf", "Host web\n  User late\n");
        s.write("conf.d/10-early.conf", "Host web\n  User early\n");
        let main = s.write("config", "Include conf.d/*.conf\n");
        let c = SshConfig::from_files(&[main]).unwrap();
        assert_eq!(c.resolve("web", None).user.as_deref(), Some("early"));
    }

    #[test]
    fn a_missing_include_is_not_an_error() {
        let s = Scratch::new("include-missing");
        let main = s.write("config", "Include nothing-here\n\nHost web\n  User me\n");
        let c = SshConfig::from_files(&[main]).unwrap();
        assert_eq!(c.resolve("web", None).user.as_deref(), Some("me"));
    }

    #[test]
    fn a_self_including_file_terminates() {
        // Otherwise opening the connect dialog hangs, which is a worse bug
        // than anything the file could have said.
        let s = Scratch::new("include-loop");
        let main = s.write("config", "Include config\nHost web\n  User me\n");
        let c = SshConfig::from_files(&[main]).unwrap();
        assert_eq!(c.resolve("web", None).user.as_deref(), Some("me"));
    }

    #[test]
    fn an_unresolvable_host_still_resolves_to_itself() {
        let r = SshConfig::default().resolve("plain.example.com", None);
        assert_eq!(r.host_name, "plain.example.com");
        assert_eq!(r.original_host, "plain.example.com");
        assert_eq!(r.port, None);
    }
}

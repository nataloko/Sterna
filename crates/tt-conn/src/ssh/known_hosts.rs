//! `known_hosts` — deciding whether the far end is who it was last time.
//!
//! Written rather than adopted, and the reason is worth recording. Both
//! candidates get this wrong in ways that are invisible until they matter:
//!
//! - **`russh::keys::known_hosts` splits the line on a single space and reads
//!   the second field as the key type.** A line carrying an `@revoked` or
//!   `@cert-authority` marker therefore parses as a host pattern named
//!   `@revoked`, which matches nothing — so a key the user explicitly revoked
//!   comes back as *unknown*, and the prompt offers to accept it. It also has
//!   no wildcard or negation matching, so `*.example.com` never matches
//!   anything.
//! - **Tera Term's `hosts.c:check_host_key` has the wildcards and the
//!   negation** (`:389`, over `matcher.c:match_pattern`) but no hashed
//!   entries at all — `|1|` appears nowhere in `hosts.c` — and the same
//!   blindness to markers. On a distro that ships `HashKnownHosts yes` —
//!   Debian and Ubuntu do — that is every entry in the file. Its matcher is
//!   also case-sensitive, so a host reached by a name typed in a different
//!   case is a host it has never seen.
//!
//! Neither gap is one a caller can paper over, because both report the same
//! thing an untouched file reports: *unknown host*. The whole value of the
//! file is telling those two apart.
//!
//! What is here is OpenSSH's own semantics: comma-separated patterns with
//! `*`/`?` globbing and `!` negation, hashed `|1|salt|hash` entries, the
//! `@revoked` and `@cert-authority` markers, `[host]:port` for anything not on
//! 22, and several files consulted in order.

use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use data_encoding::{BASE64, BASE64_MIME, BASE64_NOPAD};
use hmac::{Hmac, KeyInit, Mac};
use sha1::Sha1;
use sha2::{Digest, Sha256};

use super::pattern::matches_list;
use crate::error::Result;

/// A host key as it exists in both places that matter: an algorithm name and
/// the opaque blob that follows it.
///
/// **The algorithm is the *key's* name, not the negotiated signature
/// algorithm.** An RSA host key verified with `rsa-sha2-512` signatures is
/// still recorded — by us, by OpenSSH and by Tera Term — as `ssh-rsa`, so a
/// caller that passes the negotiated name gets `Unknown` for a host it has
/// connected to a hundred times. The blob itself carries its own type string,
/// so the comparison that decides trust does not depend on getting this right;
/// only the "same host, different key type" distinction does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostKeyRef<'a> {
    pub algorithm: &'a str,
    pub blob: &'a [u8],
}

impl HostKeyRef<'_> {
    /// `SHA256:…`, the form every other client prints and the one a user will
    /// have on a Post-it next to the console server.
    pub fn fingerprint(&self) -> String {
        format!("SHA256:{}", BASE64_NOPAD.encode(&Sha256::digest(self.blob)))
    }

    fn owned(&self) -> HostKey {
        HostKey {
            algorithm: self.algorithm.to_string(),
            blob: self.blob.to_vec(),
        }
    }
}

/// An owned [`HostKeyRef`], for handing a recorded key back to the caller.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostKey {
    pub algorithm: String,
    pub blob: Vec<u8>,
}

impl HostKey {
    pub fn as_ref(&self) -> HostKeyRef<'_> {
        HostKeyRef {
            algorithm: &self.algorithm,
            blob: &self.blob,
        }
    }

    pub fn fingerprint(&self) -> String {
        self.as_ref().fingerprint()
    }
}

/// Where a line that had something to say about this host was found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Site {
    pub file: PathBuf,
    /// 1-based, so it can be quoted at the user and used with an editor.
    pub line: usize,
}

impl fmt::Display for Site {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.file.display(), self.line)
    }
}

/// What the files say about a host key.
///
/// Five outcomes rather than a `bool`, because the frontend has to say five
/// different things, and three of them are not "do you want to continue".
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Recorded, and identical. Connect without asking.
    Trusted(Site),
    /// Recorded for this host under this algorithm, and **different**. The
    /// alarming one: either the server was rebuilt or someone is in the
    /// middle, and nothing in the file can tell those apart.
    Changed { site: Site, recorded: HostKey },
    /// An `@revoked` line names this exact key. Refuse; do not offer to
    /// accept. This is the case both existing implementations turn into
    /// `Unknown`.
    Revoked(Site),
    /// The host is recorded, but only under other algorithms — a server that
    /// gained an Ed25519 key, or a client that used to prefer RSA. Benign
    /// nine times in ten, and worth a different sentence from a first
    /// connection because the tenth is a downgrade.
    NewAlgorithm { also_known: Vec<String> },
    /// Nothing in any file mentions this host at all.
    Unknown,
}

impl Verdict {
    /// Whether connecting can proceed without asking the user.
    pub fn is_trusted(&self) -> bool {
        matches!(self, Verdict::Trusted(_))
    }
}

/// The `known_hosts` files, consulted in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnownHosts {
    files: Vec<PathBuf>,
    hash: bool,
}

impl KnownHosts {
    /// What OpenSSH reads when nothing is configured: `~/.ssh/known_hosts` and
    /// the `known_hosts2` it still honours.
    ///
    /// Tera Term's own `ssh_known_hosts` is deliberately **not** here. It
    /// lives in the install directory on Windows, which does not exist on a
    /// Linux desktop, so the only way to reach it is for the user to say
    /// where — [`with_files`](KnownHosts::with_files), from the migration
    /// path in `PLAN.md`.
    pub fn user_default() -> KnownHosts {
        let mut files = Vec::new();
        if let Some(home) = std::env::home_dir() {
            files.push(home.join(".ssh").join("known_hosts"));
            files.push(home.join(".ssh").join("known_hosts2"));
        }
        KnownHosts { files, hash: false }
    }

    /// Consult exactly these files, in this order. New keys are appended to
    /// the first, which is what OpenSSH does with `UserKnownHostsFile`.
    pub fn with_files(files: Vec<PathBuf>) -> KnownHosts {
        KnownHosts { files, hash: false }
    }

    /// Hash host names when recording new ones — OpenSSH's `HashKnownHosts`.
    ///
    /// Off by default, which is Fedora's and upstream OpenSSH's setting;
    /// Debian and Ubuntu turn it on. Reading hashed entries never depends on
    /// this, only writing them does.
    pub fn hashing(mut self, hash: bool) -> KnownHosts {
        self.hash = hash;
        self
    }

    pub fn files(&self) -> &[PathBuf] {
        &self.files
    }

    /// Judge `key`, presented as the host key of `host` on `port`.
    ///
    /// A file that does not exist is not an error — a first-ever connection
    /// has no `~/.ssh/known_hosts` — but one that exists and cannot be read
    /// is, because silently treating an unreadable file as empty downgrades
    /// every host on it to `Unknown`.
    ///
    /// **Every file is read to the end even after a match.** Stopping at the
    /// first accepting line would be faster and wrong: an `@revoked` entry
    /// further down, or in the second file, has to be able to overrule it.
    pub fn check(&self, host: &str, port: u16, key: HostKeyRef<'_>) -> Result<Verdict> {
        let want = host_pattern(host, port);
        let mut revoked: Option<Site> = None;
        let mut trusted: Option<Site> = None;
        let mut changed: Option<(Site, HostKey)> = None;
        let mut also_known: Vec<String> = Vec::new();

        for file in &self.files {
            let Some(reader) = open_if_present(file)? else {
                continue;
            };
            for (n, line) in reader.lines().enumerate() {
                let line = line?;
                let Some(entry) = Entry::parse(&line) else {
                    continue;
                };
                if !entry.matches(&want) {
                    continue;
                }
                let site = || Site {
                    file: file.clone(),
                    line: n + 1,
                };
                let recorded = entry.key();
                match entry.marker {
                    // A CA line records the authority, not the host. Comparing
                    // it against the host key would be wrong in both
                    // directions: it never equals one, and treating it as a
                    // recorded key for the host would report `Changed` for
                    // every correctly-signed connection.
                    Some(Marker::CertAuthority) => continue,
                    Some(Marker::Revoked) => {
                        if recorded == key {
                            revoked.get_or_insert_with(site);
                        }
                        continue;
                    }
                    None => {}
                }
                if recorded == key {
                    // Several keys of one type for one host is normal — key
                    // rotation, a load balancer — so one match is enough
                    // however many neighbouring lines disagree.
                    trusted.get_or_insert_with(site);
                } else if recorded.algorithm == key.algorithm {
                    if changed.is_none() {
                        changed = Some((site(), recorded.owned()));
                    }
                } else if !also_known.iter().any(|a| a == recorded.algorithm) {
                    also_known.push(recorded.algorithm.to_string());
                }
            }
        }

        Ok(if let Some(site) = revoked {
            Verdict::Revoked(site)
        } else if let Some(site) = trusted {
            Verdict::Trusted(site)
        } else if let Some((site, recorded)) = changed {
            Verdict::Changed { site, recorded }
        } else if !also_known.is_empty() {
            Verdict::NewAlgorithm { also_known }
        } else {
            Verdict::Unknown
        })
    }

    /// Record `key` for `host`, appending to the first configured file.
    ///
    /// Creates `~/.ssh` at `0700` and the file at `0600` if they are missing,
    /// because a `known_hosts` others can write is worth nothing, and OpenSSH
    /// will refuse to read a directory that is group-writable.
    pub fn learn(&self, host: &str, port: u16, key: HostKeyRef<'_>) -> Result<()> {
        let Some(path) = self.files.first() else {
            return Ok(());
        };
        if let Some(dir) = path.parent() {
            if !dir.exists() {
                fs::create_dir_all(dir)?;
                set_mode(dir, 0o700)?;
            }
        }

        let pattern = host_pattern(host, port);
        let name = if self.hash {
            hash_host(&pattern, &random_salt()?)
        } else {
            pattern
        };
        let line = format!("{name} {} {}\n", key.algorithm, BASE64.encode(key.blob));

        let existed = path.exists();
        let mut f = OpenOptions::new().create(true).append(true).open(path)?;
        if !existed {
            set_mode(path, 0o600)?;
        } else if !ends_with_newline(path)? {
            // An operator-edited file that ends without one would otherwise
            // get the new entry glued onto the last line, silently corrupting
            // whatever host was there.
            f.write_all(b"\n")?;
        }
        f.write_all(line.as_bytes())?;
        Ok(())
    }
}

/// `host` on 22, `[host]:port` on anything else — the form OpenSSH writes and
/// the one Tera Term reconstructs by hand at `hosts.c:400`.
fn host_pattern(host: &str, port: u16) -> String {
    let host = host.to_ascii_lowercase();
    if port == 22 {
        host
    } else {
        format!("[{host}]:{port}")
    }
}

fn open_if_present(path: &Path) -> Result<Option<BufReader<File>>> {
    match File::open(path) {
        Ok(f) => Ok(Some(BufReader::new(f))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn ends_with_newline(path: &Path) -> Result<bool> {
    let len = fs::metadata(path)?.len();
    if len == 0 {
        return Ok(true);
    }
    use std::io::{Read, Seek, SeekFrom};
    let mut f = File::open(path)?;
    f.seek(SeekFrom::End(-1))?;
    let mut last = [0u8; 1];
    f.read_exact(&mut last)?;
    Ok(last[0] == b'\n')
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: u32) -> Result<()> {
    Ok(())
}

fn random_salt() -> Result<Vec<u8>> {
    // SHA-1's digest length, which is what OpenSSH uses for the salt too
    // (`hostfile.c:host_hash`).
    let mut salt = vec![0u8; 20];
    getrandom::fill(&mut salt)
        .map_err(|e| std::io::Error::other(format!("no randomness for a salt: {e}")))?;
    Ok(salt)
}

fn hash_host(host: &str, salt: &[u8]) -> String {
    let mut mac = Hmac::<Sha1>::new_from_slice(salt).expect("HMAC takes any key length");
    mac.update(host.as_bytes());
    format!(
        "|1|{}|{}",
        BASE64.encode(salt),
        BASE64.encode(&mac.finalize().into_bytes())
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Marker {
    Revoked,
    CertAuthority,
}

/// One parsed line. Owns its decoded blob and borrows the rest of the line.
struct Entry<'a> {
    marker: Option<Marker>,
    patterns: &'a str,
    algorithm: &'a str,
    blob: Vec<u8>,
}

impl<'a> Entry<'a> {
    /// `None` for anything this cannot use: blank lines, comments, SSH-1
    /// entries, and lines whose base64 does not decode.
    ///
    /// Fields are separated by **any** run of whitespace. OpenSSH writes a
    /// single space, but files get edited, and a tab-separated line that
    /// silently fails to parse is a host that silently becomes unknown —
    /// which is exactly the failure this module exists to avoid.
    fn parse(line: &'a str) -> Option<Entry<'a>> {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            return None;
        }
        let mut fields = line.split_ascii_whitespace();
        let mut patterns = fields.next()?;
        let marker = match patterns {
            "@revoked" => {
                patterns = fields.next()?;
                Some(Marker::Revoked)
            }
            "@cert-authority" => {
                patterns = fields.next()?;
                Some(Marker::CertAuthority)
            }
            _ => None,
        };
        let algorithm = fields.next()?;
        // SSH-1: `patterns bits exponent modulus`. Dropped permanently
        // (`PLAN.md`), and the shape is unambiguous — a decimal bit count
        // where SSH-2 has a key type.
        if algorithm.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let blob = BASE64_MIME.decode(fields.next()?.as_bytes()).ok()?;
        Some(Entry {
            marker,
            patterns,
            algorithm,
            blob,
        })
    }

    fn key(&self) -> HostKeyRef<'_> {
        HostKeyRef {
            algorithm: self.algorithm,
            blob: &self.blob,
        }
    }

    fn matches(&self, host: &str) -> bool {
        match_patterns(self.patterns, host)
    }
}

/// OpenSSH's `match_hostname`: a comma-separated list where any `!pattern`
/// that matches vetoes the whole entry, and hashed entries stand alone.
fn match_patterns(patterns: &str, host: &str) -> bool {
    // A hashed entry is never wildcarded or negated — it is the digest of one
    // exact name — so it is taken out before the glob rules apply.
    let mut hashed = false;
    let plain: Vec<&str> = patterns
        .split(',')
        .filter(|entry| match entry.strip_prefix("|1|") {
            Some(rest) => {
                hashed |= match_hashed(rest, host);
                false
            }
            None => true,
        })
        .collect();
    hashed || matches_list(plain.into_iter(), host)
}

fn match_hashed(rest: &str, host: &str) -> bool {
    let mut parts = rest.split('|');
    let (Some(salt), Some(hash)) = (parts.next(), parts.next()) else {
        return false;
    };
    let (Ok(salt), Ok(hash)) = (
        BASE64_MIME.decode(salt.as_bytes()),
        BASE64_MIME.decode(hash.as_bytes()),
    ) else {
        return false;
    };
    let Ok(mut mac) = Hmac::<Sha1>::new_from_slice(&salt) else {
        return false;
    };
    mac.update(host.as_bytes());
    mac.verify_slice(&hash).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch directory keyed by test name, so tests can run concurrently.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Scratch {
            let dir = std::env::temp_dir().join(format!("tt-kh-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).expect("scratch dir");
            Scratch(dir)
        }

        /// Writes `body` to a file and returns a `KnownHosts` over just it.
        fn file(&self, body: &str) -> KnownHosts {
            let path = self.0.join("known_hosts");
            fs::write(&path, body).expect("write");
            KnownHosts::with_files(vec![path])
        }

        fn read(&self) -> String {
            fs::read_to_string(self.0.join("known_hosts")).expect("read")
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// A blob is opaque here, so tests need nothing more than distinct bytes.
    /// Both real algorithm names, because the `Changed` / `NewAlgorithm`
    /// split turns on comparing them.
    const ED: &str = "ssh-ed25519";
    const RSA: &str = "ssh-rsa";

    fn key<'a>(algorithm: &'a str, blob: &'a [u8]) -> HostKeyRef<'a> {
        HostKeyRef { algorithm, blob }
    }

    /// One `known_hosts` line, with the blob base64'd the way the file has it.
    fn line(patterns: &str, algorithm: &str, blob: &[u8]) -> String {
        format!("{patterns} {algorithm} {}\n", BASE64.encode(blob))
    }

    #[test]
    fn an_absent_file_is_not_an_error() {
        let kh = KnownHosts::with_files(vec![PathBuf::from("/nonexistent/known_hosts")]);
        assert_eq!(
            kh.check("host", 22, key(ED, b"k")).unwrap(),
            Verdict::Unknown
        );
    }

    #[test]
    fn an_exact_entry_is_trusted() {
        let s = Scratch::new("exact");
        let kh = s.file(&line("host.example", ED, b"key"));
        assert!(kh
            .check("host.example", 22, key(ED, b"key"))
            .unwrap()
            .is_trusted());
    }

    #[test]
    fn a_different_key_of_the_same_type_is_the_alarming_one() {
        let s = Scratch::new("changed");
        let kh = s.file(&line("host.example", ED, b"old"));
        let Verdict::Changed { site, recorded } =
            kh.check("host.example", 22, key(ED, b"new")).unwrap()
        else {
            panic!("expected Changed");
        };
        // The line number is what the user needs to fix it by hand.
        assert_eq!(site.line, 1);
        assert_eq!(recorded.blob, b"old");
    }

    #[test]
    fn a_different_algorithm_is_not_a_changed_key() {
        // A server that grew an Ed25519 key is the common case, and telling
        // the user their host key changed would train them to click through
        // the one warning that matters.
        let s = Scratch::new("newalg");
        let kh = s.file(&line("host.example", RSA, b"rsakey"));
        assert_eq!(
            kh.check("host.example", 22, key(ED, b"edkey")).unwrap(),
            Verdict::NewAlgorithm {
                also_known: vec![RSA.to_string()]
            }
        );
    }

    #[test]
    fn one_match_among_several_recorded_keys_is_enough() {
        // Key rotation and load balancers both put several keys of one type
        // against one name.
        let s = Scratch::new("several");
        let kh = s.file(&format!(
            "{}{}",
            line("host.example", ED, b"old"),
            line("host.example", ED, b"new")
        ));
        assert!(kh
            .check("host.example", 22, key(ED, b"new"))
            .unwrap()
            .is_trusted());
    }

    #[test]
    fn a_revoked_key_overrules_a_trusting_line_above_it() {
        // The reason `check` reads to the end of every file. Returning at the
        // first accepting line would accept a key the user revoked.
        let s = Scratch::new("revoked");
        let kh = s.file(&format!(
            "{}@revoked {}",
            line("host.example", ED, b"key"),
            line("host.example", ED, b"key")
        ));
        let Verdict::Revoked(site) = kh.check("host.example", 22, key(ED, b"key")).unwrap() else {
            panic!("expected Revoked");
        };
        assert_eq!(site.line, 2);
    }

    #[test]
    fn a_revoked_line_says_nothing_about_a_different_key() {
        // A blocklist entry is not a record of what the host's key *is*, so a
        // key that differs from a revoked one is unknown rather than changed.
        // Reporting `Changed` here would raise the man-in-the-middle alarm on
        // every host whose compromised key the user had dutifully revoked.
        let s = Scratch::new("revoked-other");
        let kh = s.file(&format!("@revoked {}", line("host.example", ED, b"bad")));
        assert_eq!(
            kh.check("host.example", 22, key(ED, b"good")).unwrap(),
            Verdict::Unknown
        );
    }

    #[test]
    fn a_cert_authority_line_is_not_a_host_key() {
        // Comparing the CA's key against the host's would report `Changed`
        // for every correctly-signed connection.
        let s = Scratch::new("ca");
        let kh = s.file(&format!(
            "@cert-authority {}",
            line("*.example", ED, b"cakey")
        ));
        assert_eq!(
            kh.check("host.example", 22, key(ED, b"hostkey")).unwrap(),
            Verdict::Unknown
        );
    }

    #[test]
    fn a_marker_is_not_a_hostname() {
        // russh reads the marker as the pattern, so this line matches nothing
        // there and the revoked key comes back as unknown. Guard against
        // acquiring the same bug.
        let e = Entry::parse("@revoked host.example ssh-ed25519 a2V5").expect("parses");
        assert_eq!(e.marker, Some(Marker::Revoked));
        assert!(e.matches("host.example"));
    }

    #[test]
    fn a_non_default_port_is_bracketed() {
        let s = Scratch::new("port");
        let kh = s.file(&line("[host.example]:2222", ED, b"key"));
        assert!(kh
            .check("host.example", 2222, key(ED, b"key"))
            .unwrap()
            .is_trusted());
        // And the plain form must not answer for it: a key recorded for the
        // service on 22 says nothing about whatever is on 2222.
        assert_eq!(
            kh.check("host.example", 22, key(ED, b"key")).unwrap(),
            Verdict::Unknown
        );
    }

    #[test]
    fn wildcards_and_negation_match_the_way_openssh_does() {
        assert!(match_patterns("*.example.com", "host.example.com"));
        assert!(match_patterns("h??t.example.com", "host.example.com"));
        assert!(!match_patterns("*.example.com", "host.example.org"));
        // A negated pattern vetoes the whole entry, even alongside a match.
        assert!(!match_patterns(
            "*.example.com,!secret.example.com",
            "secret.example.com"
        ));
        assert!(match_patterns(
            "*.example.com,!secret.example.com",
            "host.example.com"
        ));
        // `*` spans dots, which is why the negation above is needed at all.
        assert!(match_patterns("*", "anything.at.all"));
    }

    #[test]
    fn hostnames_match_case_insensitively() {
        assert!(match_patterns("Host.Example.COM", "host.example.com"));
    }

    #[test]
    fn a_hashed_entry_matches_its_host() {
        // Debian and Ubuntu ship `HashKnownHosts yes`, so on those machines
        // this is every line in the file. Tera Term supports none of them.
        let hashed = hash_host("host.example", b"0123456789abcdefghij");
        assert!(match_patterns(&hashed, "host.example"));
        assert!(!match_patterns(&hashed, "other.example"));
    }

    #[test]
    fn hashing_round_trips_through_the_file() {
        let s = Scratch::new("hash-write");
        let path = s.0.join("known_hosts");
        let kh = KnownHosts::with_files(vec![path]).hashing(true);
        kh.learn("host.example", 2222, key(ED, b"key")).unwrap();
        assert!(s.read().starts_with("|1|"), "{:?}", s.read());
        assert!(kh
            .check("host.example", 2222, key(ED, b"key"))
            .unwrap()
            .is_trusted());
    }

    #[test]
    fn learning_appends_and_survives_a_missing_final_newline() {
        // Hand-edited files lose the trailing newline, and gluing the new
        // entry onto the last line silently corrupts whatever host was there.
        let s = Scratch::new("append");
        let path = s.0.join("known_hosts");
        fs::write(&path, line("other", ED, b"k").trim_end()).unwrap();
        let kh = KnownHosts::with_files(vec![path]);
        kh.learn("host.example", 22, key(ED, b"key")).unwrap();
        assert_eq!(s.read().lines().count(), 2);
        assert!(kh.check("other", 22, key(ED, b"k")).unwrap().is_trusted());
        assert!(kh
            .check("host.example", 22, key(ED, b"key"))
            .unwrap()
            .is_trusted());
    }

    #[test]
    fn learning_creates_the_directory_and_the_file_privately() {
        let s = Scratch::new("create");
        let path = s.0.join("nested").join("known_hosts");
        let kh = KnownHosts::with_files(vec![path.clone()]);
        kh.learn("host.example", 22, key(ED, b"key")).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode(path.parent().unwrap()), 0o700);
            assert_eq!(mode(&path), 0o600);
        }
    }

    #[test]
    fn fields_may_be_separated_by_tabs() {
        // OpenSSH writes single spaces; humans and scripts do not.
        let e = Entry::parse("host.example\tssh-ed25519\ta2V5").expect("parses");
        assert_eq!(e.algorithm, ED);
        assert_eq!(e.blob, b"key");
    }

    #[test]
    fn comments_blank_lines_and_ssh1_entries_are_skipped() {
        assert!(Entry::parse("# a comment").is_none());
        assert!(Entry::parse("   ").is_none());
        // SSH-1: `patterns bits exponent modulus`, dropped permanently.
        assert!(Entry::parse("host.example 1024 35 130321").is_none());
        // A trailing comment field is allowed and ignored.
        let e = Entry::parse("host.example ssh-ed25519 a2V5 nata@laptop").expect("parses");
        assert_eq!(e.blob, b"key");
    }

    #[test]
    fn the_second_file_is_consulted_too() {
        let s = Scratch::new("two-files");
        let first = s.0.join("known_hosts");
        let second = s.0.join("known_hosts2");
        fs::write(&first, line("other", ED, b"k")).unwrap();
        fs::write(&second, line("host.example", ED, b"key")).unwrap();
        let kh = KnownHosts::with_files(vec![first, second.clone()]);
        let Verdict::Trusted(site) = kh.check("host.example", 22, key(ED, b"key")).unwrap() else {
            panic!("expected Trusted");
        };
        assert_eq!(site.file, second);
    }

    #[test]
    fn a_fingerprint_is_the_form_people_compare() {
        // `ssh-keygen -lf` prints exactly this for the same bytes.
        assert_eq!(
            key(ED, b"").fingerprint(),
            "SHA256:47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU"
        );
    }
}

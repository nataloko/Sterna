//! Stage 0 spike 5 — can `russh` negotiate what old equipment offers?
//!
//! The risk in PLAN.md was "russh maturity against old gear". Nobody involved
//! has any old gear, so this tests the part that *is* testable: whether every
//! pre-2020 algorithm can actually be negotiated end-to-end, against two
//! independent server implementations.
//!
//!   :2222  OpenSSH 9.6 with the legacy algorithms explicitly re-enabled
//!   :2223  Dropbear 2022.83 — a different codebase, and the one actually
//!          found on console servers
//!
//! What this canNOT test is real-device *behaviour* — non-RFC banners, devices
//! that hang up on an unexpected packet, 30-second key exchange on a weak CPU.
//! That risk stays open; see PLAN.md.

use std::borrow::Cow;
use std::sync::Arc;
use std::time::Duration;

use russh::keys::{Algorithm, HashAlg, PrivateKeyWithHashAlg, load_secret_key};
use russh::{Preferred, client, cipher, kex, mac};

struct Handler;

impl client::Handler for Handler {
    type Error = russh::Error;
    async fn check_server_key(
        &mut self,
        _key: &russh::keys::ssh_key::PublicKey,
    ) -> Result<bool, Self::Error> {
        Ok(true) // host key policy is not what this audit is measuring
    }
}

struct Case {
    label: &'static str,
    port: u16,
    kex: Vec<kex::Name>,
    key: Vec<Algorithm>,
    cipher: Vec<cipher::Name>,
    mac: Vec<mac::Name>,
}

async fn try_case(c: &Case, keyfile: &str, user: &str) -> Result<String, String> {
    let config = client::Config {
        inactivity_timeout: Some(Duration::from_secs(8)),
        preferred: Preferred {
            kex: Cow::Owned(c.kex.clone()),
            key: Cow::Owned(c.key.clone()),
            cipher: Cow::Owned(c.cipher.clone()),
            mac: Cow::Owned(c.mac.clone()),
            ..Default::default()
        },
        ..Default::default()
    };

    let key = load_secret_key(keyfile, None).map_err(|e| format!("key load: {e}"))?;

    let mut session = client::connect(Arc::new(config), ("127.0.0.1", c.port), Handler)
        .await
        .map_err(|e| format!("connect: {e}"))?;

    let hash = session
        .best_supported_rsa_hash()
        .await
        .map_err(|e| format!("rsa hash probe: {e}"))?
        .flatten();

    let auth = session
        .authenticate_publickey(user, PrivateKeyWithHashAlg::new(Arc::new(key), hash))
        .await
        .map_err(|e| format!("auth: {e}"))?;
    if !auth.success() {
        return Err("auth rejected".into());
    }

    let mut channel = session
        .channel_open_session()
        .await
        .map_err(|e| format!("channel: {e}"))?;
    channel
        .exec(true, "echo spike5")
        .await
        .map_err(|e| format!("exec: {e}"))?;

    let mut out = Vec::new();
    while let Some(msg) = channel.wait().await {
        if let russh::ChannelMsg::Data { ref data } = msg {
            out.extend_from_slice(data);
        }
        if matches!(msg, russh::ChannelMsg::ExitStatus { .. }) {
            break;
        }
    }
    let text = String::from_utf8_lossy(&out).trim().to_string();
    if text == "spike5" {
        Ok(text)
    } else {
        Err(format!("unexpected output {text:?}"))
    }
}

/// A known-good baseline for the dimensions a case is not varying.
fn base_kex() -> Vec<kex::Name> {
    vec![kex::CURVE25519, kex::DH_G14_SHA256]
}
fn base_key() -> Vec<Algorithm> {
    vec![
        Algorithm::Ed25519,
        Algorithm::Rsa { hash: Some(HashAlg::Sha256) },
    ]
}
fn base_cipher() -> Vec<cipher::Name> {
    vec![cipher::AES_256_CTR, cipher::AES_128_CTR]
}
fn base_mac() -> Vec<mac::Name> {
    vec![mac::HMAC_SHA256, mac::HMAC_SHA1]
}

fn cases() -> Vec<Case> {
    let mut v = Vec::new();
    for (port, tag) in [(2222u16, "openssh"), (2223, "dropbear")] {
        // Key exchange — the dimension most likely to fail against old gear.
        for (name, k) in [
            ("kex dh-group1-sha1", kex::DH_G1_SHA1),
            ("kex dh-group14-sha1", kex::DH_G14_SHA1),
            ("kex dh-gex-sha1", kex::DH_GEX_SHA1),
            ("kex dh-group14-sha256", kex::DH_G14_SHA256),
            ("kex curve25519", kex::CURVE25519),
        ] {
            v.push(Case {
                label: Box::leak(format!("{tag:<9} {name}").into_boxed_str()),
                port,
                kex: vec![k],
                key: base_key(),
                cipher: base_cipher(),
                mac: base_mac(),
            });
        }
        // Host key algorithms. ssh-rsa means an RSA key with SHA-1 signatures,
        // which is what everything shipped before ~2020 presents.
        for (name, a) in [
            ("hostkey ssh-rsa (SHA-1)", Algorithm::Rsa { hash: None }),
            ("hostkey rsa-sha2-256", Algorithm::Rsa { hash: Some(HashAlg::Sha256) }),
            ("hostkey rsa-sha2-512", Algorithm::Rsa { hash: Some(HashAlg::Sha512) }),
            ("hostkey ssh-ed25519", Algorithm::Ed25519),
        ] {
            v.push(Case {
                label: Box::leak(format!("{tag:<9} {name}").into_boxed_str()),
                port,
                kex: base_kex(),
                key: vec![a],
                cipher: base_cipher(),
                mac: base_mac(),
            });
        }
        // Ciphers — CBC modes and 3DES are the legacy end.
        for (name, c) in [
            ("cipher 3des-cbc", cipher::TRIPLE_DES_CBC),
            ("cipher aes128-cbc", cipher::AES_128_CBC),
            ("cipher aes256-cbc", cipher::AES_256_CBC),
            ("cipher aes128-ctr", cipher::AES_128_CTR),
            ("cipher aes256-gcm", cipher::AES_256_GCM),
            ("cipher chacha20", cipher::CHACHA20_POLY1305),
        ] {
            v.push(Case {
                label: Box::leak(format!("{tag:<9} {name}").into_boxed_str()),
                port,
                kex: base_kex(),
                key: base_key(),
                cipher: vec![c],
                mac: base_mac(),
            });
        }
        // MACs.
        for (name, m) in [
            ("mac hmac-sha1", mac::HMAC_SHA1),
            ("mac hmac-sha2-256", mac::HMAC_SHA256),
            ("mac hmac-sha2-512", mac::HMAC_SHA512),
        ] {
            v.push(Case {
                label: Box::leak(format!("{tag:<9} {name}").into_boxed_str()),
                port,
                kex: base_kex(),
                key: base_key(),
                cipher: base_cipher(),
                mac: vec![m],
            });
        }
    }
    v
}

/// Auth methods, tested separately from the algorithm matrix. Old equipment
/// rarely supports public keys, so password and keyboard-interactive are the
/// paths that actually matter — and the UI depends on telling a *rejection*
/// apart from a *transport failure*, so the wrong-password case is a real
/// assertion, not padding.
async fn auth_cases(user: &str, pass: &str) -> Vec<(String, Result<String, String>)> {
    let mut out = Vec::new();
    let cfg = || {
        Arc::new(client::Config {
            inactivity_timeout: Some(Duration::from_secs(8)),
            ..Default::default()
        })
    };

    // password, correct
    out.push(("auth password (correct)".to_string(), async {
        let mut s = client::connect(cfg(), ("127.0.0.1", 2222u16), Handler)
            .await
            .map_err(|e| format!("connect: {e}"))?;
        let r = s.authenticate_password(user, pass).await
            .map_err(|e| format!("auth: {e}"))?;
        if r.success() { Ok("accepted".into()) } else { Err("rejected a valid password".into()) }
    }.await));

    // password, wrong — must be cleanly rejected, not an error
    out.push(("auth password (wrong, expect reject)".to_string(), async {
        let mut s = client::connect(cfg(), ("127.0.0.1", 2222u16), Handler)
            .await
            .map_err(|e| format!("connect: {e}"))?;
        let r = s.authenticate_password(user, "definitely-wrong").await
            .map_err(|e| format!("auth: {e}"))?;
        if r.success() { Err("accepted a wrong password".into()) } else { Ok("rejected".into()) }
    }.await));

    // keyboard-interactive: the prompt/response cycle old devices use
    out.push(("auth keyboard-interactive".to_string(), async {
        let mut s = client::connect(cfg(), ("127.0.0.1", 2222u16), Handler)
            .await
            .map_err(|e| format!("connect: {e}"))?;
        let mut resp = s.authenticate_keyboard_interactive_start(user, None).await
            .map_err(|e| format!("kbdint start: {e}"))?;
        for _ in 0..4 {
            match resp {
                client::KeyboardInteractiveAuthResponse::Success => {
                    return Ok("accepted".into())
                }
                client::KeyboardInteractiveAuthResponse::Failure { .. } => {
                    return Err("rejected".into())
                }
                client::KeyboardInteractiveAuthResponse::InfoRequest { ref prompts, .. } => {
                    let answers = prompts.iter().map(|_| pass.to_string()).collect();
                    resp = s.authenticate_keyboard_interactive_respond(answers).await
                        .map_err(|e| format!("kbdint respond: {e}"))?;
                }
            }
        }
        Err("too many prompt rounds".into())
    }.await));

    out
}

#[tokio::main]
async fn main() {
    let dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or("/tmp".into())
        + "/sterna-ssh-audit";
    let keyfile = format!("{dir}/id_rsa");
    let user = std::env::var("USER").unwrap_or("nata".into());

    println!("=== spike 5: russh 0.62 vs legacy algorithms ===\n");
    let (mut pass, mut na, mut fail) = (0, 0, 0);
    let mut failures = Vec::new();

    for c in cases() {
        match try_case(&c, &keyfile, &user).await {
            Ok(_) => {
                println!("  ok   {}", c.label);
                pass += 1;
            }
            // Each case offers exactly ONE algorithm in the dimension under
            // test, so "No common X" means the SERVER lacks it — russh offered
            // it correctly. That is an expected negative, not a russh gap.
            // Any other error would be a real finding.
            Err(e) if e.contains("No common") => {
                println!("  n/a  {:<40} server does not offer it", c.label);
                na += 1;
            }
            Err(e) => {
                println!("  FAIL {:<40} {}", c.label, e);
                failures.push((c.label, e));
                fail += 1;
            }
        }
    }

    println!();
    for (label, r) in auth_cases("sterna-test", "spike5-not-a-secret").await {
        match r {
            Ok(note) => { println!("  ok   openssh   {label:<38} {note}"); pass += 1; }
            Err(e) => {
                println!("  FAIL openssh   {label:<38} {e}");
                failures.push((Box::leak(label.into_boxed_str()) as &str, e));
                fail += 1;
            }
        }
    }

    println!("\n{pass} negotiated, {na} not offered by the server, {fail} genuine failures");
    if !failures.is_empty() {
        println!("\nGENUINE FAILURES (server offered it, russh could not complete):");
        for (l, e) in &failures {
            println!("  {l}: {e}");
        }
    } else {
        println!("\nNo algorithm the server offered failed to negotiate.");
    }
    std::process::exit(if fail == 0 { 0 } else { 1 });
}

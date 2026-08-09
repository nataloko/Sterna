//! The two password stores, underneath the eight commands in [`pwdcmds`].
//!
//! Tera Term has stored macro passwords twice over, and both formats are still
//! read by the shipping program, so both are here.
//!
//! **Version 1 (`ttmenc.c`, 1994) is obfuscation, not encryption.** It takes no
//! key. Anything it writes can be read back by anyone holding the file — the
//! function to do it is thirty lines below the one that wrote it, and
//! [`v1_decrypt`] is that function. It is ported because existing
//! `password.dat` files are full of it and a successor that cannot read them is
//! not a successor; it is **not** ported because it protects anything. The
//! output goes into an INI file under `[Password]`, one entry per key name.
//!
//! **Version 2 (`ttmenc2.c`, 2024) is real.** The password is encrypted with
//! AES-256-CTR under a key derived from a second password the macro supplies,
//! by PBKDF2-HMAC-SHA512 at 210001 iterations, and the record carries an
//! HMAC-SHA512 over itself. The key *name* is stored only as a PBKDF2 hash, so
//! the file does not say whose passwords it holds. Each record is a fixed 381
//! bytes, base64'd to 508 characters, one per line — which is why the file is
//! not an INI file and the two formats can share one path without colliding.
//!
//! Everything here is byte-compatible with upstream in both directions. The
//! constants are `ttmenc2.c:45-55` and are not ours to choose: change one and
//! every file written by a real Tera Term stops opening.
//!
//! ## What is deliberately not reproduced
//!
//! Upstream's two encoders have five memory defects between them, listed on
//! [`v1_encrypt`], [`v1_decrypt`] and [`v2_set`]. None is reproduced — they are
//! out-of-bounds accesses with no result a macro can see, which is the same
//! rule the `ttl.cpp` handle-array bugs are held to. The **observable**
//! quirks are all kept, and the one that bites is on
//! [`Interp::cmd_get_password`](crate::Interp): a v1 record whose first and
//! last characters happen to be a matching pair of quotes is unreadable,
//! because the INI layer strips them.
//!
//! [`pwdcmds`]: crate::pwdcmds

use std::path::Path;

use aes::cipher::{KeyIvInit, StreamCipher};
use aes::Aes256;
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha512;

type Aes256Ctr = ctr::Ctr128BE<Aes256>;

// ---------------------------------------------------------------------------
// Version 1 — `ttmenc.c`
// ---------------------------------------------------------------------------

/// `EncCharacterize` / `DecCharacter`'s rolling bias (`ttmenc.c:62`).
///
/// Both functions advance `b` through the same six values, so the two are
/// exact inverses as long as this is the only place the sequence is written
/// down.
fn next_bias(b: u8) -> u8 {
    match b {
        ..=0x2f => 0x30,
        0x30..=0x3f => 0x40,
        0x40..=0x4f => 0x50,
        0x50..=0x5f => 0x60,
        0x60..=0x6f => 0x70,
        _ => 0x21,
    }
}

/// `Encrypt` (`ttmenc.c:84`) — the v1 obfuscation.
///
/// The plaintext is read six bits at a time, most significant first; each group
/// is written as two characters, the group plus a random mask and the mask
/// itself. A leading character carries a random value and a trailing one
/// carries its complement, which is the only integrity check the format has.
/// `rand` is that randomness — it exists so identical passwords do not produce
/// identical records, and **not** to make the result secret, since the mask is
/// stored beside the byte it masked.
///
/// Upstream seeds `srand` from `GetTickCount` and calls `rand`; the values are
/// arbitrary, so this takes the interpreter's own source rather than
/// reproducing an MSVC PRNG. A host with a fixed
/// [`random_u32`](crate::ScriptHost::random_u32) therefore gets a repeatable
/// record, which is what the tests below rely on.
///
/// **Upstream overflows its caller's buffer here.** The output is
/// `2·ceil(4n/3) + 2` characters for an `n`-byte password, and both callers
/// hand it a `char[512]` (`ttl.cpp:2591`, `ttl_gui.cpp:237`) while accepting a
/// password of up to 511 — so anything over **190 characters** writes past the
/// end of a stack buffer. Not reproduced; a `Vec` is the right length by
/// construction.
pub fn v1_encrypt(plain: &[u8], mut rand: impl FnMut() -> u32) -> Vec<u8> {
    if plain.is_empty() || plain[0] == 0 {
        return Vec::new();
    }
    let r0 = (rand() & 0x3f) as u8;
    let mut out = vec![r0];
    // `EncSeparate` indexes `Str[cptr + 1]`, so the terminator is part of the
    // input as far as the bitstream is concerned.
    let mut src = plain.to_vec();
    src.push(0);
    src.push(0);

    let mut i = 0usize;
    loop {
        let cptr = i / 8;
        // The loop ends at the NUL, which is why a password may be encoded
        // together with a few bits of its own terminator.
        if src[cptr] == 0 {
            break;
        }
        let bptr = i % 8;
        let d = ((src[cptr] as u32) << 8) | src[cptr + 1] as u32;
        let b = ((d >> (10 - bptr)) & 0x3f) as u8;
        i += 6;

        let r = (rand() & 0x3f) as u8;
        out.push((b.wrapping_add(r)) & 0x3f);
        out.push(r);
    }
    out.push((!r0) & 0x3f);

    let mut bias = 0x21u8;
    for c in out.iter_mut() {
        let mut d = c.wrapping_add(bias);
        if d > 0x7e {
            d -= 0x5e;
        }
        bias = next_bias(bias);
        *c = d;
    }
    out
}

/// `Decrypt` (`ttmenc.c:168`) — the inverse, which needs no key.
///
/// An empty result is every kind of failure at once: a record whose first and
/// last characters do not complement each other, and one that was never a
/// record. Upstream cannot tell those apart either.
///
/// **Two overflows here are upstream's and are not reproduced.** The working
/// buffer is a `char[512]` indexed by `strlen` of a value read from the
/// password file, and the output is a `TStrVal` — also 512 — holding
/// three-quarters of the input. Both are reachable from a file the macro
/// names, and `getpassword` will happily be pointed at any file at all.
pub fn v1_decrypt(enc: &[u8]) -> Vec<u8> {
    if enc.is_empty() {
        return Vec::new();
    }
    let mut bias = 0x21u8;
    let tmp: Vec<u8> = enc
        .iter()
        .map(|&c| {
            let d = if c < bias {
                0x5eu8.wrapping_add(c).wrapping_sub(bias)
            } else {
                c - bias
            } & 0x3f;
            bias = next_bias(bias);
            d
        })
        .collect();

    if tmp[0] ^ tmp[tmp.len() - 1] != 0x3f {
        return Vec::new();
    }

    // `DecCombine` keeps a rolling sixteen-bit window and always writes the
    // byte past the one it is filling, so the plaintext is NUL-terminated by
    // the bits of the terminator that `Encrypt` folded in.
    let mut out = vec![0u8; tmp.len().div_ceil(2) * 6 / 8 + 2];
    let mut k = 0usize;
    let mut i = 1usize;
    while i + 2 < tmp.len() {
        let b = tmp[i].wrapping_sub(tmp[i + 1]) & 0x3f;
        let cptr = k / 8;
        let bptr = k % 8;
        if bptr == 0 {
            out[cptr] = 0;
        }
        let d = ((out[cptr] as u32) << 8) | ((b as u32) << (10 - bptr));
        out[cptr] = (d >> 8) as u8;
        out[cptr + 1] = (d & 0xff) as u8;
        k += 6;
        i += 2;
    }

    let end = out.iter().position(|&c| c == 0).unwrap_or(out.len());
    out.truncate(end);
    out
}

// ---------------------------------------------------------------------------
// Base64 — `ttlib.c:76`, upstream's own
// ---------------------------------------------------------------------------

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// `b64encode`. Standard alphabet, standard padding.
///
/// A v2 record is 381 bytes, which is divisible by three, so the encoder never
/// reaches its padding arm in practice — it is here because the function is
/// shared and a wrong tail would be a silent difference the day something else
/// uses it.
fn b64encode(src: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(src.len().div_ceil(3) * 4);
    for chunk in src.chunks(3) {
        let mut b = 0u32;
        for &c in chunk {
            b = (b << 8) | c as u32;
        }
        match chunk.len() {
            3 => {
                out.push(B64[(b >> 18) as usize & 0x3f]);
                out.push(B64[(b >> 12) as usize & 0x3f]);
                out.push(B64[(b >> 6) as usize & 0x3f]);
                out.push(B64[b as usize & 0x3f]);
            }
            2 => {
                out.push(B64[(b >> 10) as usize & 0x3f]);
                out.push(B64[(b >> 4) as usize & 0x3f]);
                out.push(B64[(b << 2) as usize & 0x3f]);
                out.push(b'=');
            }
            _ => {
                out.push(B64[(b >> 2) as usize & 0x3f]);
                out.push(B64[(b << 4) as usize & 0x3f]);
                out.push(b'=');
                out.push(b'=');
            }
        }
    }
    out
}

/// `b64decode`. **Stops at the first character it does not know**, `=`
/// included, rather than reporting an error — so a truncated or corrupt line
/// decodes to however much was valid, and the caller's length check is what
/// rejects it. That is upstream's contract and the v2 reader depends on it.
fn b64decode(src: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(src.len() / 4 * 3);
    let mut b = 0u32;
    let mut state = 0;
    for &c in src {
        if c.is_ascii_whitespace() {
            continue;
        }
        let Some(v) = B64.iter().position(|&t| t == c) else {
            break;
        };
        b = (b << 6) | v as u32;
        state += 1;
        if state == 4 {
            out.push((b >> 16) as u8);
            out.push((b >> 8) as u8);
            out.push(b as u8);
            state = 0;
        }
    }
    match state {
        2 => out.push((b >> 4) as u8),
        3 => {
            out.push((b >> 10) as u8);
            out.push((b >> 2) as u8);
        }
        _ => {}
    }
    out
}

// ---------------------------------------------------------------------------
// Version 2 — `ttmenc2.c`
// ---------------------------------------------------------------------------

/// `ENCRYPT2_SALTLEN`.
const SALT: usize = 16;
/// SHA-512's digest, which is every hash length in the format.
const HASH: usize = 64;
/// `ENCRYPT2_ITER1` — the key-name hash and the HMAC key.
const ITER1: u32 = 1001;
/// `ENCRYPT2_ITER2` — the password key. Deliberately expensive.
const ITER2: u32 = 210001;
/// `ENCRYPT2_IKLEN` + `ENCRYPT2_IVLEN`.
const IKLEN: usize = 32;
const IVLEN: usize = 16;
/// `ENCRYPT2_TAG`, which is how a v2 line is told from anything else.
const TAG: [u8; 2] = [0x00, 0x02];
/// `ENCRYPT2_PWD_MAX_LEN`. A password is NUL-padded to exactly this, so the
/// record does not leak its length.
pub const PWD_MAX_LEN: usize = 203;

/// `ENCRYPT2_PROFILE_LEN` — 381 bytes, and every field is a byte array, so the
/// struct has no padding and the wire form is the struct.
const PROFILE_LEN: usize = 2 + SALT + HASH + SALT + PWD_MAX_LEN + SALT + HASH;
/// `ENCRYPT2_BASE64_LEN`. 381 divides by three, so there is no `=` on the end.
const B64_LEN: usize = PROFILE_LEN / 3 * 4;
/// `ENCRYPT2_MaxLineLen`, which is also `PwdFileReadln`'s truncation point: a
/// longer line is cut here, fails the length check and is passed over.
const MAX_LINE: usize = 512;

/// The field offsets, in the order `Encrypt2Profile` declares them.
const O_KEYSALT: usize = 2;
const O_KEYHASH: usize = O_KEYSALT + SALT;
const O_PASSSALT: usize = O_KEYHASH + HASH;
const O_PASSSTR: usize = O_PASSSALT + SALT;
const O_ENCSALT: usize = O_PASSSTR + PWD_MAX_LEN;
const O_ENCHASH: usize = O_ENCSALT + SALT;

/// One record, as its 381 bytes.
///
/// Kept flat rather than as named fields because the HMAC is taken over the
/// first 317 bytes of the struct as laid out, so the layout *is* the format
/// and a field-per-member version would have to serialise it back anyway.
#[derive(Clone)]
struct Profile([u8; PROFILE_LEN]);

impl Profile {
    fn get(&self, at: usize, len: usize) -> &[u8] {
        &self.0[at..at + len]
    }

    fn put(&mut self, at: usize, bytes: &[u8]) {
        self.0[at..at + bytes.len()].copy_from_slice(bytes);
    }
}

/// `PKCS5_PBKDF2_HMAC` with `EVP_sha512`.
fn pbkdf2(password: &[u8], salt: &[u8], iter: u32, out: &mut [u8]) {
    pbkdf2::pbkdf2_hmac::<Sha512>(password, salt, iter, out);
}

/// `HMAC(EVP_sha512, ...)`.
fn hmac(key: &[u8], data: &[u8]) -> [u8; HASH] {
    let mut m = <Hmac<Sha512> as KeyInit>::new_from_slice(key).expect("HMAC takes any key length");
    m.update(data);
    m.finalize().into_bytes().into()
}

/// The cipher, keyed and positioned at the start of its keystream.
///
/// `EVP_aes_256_ctr` counts the whole IV block big-endian, which is what
/// `Ctr128BE` is. Key and IV are the two halves of one 48-byte PBKDF2 output —
/// upstream derives them together in a single call, so deriving them
/// separately would produce different bytes.
fn stream(enc: &[u8], salt: &[u8; SALT]) -> Aes256Ctr {
    let mut key_iv = [0u8; IKLEN + IVLEN];
    pbkdf2(enc, salt, ITER2, &mut key_iv);
    let mut key = [0u8; IKLEN];
    let mut iv = [0u8; IVLEN];
    key.copy_from_slice(&key_iv[..IKLEN]);
    iv.copy_from_slice(&key_iv[IKLEN..]);
    Aes256Ctr::new(&key.into(), &iv.into())
}

/// `RAND_bytes`. The OS generator, not the interpreter's `random`: a salt that
/// a host could make repeatable would be no salt at all.
fn rand_bytes(out: &mut [u8]) -> bool {
    getrandom::fill(out).is_ok()
}

/// `Encrypt2EncDec` with `encrypt` set (`ttmenc2.c:279`).
///
/// Fills `PassSalt`, `PassStr`, `EncSalt` and `EncHash`. The three ciphertext
/// fields are **one continuous keystream** — 203 bytes of NUL-padded password,
/// then the 16-byte `EncSalt`, then the 64-byte HMAC at offset 219 — because
/// upstream pushes all three through the same OpenSSL cipher BIO in that
/// order.
fn v2_encrypt(profile: &mut Profile, pass: &[u8], enc: &[u8]) -> bool {
    let mut pass_salt = [0u8; SALT];
    let mut enc_salt = [0u8; SALT];
    if !rand_bytes(&mut pass_salt) || !rand_bytes(&mut enc_salt) {
        return false;
    }
    profile.put(O_PASSSALT, &pass_salt);

    let mut cipher = stream(enc, &pass_salt);

    let mut buf = [0u8; PWD_MAX_LEN];
    buf[..pass.len()].copy_from_slice(pass);
    cipher.apply_keystream(&mut buf);
    profile.put(O_PASSSTR, &buf);
    cipher.apply_keystream(&mut enc_salt);
    profile.put(O_ENCSALT, &enc_salt);

    // The HMAC key comes from `EncSalt` **as stored**, which by this point is
    // the encrypted form. Reads the same way round on the way back, so it is a
    // quirk rather than a defect — but taking the plaintext salt here would
    // write files nothing else can open.
    let mut hkey = [0u8; HASH];
    pbkdf2(enc, &enc_salt, ITER1, &mut hkey);
    let mut mac = hmac(&hkey, profile.get(0, PROFILE_LEN - HASH));
    cipher.apply_keystream(&mut mac);
    profile.put(O_ENCHASH, &mac);
    true
}

/// `Encrypt2EncDec` with `encrypt` clear. `None` is a failed HMAC, which is
/// the wrong `<encryptstr>` or a tampered record; upstream cannot tell those
/// apart and neither can this.
fn v2_decrypt(profile: &Profile, enc: &[u8]) -> Option<Vec<u8>> {
    let mut salt = [0u8; SALT];
    salt.copy_from_slice(profile.get(O_PASSSALT, SALT));
    let mut cipher = stream(enc, &salt);

    let mut plain = profile.get(O_PASSSTR, PWD_MAX_LEN + SALT + HASH).to_vec();
    cipher.apply_keystream(&mut plain);
    let want = &plain[PWD_MAX_LEN + SALT..];

    let mut hkey = [0u8; HASH];
    pbkdf2(enc, profile.get(O_ENCSALT, SALT), ITER1, &mut hkey);
    let got = hmac(&hkey, profile.get(0, PROFILE_LEN - HASH));

    // Constant time, as `CRYPTO_memcmp` is: the comparison is over a MAC an
    // attacker can grind against.
    let mut diff = 0u8;
    for (a, b) in got.iter().zip(want) {
        diff |= a ^ b;
    }
    if diff != 0 {
        return None;
    }
    let pass = &plain[..PWD_MAX_LEN];
    let end = pass.iter().position(|&c| c == 0).unwrap_or(PWD_MAX_LEN);
    Some(pass[..end].to_vec())
}

/// The file, as `PwdFileReadln` sees it.
///
/// Upstream works a byte at a time on an open handle and rewrites in place;
/// this reads the lot, edits, and writes it back. A record is 508 characters,
/// a file holds a handful, and the in-place version has a trap this does not:
/// its delete removes exactly `508 + 2` bytes at a computed offset, so a file
/// somebody has run through an editor that converted the line endings comes
/// back corrupt. Lines that are not v2 records are preserved either way, which
/// is what lets one file hold both formats.
fn read_lines(path: &Path) -> Option<Vec<Vec<u8>>> {
    let bytes = std::fs::read(path).ok()?;
    let mut lines = Vec::new();
    let mut cur = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\r' => {
                if bytes.get(i + 1) == Some(&b'\n') {
                    i += 1;
                }
                lines.push(std::mem::take(&mut cur));
            }
            b'\n' => lines.push(std::mem::take(&mut cur)),
            c => {
                // `PwdFileReadln` fills a 512-byte buffer and drops the rest of
                // an over-long line rather than splitting it.
                if cur.len() < MAX_LINE - 1 {
                    cur.push(c);
                }
            }
        }
        i += 1;
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    Some(lines)
}

/// Write the records back, CRLF-terminated — which is what upstream's own
/// writer emits for every line it produces.
fn write_lines(path: &Path, lines: &[Vec<u8>]) -> bool {
    let mut out = Vec::new();
    for l in lines {
        out.extend_from_slice(l);
        out.extend_from_slice(b"\r\n");
    }
    std::fs::write(path, out).is_ok()
}

/// Decode one line, if it is a v2 record at all.
fn parse(line: &[u8]) -> Option<Profile> {
    if line.len() != B64_LEN {
        return None;
    }
    let bytes = b64decode(line);
    if bytes.len() != PROFILE_LEN || bytes[..2] != TAG {
        return None;
    }
    let mut p = Profile([0u8; PROFILE_LEN]);
    p.0.copy_from_slice(&bytes);
    Some(p)
}

/// `Encrypt2ProfileSearch` — the index of the record whose key hash matches.
///
/// **The last match wins.** Upstream does not stop at the first, explicitly so
/// that the time it takes does not say where in the file the answer was, and
/// its saved position is therefore the last one it saw.
fn search(lines: &[Vec<u8>], key: &[u8]) -> Option<usize> {
    let mut found = None;
    for (i, line) in lines.iter().enumerate() {
        let Some(p) = parse(line) else { continue };
        let mut want = [0u8; HASH];
        pbkdf2(key, p.get(O_KEYSALT, SALT), ITER1, &mut want);
        if want == p.get(O_KEYHASH, HASH) {
            found = Some(i);
        }
    }
    found
}

/// `Encrypt2SetPassword` — add or replace one record. False is any failure.
///
/// **Upstream's "has it changed?" test cannot be trusted and is not copied.**
/// It decrypts the old record and compares 203 bytes of it against the new
/// password's `TStrVal`, which `strncpy_s` NUL-*terminates* rather than
/// NUL-*pads* — so the tail of the comparison is uninitialised stack and the
/// answer is usually "changed" whatever the truth is. The same call also writes
/// `PassStr[203]`, one past the field, into the record's own `EncSalt`.
/// Comparing the two byte strings properly is what the test was reaching for,
/// and a macro cannot tell the difference: both paths answer `result` 1, and
/// only whether the line is rewritten with fresh salts differs.
pub fn v2_set(path: &Path, key: &[u8], pass: &[u8], enc: &[u8]) -> bool {
    if pass.len() > PWD_MAX_LEN {
        return false;
    }
    // `OPEN_ALWAYS` — the file is created if it is not there.
    let mut lines = read_lines(path).unwrap_or_default();
    let at = search(&lines, key);

    if let Some(i) = at {
        if let Some(old) = parse(&lines[i]) {
            if v2_decrypt(&old, enc).as_deref() == Some(pass) {
                return true; // パスワード変更無し — nothing to write.
            }
        }
    }

    let mut p = Profile([0u8; PROFILE_LEN]);
    p.put(0, &TAG);
    let mut key_salt = [0u8; SALT];
    if !rand_bytes(&mut key_salt) {
        return false;
    }
    p.put(O_KEYSALT, &key_salt);
    let mut key_hash = [0u8; HASH];
    pbkdf2(key, &key_salt, ITER1, &mut key_hash);
    p.put(O_KEYHASH, &key_hash);
    if !v2_encrypt(&mut p, pass, enc) {
        return false;
    }

    let line = b64encode(&p.0);
    match at {
        Some(i) => lines[i] = line,
        None => lines.push(line),
    }
    write_lines(path, &lines)
}

/// `Encrypt2GetPassword`. `None` covers a missing file, a missing key and a
/// wrong `<encryptstr>` alike — all three are `result` 0 to the macro.
pub fn v2_get(path: &Path, key: &[u8], enc: &[u8]) -> Option<Vec<u8>> {
    let lines = read_lines(path)?;
    let i = search(&lines, key)?;
    v2_decrypt(&parse(&lines[i])?, enc)
}

/// `Encrypt2IsPassword` — is there a record under this key name? The
/// `<encryptstr>` is not needed and not asked for, because the key hash is
/// stored in the clear.
pub fn v2_is(path: &Path, key: &[u8]) -> bool {
    read_lines(path).is_some_and(|l| search(&l, key).is_some())
}

/// `Encrypt2DelPassword`. An **empty** key deletes every v2 record and leaves
/// everything else in the file alone, which is how one file can hold v1 entries
/// under `[Password]` and v2 records at the same time.
pub fn v2_del(path: &Path, key: &[u8]) -> bool {
    let Some(mut lines) = read_lines(path) else {
        return false;
    };
    if key.is_empty() {
        let before = lines.len();
        lines.retain(|l| parse(l).is_none());
        if lines.len() == before {
            // Nothing to do, but upstream still rewrites and still says yes.
            return write_lines(path, &lines);
        }
        return write_lines(path, &lines);
    }
    let Some(i) = search(&lines, key) else {
        return false;
    };
    lines.remove(i);
    write_lines(path, &lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("tt-ttl-pwd-{tag}"));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A counter standing in for `rand()`, so a record is reproducible.
    fn counting() -> impl FnMut() -> u32 {
        let mut n = 0u32;
        move || {
            n = n.wrapping_add(7);
            n
        }
    }

    /// Records produced by `ttmenc.c` **compiled and run**, with `rand()`
    /// replaced by the counter above so the output is reproducible. Nothing
    /// else about the function was touched. Getting these by reading the C
    /// would have missed the bias sequence; getting them by running it is the
    /// same discipline `oracle/` uses on the VT engine.
    const GOLDEN: &[(&str, &str)] = &[
        ("password", "(ZNku3=FcV,:REx~A4'cMttJ"),
        ("x", "(\\NeuJ"),
        ("ab", "(VN{u6=h"),
        ("abc", "(VN{u7=6c*"),
        ("abcd", "(VN{u7=6cS,CRh"),
        ("hunter2", "(XNluC=AcW,IR=x#A$']M*"),
        (
            "The quick brown fox!",
            "(SNkuO=8c$,JRmx&A2'SMqtv<Zb!+KQYwi@N&YLQs4;WaQ*0PDv1?x%h",
        ),
    ];

    #[test]
    fn v1_matches_the_records_upstreams_own_encoder_produced() {
        for (plain, want) in GOLDEN {
            assert_eq!(
                v1_encrypt(plain.as_bytes(), counting()),
                want.as_bytes(),
                "encoding {plain}"
            );
            assert_eq!(v1_decrypt(want.as_bytes()), plain.as_bytes(), "{plain}");
        }
        // ...and with the randomness turned off entirely, which pins the
        // rolling bias on its own: the first character is the bias itself and
        // the last is the complement under the sixth value of the cycle.
        let enc = v1_encrypt(b"password", || 0);
        assert_eq!(enc, b"!L@V`u!c@l`I!M@!`.!V@``Q");
        assert_eq!(v1_decrypt(&enc), b"password");
    }

    #[test]
    fn v1_round_trips_at_every_length_that_shifts_the_bit_window() {
        // The bitstream regroups eight bits into six, so the interesting
        // lengths are the ones where the two line up differently. Three is the
        // period; go well past it, and past the point where the encoded form
        // would have overflowed upstream's 512-byte buffer.
        for n in 1..=400usize {
            let plain: Vec<u8> = (0..n).map(|i| b'!' + (i % 90) as u8).collect();
            let enc = v1_encrypt(&plain, counting());
            assert_eq!(v1_decrypt(&enc), plain, "length {n}");
            // `2·ceil(4n/3) + 2`, which upstream's own run confirms.
            assert_eq!(enc.len(), 2 * (4 * n).div_ceil(3) + 2, "length {n}");
            // Everything written is printable ASCII, which is what lets the
            // record live in an INI file.
            assert!(enc.iter().all(|&c| (0x21..=0x7e).contains(&c)), "{n}");
        }
    }

    #[test]
    fn v1_passes_the_length_upstreams_buffer_could_not_hold() {
        // 190 characters encode to 510 and fit a `char[512]` with the
        // terminator. 191 encode to 512 and need 513. Both figures are
        // upstream's compiled output, not arithmetic.
        assert_eq!(v1_encrypt(&[b'x'; 190], counting()).len(), 510);
        let enc = v1_encrypt(&[b'x'; 191], counting());
        assert_eq!(enc.len(), 512);
        assert_eq!(v1_decrypt(&enc), vec![b'x'; 191]);
    }

    #[test]
    fn v1_carries_every_byte_value_and_rejects_a_broken_record() {
        // Upstream round-trips 1..=255 too — a TTL string is bytes, and a
        // password with a high byte in it must not come back mangled.
        for c in 1..=255u8 {
            assert_eq!(v1_decrypt(&v1_encrypt(&[c], counting())), vec![c], "{c}");
        }

        let mut enc = v1_encrypt(b"hunter2", counting());
        let last = enc.len() - 1;
        enc[last] = enc[last].wrapping_add(1);
        assert!(v1_decrypt(&enc).is_empty());
        assert!(v1_decrypt(b"").is_empty());
        // The empty password is not stored at all, which is why `setpassword`
        // refuses one.
        assert!(v1_encrypt(b"", counting()).is_empty());
    }

    #[test]
    fn base64_matches_upstreams_alphabet_and_padding() {
        assert_eq!(b64encode(b""), b"");
        assert_eq!(b64encode(b"f"), b"Zg==");
        assert_eq!(b64encode(b"fo"), b"Zm8=");
        assert_eq!(b64encode(b"foo"), b"Zm9v");
        assert_eq!(b64encode(b"foobar"), b"Zm9vYmFy");
        assert_eq!(b64decode(b"Zm9vYmFy"), b"foobar");
        assert_eq!(b64decode(b"Zg=="), b"f");
        assert_eq!(b64decode(b"Zm8="), b"fo");
        // Whitespace is skipped; anything else ends the decode where it stands,
        // which is how a corrupt line comes out the wrong length and is then
        // passed over rather than reported.
        assert_eq!(b64decode(b"Zm9v\r\nYmFy"), b"foobar");
        assert_eq!(b64decode(b"Zm9v*YmFy"), b"foo");
    }

    #[test]
    fn v2_round_trips_through_a_file() {
        let d = scratch("v2");
        let f = d.join("password.dat");
        assert!(!v2_is(&f, b"acct"));
        assert!(v2_get(&f, b"acct", b"master").is_none());

        assert!(v2_set(&f, b"acct", b"hunter2", b"master"));
        assert!(v2_is(&f, b"acct"));
        assert_eq!(
            v2_get(&f, b"acct", b"master").as_deref(),
            Some(&b"hunter2"[..])
        );

        // The wrong <encryptstr> fails the HMAC rather than returning rubbish.
        assert!(v2_get(&f, b"acct", b"wrong").is_none());
        // The key name is hashed, so a near miss is a miss.
        assert!(!v2_is(&f, b"acc"));

        // One line, 508 characters, CRLF.
        let raw = std::fs::read(&f).unwrap();
        assert_eq!(raw.len(), B64_LEN + 2);
        assert_eq!(&raw[B64_LEN..], b"\r\n");
    }

    #[test]
    fn v2_replaces_in_place_and_deletes_by_key() {
        let d = scratch("v2edit");
        let f = d.join("password.dat");
        assert!(v2_set(&f, b"a", b"one", b"k"));
        assert!(v2_set(&f, b"b", b"two", b"k"));
        assert_eq!(std::fs::read(&f).unwrap().len(), 2 * (B64_LEN + 2));

        // Replacing a key keeps the file at two records, in order.
        assert!(v2_set(&f, b"a", b"three", b"k"));
        assert_eq!(std::fs::read(&f).unwrap().len(), 2 * (B64_LEN + 2));
        assert_eq!(v2_get(&f, b"a", b"k").as_deref(), Some(&b"three"[..]));
        assert_eq!(v2_get(&f, b"b", b"k").as_deref(), Some(&b"two"[..]));

        // Setting the same password again is a no-op that still reports success.
        assert!(v2_set(&f, b"a", b"three", b"k"));

        assert!(v2_del(&f, b"a"));
        assert!(!v2_is(&f, b"a"));
        assert!(v2_is(&f, b"b"));
        // A key that is not there is a failure, not a silent success.
        assert!(!v2_del(&f, b"a"));
    }

    #[test]
    fn v2_leaves_everything_that_is_not_a_record_alone() {
        let d = scratch("v2mixed");
        let f = d.join("password.dat");
        // The two formats share a filename in the documentation's own example,
        // so a v1 INI body and v2 records have to survive each other.
        std::fs::write(&f, b"[Password]\r\nacct=Ab3!x\r\n").unwrap();
        assert!(v2_set(&f, b"acct", b"hunter2", b"k"));
        assert_eq!(v2_get(&f, b"acct", b"k").as_deref(), Some(&b"hunter2"[..]));

        // An empty key deletes every v2 record and nothing else.
        assert!(v2_del(&f, b""));
        assert_eq!(std::fs::read(&f).unwrap(), b"[Password]\r\nacct=Ab3!x\r\n");
        assert!(!v2_is(&f, b"acct"));
    }

    #[test]
    fn v2_takes_a_password_of_exactly_the_maximum_and_refuses_one_more() {
        let d = scratch("v2max");
        let f = d.join("password.dat");
        let long = vec![b'p'; PWD_MAX_LEN];
        assert!(v2_set(&f, b"k", &long, b"e"));
        assert_eq!(v2_get(&f, b"k", b"e"), Some(long));
        assert!(!v2_set(&f, b"k2", &vec![b'p'; PWD_MAX_LEN + 1], b"e"));
    }

    #[test]
    fn a_tampered_record_does_not_decrypt() {
        let d = scratch("v2tamper");
        let f = d.join("password.dat");
        assert!(v2_set(&f, b"k", b"secret", b"e"));
        let mut raw = std::fs::read(&f).unwrap();
        // Flip a character inside the encrypted password, which starts at
        // byte 98 of the record and so at character 131 of the line. The key
        // hash is in the clear before it, which is why the record is still
        // found — it is the HMAC that refuses it.
        raw[200] = if raw[200] == b'A' { b'B' } else { b'A' };
        std::fs::write(&f, &raw).unwrap();
        // Still recognised as a record under that key...
        assert!(v2_is(&f, b"k"));
        // ...and still refused.
        assert!(v2_get(&f, b"k", b"e").is_none());
    }
}

//! `sprintf` and `sprintf2`, and the C `printf` underneath them.
//!
//! Upstream hands each conversion to the CRT's `_snprintf_s` one at a time
//! (`ttl.cpp:4408`), having first checked the spec with a **regular
//! expression** — `^%[-+0 #]*([1-9][0-9]*|\*)?(?:\.([0-9]*|\*))?$`, compiled
//! with Oniguruma, ASCII, default syntax, on every call. That pattern is fixed
//! and its two groups only ever answer "is this a `*`", so nothing here needs a
//! regex engine; the validation is the same grammar written out, and
//! [`crate::regex`] is left for the commands that genuinely need it.
//!
//! What does have to be reproduced is the CRT, because the output is C's and
//! not Rust's: `{:e}` gives `1.5e0` where `%e` gives `1.500000e+00`, and no
//! Rust format has `%g`, `%o` or `%a` at all. So the conversions are written
//! out, and the goldens in the tests come from a C program compiled and run
//! rather than from what C ought to do.
//!
//! **A TTL macro has no floating-point type**, which is why `%f` and its
//! relatives read a *string* argument and put it through `atof`: `sprintf '%f'
//! '1.5'`. The quotes are not optional and their absence is not an error — see
//! [`atof`], which answers 0 for anything it cannot read, exactly as C does.

use crate::error::{TtlError, TtlResult};
use crate::expr;
use crate::host::ScriptHost;
use crate::interp::Interp;
use crate::lexer::MAX_STR_LEN;
use crate::rsv::Rsv;

/// What a conversion wants for its value.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ArgType {
    Integer,
    Double,
    Str,
}

/// One `%...` conversion, as far as the spec goes.
#[derive(Clone, Copy, Debug, Default)]
struct Spec {
    minus: bool,
    plus: bool,
    zero: bool,
    space: bool,
    hash: bool,
    width: Option<usize>,
    /// `None` is "no `.` at all", which is not the same as `.0`.
    precision: Option<usize>,
    conv: u8,
}

/// The grammar upstream checks with Oniguruma, written out.
///
/// `body` is everything between the `%` and the conversion character. It must
/// be flags, then an optional width that is either `[1-9][0-9]*` or `*`, then
/// optionally a `.` followed by `[0-9]*` or `*` — and nothing else. Note what
/// the pattern does **not** allow: a width may not start with `0` (that digit
/// is a flag and the rest would have to be non-zero), and there are no length
/// modifiers, so `%ld` is a syntax error rather than a long.
///
/// The two `bool`s are "the width was `*`" and "the precision was `*`".
fn parse_spec(body: &[u8], conv: u8) -> Option<(Spec, bool, bool)> {
    let mut s = Spec {
        conv,
        ..Spec::default()
    };
    let mut i = 0;
    while i < body.len() {
        match body[i] {
            b'-' => s.minus = true,
            b'+' => s.plus = true,
            b'0' => s.zero = true,
            b' ' => s.space = true,
            b'#' => s.hash = true,
            _ => break,
        }
        i += 1;
    }

    let mut width_star = false;
    if i < body.len() && body[i] == b'*' {
        width_star = true;
        i += 1;
    } else if i < body.len() && body[i].is_ascii_digit() {
        // `[1-9][0-9]*` — a leading zero was already eaten as a flag, so a
        // width of `0...` cannot get here.
        if body[i] == b'0' {
            return None;
        }
        let start = i;
        while i < body.len() && body[i].is_ascii_digit() {
            i += 1;
        }
        s.width = std::str::from_utf8(&body[start..i]).ok()?.parse().ok();
        s.width?;
    }

    let mut prec_star = false;
    if i < body.len() && body[i] == b'.' {
        i += 1;
        if i < body.len() && body[i] == b'*' {
            prec_star = true;
            i += 1;
        } else {
            let start = i;
            while i < body.len() && body[i].is_ascii_digit() {
                i += 1;
            }
            // `[0-9]*` — `%.f` is legal and means `%.0f`.
            s.precision = Some(
                std::str::from_utf8(&body[start..i])
                    .ok()?
                    .parse()
                    .unwrap_or(0),
            );
        }
    }

    (i == body.len()).then_some((s, width_star, prec_star))
}

/// Which kind of value a conversion character takes, or `None` if it is not one.
fn arg_type(c: u8) -> Option<ArgType> {
    match c {
        b'c' | b'd' | b'i' | b'o' | b'u' | b'x' | b'X' => Some(ArgType::Integer),
        b'e' | b'E' | b'f' | b'g' | b'G' | b'a' | b'A' => Some(ArgType::Double),
        b's' => Some(ArgType::Str),
        _ => None,
    }
}

/// `atof` — C's, which is `strtod` with the error reporting thrown away.
///
/// Leading whitespace, an optional sign, digits with an optional point and an
/// optional exponent; it stops at the first character it cannot use and
/// answers **0 for anything it never started**, which is why `sprintf '%f'
/// 'abc'` prints `0.000000` rather than failing. `inf` and `nan` are C99's and
/// are accepted too.
pub fn atof(s: &[u8]) -> f64 {
    let t = s.iter().position(|c| !c.is_ascii_whitespace()).unwrap_or(0);
    let s = &s[t..];
    let mut end = 0;
    let mut seen_digit = false;
    let mut i = 0;
    if i < s.len() && (s[i] == b'+' || s[i] == b'-') {
        i += 1;
    }
    // `inf`, `infinity`, `nan` — matched case-insensitively, longest first.
    for name in ["infinity", "inf", "nan"] {
        if s[i..].len() >= name.len() && s[i..i + name.len()].eq_ignore_ascii_case(name.as_bytes())
        {
            return std::str::from_utf8(&s[..i + name.len()])
                .ok()
                .and_then(|t| t.parse().ok())
                .unwrap_or(0.0);
        }
    }
    while i < s.len() && s[i].is_ascii_digit() {
        i += 1;
        seen_digit = true;
    }
    if i < s.len() && s[i] == b'.' {
        i += 1;
        while i < s.len() && s[i].is_ascii_digit() {
            i += 1;
            seen_digit = true;
        }
    }
    if seen_digit {
        end = i;
        if i < s.len() && (s[i] == b'e' || s[i] == b'E') {
            let mut j = i + 1;
            if j < s.len() && (s[j] == b'+' || s[j] == b'-') {
                j += 1;
            }
            if j < s.len() && s[j].is_ascii_digit() {
                while j < s.len() && s[j].is_ascii_digit() {
                    j += 1;
                }
                end = j;
            }
        }
    }
    std::str::from_utf8(&s[..end])
        .ok()
        .and_then(|t| t.parse().ok())
        .unwrap_or(0.0)
}

/// The value a conversion is formatting.
enum Value<'a> {
    Int(i32),
    Float(f64),
    Str(&'a [u8]),
}

/// One conversion, applied.
fn convert(s: &Spec, v: Value<'_>) -> Vec<u8> {
    let (body, sign): (Vec<u8>, &[u8]) = match v {
        Value::Str(bytes) => {
            let n = s.precision.unwrap_or(bytes.len()).min(bytes.len());
            (bytes[..n].to_vec(), b"")
        }
        Value::Int(n) if s.conv == b'c' => (vec![n as u8], b""),
        Value::Int(n) => return int_conv(s, n),
        Value::Float(f) => return float_conv(s, f),
    };
    pad(s, sign, &body, false)
}

/// `d`, `i`, `o`, `u`, `x`, `X`.
fn int_conv(s: &Spec, n: i32) -> Vec<u8> {
    let signed = s.conv == b'd' || s.conv == b'i';
    let (digits, sign): (String, &[u8]) = if signed {
        let mag = (n as i64).unsigned_abs();
        let sign: &[u8] = if n < 0 {
            b"-"
        } else if s.plus {
            b"+"
        } else if s.space {
            b" "
        } else {
            b""
        };
        (mag.to_string(), sign)
    } else {
        // C promotes to `unsigned int`, so a negative value wraps.
        let u = n as u32;
        let text = match s.conv {
            b'o' => format!("{u:o}"),
            b'x' => format!("{u:x}"),
            b'X' => format!("{u:X}"),
            _ => format!("{u}"),
        };
        (text, b"")
    };

    let mut body = digits.into_bytes();
    // A precision is a minimum digit count, and it turns the `0` flag off.
    if let Some(p) = s.precision {
        // `%.0d` of zero prints nothing at all, which is C and is easy to miss.
        if p == 0 && body == b"0" {
            body.clear();
        }
        while body.len() < p {
            body.insert(0, b'0');
        }
    }
    if s.hash {
        match s.conv {
            b'o' if !body.starts_with(b"0") => body.insert(0, b'0'),
            b'x' if n != 0 => {
                body.splice(0..0, *b"0x");
            }
            b'X' if n != 0 => {
                body.splice(0..0, *b"0X");
            }
            _ => {}
        }
    }
    pad(s, sign, &body, s.precision.is_none())
}

/// `e`, `E`, `f`, `g`, `G`, `a`, `A`.
fn float_conv(s: &Spec, f: f64) -> Vec<u8> {
    let sign: &[u8] = if f.is_sign_negative() {
        b"-"
    } else if s.plus {
        b"+"
    } else if s.space {
        b" "
    } else {
        b""
    };
    let mag = f.abs();

    if mag.is_nan() || mag.is_infinite() {
        let text = if mag.is_nan() { "nan" } else { "inf" };
        let text = if s.conv.is_ascii_uppercase() {
            text.to_uppercase()
        } else {
            text.to_string()
        };
        // Neither is zero-padded, whatever the flags say.
        return pad(s, sign, text.as_bytes(), false);
    }

    let p = s.precision.unwrap_or(6);
    let body = match s.conv {
        b'f' => fixed(mag, p, s.hash),
        b'e' | b'E' => exponential(mag, p, s.hash, s.conv == b'E'),
        b'a' | b'A' => hex_float(mag, s.precision, s.conv == b'A'),
        // `%g` picks between the two and, unless `#` is given, drops the
        // trailing zeros the choice left behind. A precision of 0 means 1.
        _ => {
            let p = p.max(1);
            let exp = if mag == 0.0 {
                0
            } else {
                // The exponent `%e` would have used, after its own rounding.
                let e = exponential(mag, p - 1, false, false);
                let at = e.iter().position(|&c| c == b'e').unwrap();
                std::str::from_utf8(&e[at + 1..]).unwrap().parse().unwrap()
            };
            let mut out = if exp < -4 || exp >= p as i32 {
                exponential(mag, p - 1, s.hash, s.conv == b'G')
            } else {
                fixed(mag, (p as i32 - 1 - exp) as usize, s.hash)
            };
            if !s.hash {
                trim_zeros(&mut out);
            }
            out
        }
    };
    pad(s, sign, &body, true)
}

/// `%f`'s digits, without the sign.
fn fixed(mag: f64, p: usize, hash: bool) -> Vec<u8> {
    let mut out = format!("{mag:.p$}").into_bytes();
    if hash && p == 0 {
        out.push(b'.');
    }
    out
}

/// `%e`'s digits, without the sign. C's exponent is at least two digits and
/// always signed, which Rust's `{:e}` is not.
fn exponential(mag: f64, p: usize, hash: bool, upper: bool) -> Vec<u8> {
    let text = format!("{mag:.p$e}");
    let (mantissa, exp) = text.split_once('e').expect("{:e} always writes one");
    let exp: i32 = exp.parse().expect("and a decimal exponent after it");
    let mut out = mantissa.as_bytes().to_vec();
    if hash && p == 0 {
        out.push(b'.');
    }
    out.push(if upper { b'E' } else { b'e' });
    out.push(if exp < 0 { b'-' } else { b'+' });
    out.extend_from_slice(format!("{:02}", exp.abs()).as_bytes());
    out
}

/// `%a` — C99's hexadecimal float, normalised to a leading `1` as glibc and
/// the MSVC CRT both do.
///
/// With no precision, as many hex digits as the value needs and no more; with
/// one, exactly that many, rounded. Zero is `0x0p+0`.
fn hex_float(mag: f64, precision: Option<usize>, upper: bool) -> Vec<u8> {
    let bits = mag.to_bits();
    let raw_exp = ((bits >> 52) & 0x7ff) as i32;
    let frac = bits & 0x000f_ffff_ffff_ffff;

    let (lead, frac, exp) = if raw_exp == 0 {
        if frac == 0 {
            (0u8, 0u64, 0i32)
        } else {
            // Subnormal: shift up until the implicit bit is a 1, which is what
            // "normalised" costs at the bottom of the range.
            let shift = frac.leading_zeros() - 11;
            (
                1u8,
                (frac << (shift + 1)) & 0x000f_ffff_ffff_ffff,
                -1022 - shift as i32,
            )
        }
    } else {
        (1u8, frac, raw_exp - 1023)
    };

    // Thirteen hex digits hold the 52-bit fraction exactly.
    let mut digits: Vec<u8> = (0..13)
        .map(|i| ((frac >> (48 - 4 * i)) & 0xf) as u8)
        .collect();
    let mut lead = lead;

    match precision {
        Some(p) => {
            if p < digits.len() {
                // Round half away from zero on the first dropped digit, which
                // is what both CRTs do here.
                let round_up = digits[p] >= 8;
                digits.truncate(p);
                if round_up {
                    let mut i = p;
                    loop {
                        if i == 0 {
                            lead += 1;
                            break;
                        }
                        i -= 1;
                        if digits[i] == 0xf {
                            digits[i] = 0;
                        } else {
                            digits[i] += 1;
                            break;
                        }
                    }
                }
            } else {
                digits.resize(p, 0);
            }
        }
        None => {
            while digits.last() == Some(&0) {
                digits.pop();
            }
        }
    }

    let hex = |d: u8| -> u8 {
        let table = if upper {
            b"0123456789ABCDEF"
        } else {
            b"0123456789abcdef"
        };
        table[d as usize]
    };
    let mut out = if upper {
        b"0X".to_vec()
    } else {
        b"0x".to_vec()
    };
    out.push(hex(lead));
    if !digits.is_empty() {
        out.push(b'.');
        out.extend(digits.into_iter().map(hex));
    }
    out.push(if upper { b'P' } else { b'p' });
    out.push(if exp < 0 { b'-' } else { b'+' });
    out.extend_from_slice(exp.abs().to_string().as_bytes());
    out
}

/// `%g`'s tidy-up: drop trailing fraction zeros, and the point with them.
fn trim_zeros(out: &mut Vec<u8>) {
    let end = out
        .iter()
        .position(|&c| c == b'e' || c == b'E')
        .unwrap_or(out.len());
    if !out[..end].contains(&b'.') {
        return;
    }
    let mut cut = end;
    while cut > 0 && out[cut - 1] == b'0' {
        cut -= 1;
    }
    if cut > 0 && out[cut - 1] == b'.' {
        cut -= 1;
    }
    out.drain(cut..end);
}

/// Width, and where the sign and the zero padding go relative to it.
fn pad(s: &Spec, sign: &[u8], body: &[u8], zero_ok: bool) -> Vec<u8> {
    let width = s.width.unwrap_or(0);
    let len = sign.len() + body.len();
    let mut out = Vec::with_capacity(width.max(len));
    if len >= width {
        out.extend_from_slice(sign);
        out.extend_from_slice(body);
        return out;
    }
    let fill = width - len;
    if s.minus {
        // `-` beats `0`, which is C and is the usual reason a `%-05d` looks
        // wrong to whoever wrote it.
        out.extend_from_slice(sign);
        out.extend_from_slice(body);
        out.resize(width, b' ');
    } else if s.zero && zero_ok {
        out.extend_from_slice(sign);
        out.resize(sign.len() + fill, b'0');
        out.extend_from_slice(body);
    } else {
        out.resize(fill, b' ');
        out.extend_from_slice(sign);
        out.extend_from_slice(body);
    }
    out
}

impl Interp {
    /// Dispatch for the commands in this file. `None` means "not one of mine".
    pub(crate) fn sprintf_command(
        &mut self,
        host: &mut dyn ScriptHost,
        w: Rsv,
    ) -> Option<TtlResult<()>> {
        Some(match w {
            Rsv::Sprintf => self.cmd_sprintf(host, false),
            Rsv::Sprintf2 => self.cmd_sprintf(host, true),
            _ => return None,
        })
    }

    /// `sprintf <format> [<arg> ...]` → `inputstr`, and
    /// `sprintf2 <strvar> <format> [<arg> ...]` → the variable.
    ///
    /// `result` is **0 for success**, which is the opposite way round from
    /// nearly every other command here: 1 is a format that would not parse as
    /// a string, 2 a conversion spec the grammar refused, 3 an argument that
    /// was missing or of the wrong type, and 4 — `sprintf2` only — a first
    /// argument that is not a string variable. `result` is set before the
    /// error is reported, so a macro with `errorhandler` on can read it.
    ///
    /// **The arguments are pulled as each conversion is reached**, so their
    /// types are decided by the format and a wrong one stops the whole command
    /// rather than printing something odd. `%s` will **not** quietly take a
    /// number: `GetStrVal` is the arm of `GetStrVal2` with the automatic
    /// conversion switched off, so `sprintf '%s' 42` is a type mismatch. And
    /// `%f` and its relatives take a **string** and put it through [`atof`],
    /// because TTL has no floating-point type — the quotes in
    /// `sprintf '%f' '1.5'` are load-bearing, and without them the `1.5` is
    /// read as the integer 1 and then rejected.
    ///
    /// Anything the grammar does not recognise stays in the output verbatim:
    /// a `%` with no conversion after it, or a spec that runs into the end of
    /// the format, is appended as it stands rather than being an error.
    fn cmd_sprintf(&mut self, host: &mut dyn ScriptHost, to_var: bool) -> TtlResult<()> {
        let var = if to_var {
            match expr::get_str_var(&mut self.lx, &mut self.vars) {
                Ok(v) => Some(v),
                Err(e) => {
                    self.set_result(4);
                    return Err(e);
                }
            }
        } else {
            None
        };

        let fmt = match expr::get_str_val(&mut self.lx, &mut self.vars) {
            Ok(f) => f,
            Err(e) => {
                self.set_result(1);
                return Err(e);
            }
        };

        let mut out: Vec<u8> = Vec::new();
        let mut spec: Vec<u8> = Vec::new();
        for &c in fmt.iter() {
            if spec.is_empty() {
                if c == b'%' {
                    spec.push(c);
                } else if out.len() < MAX_STR_LEN - 1 {
                    out.push(c);
                } else {
                    // Upstream breaks out of the whole loop here rather than
                    // truncating the tail, so a long literal drops everything
                    // after it — conversions included.
                    break;
                }
                continue;
            }

            if c == b'%' {
                if spec.len() == 1 {
                    out.push(b'%');
                    spec.clear();
                } else {
                    out.extend_from_slice(&spec);
                    spec.clear();
                    spec.push(b'%');
                }
                continue;
            }

            let Some(kind) = arg_type(c) else {
                spec.push(c);
                continue;
            };

            let Some((mut s, width_star, prec_star)) = parse_spec(&spec[1..], c) else {
                self.set_result(2);
                return Err(TtlError::Syntax);
            };
            if width_star {
                match expr::get_int_val(&mut self.lx, &mut self.vars) {
                    // A negative `*` width is C's `-` flag with the magnitude,
                    // which upstream gets from the CRT for free.
                    Ok(w) if w < 0 => {
                        s.minus = true;
                        s.width = Some(w.unsigned_abs() as usize);
                    }
                    Ok(w) => s.width = Some(w as usize),
                    Err(e) => {
                        self.set_result(3);
                        return Err(e);
                    }
                }
            }
            if prec_star {
                match expr::get_int_val(&mut self.lx, &mut self.vars) {
                    // ...and a negative `*` precision is no precision at all.
                    Ok(p) if p < 0 => s.precision = None,
                    Ok(p) => s.precision = Some(p as usize),
                    Err(e) => {
                        self.set_result(3);
                        return Err(e);
                    }
                }
            }

            let piece = match kind {
                ArgType::Integer => match expr::get_int_val(&mut self.lx, &mut self.vars) {
                    Ok(n) => convert(&s, Value::Int(n)),
                    Err(e) => {
                        self.set_result(3);
                        return Err(e);
                    }
                },
                ArgType::Str | ArgType::Double => {
                    match expr::get_str_val(&mut self.lx, &mut self.vars) {
                        Ok(text) if kind == ArgType::Str => convert(&s, Value::Str(&text)),
                        Ok(text) => convert(&s, Value::Float(atof(&text))),
                        Err(e) => {
                            self.set_result(3);
                            return Err(e);
                        }
                    }
                }
            };
            out.extend_from_slice(&piece);
            spec.clear();
        }
        // A spec that ran into the end of the format is emitted as it stands.
        out.extend_from_slice(&spec);
        out.truncate(MAX_STR_LEN - 1);

        // **No end-of-line check.** The arguments are consumed by the
        // conversions, so anything the format did not ask for is simply left
        // on the line and ignored: `sprintf '%d' 1 2 3` prints `1`.
        match var {
            Some(v) => self.vars.set_str(v, &out),
            None => self.set_input_str(&out),
        }
        let _ = host;
        self.set_result(0);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::RecordingHost;
    use crate::TtlError;

    fn run(src: &str) -> RecordingHost {
        let mut host = RecordingHost::new();
        let mut it = Interp::new("t.ttl", src.as_bytes().to_vec(), &mut host);
        it.run(&mut host);
        host
    }

    fn out(src: &str) -> String {
        let h = run(src);
        assert!(h.errors.is_empty(), "unexpected errors: {:?}", h.errors);
        String::from_utf8_lossy(&h.output).into_owned()
    }

    /// Format one spec through the interpreter and return what came out.
    fn f(src: &str) -> String {
        out(&format!("sprintf {src}\ndispstr inputstr"))
    }

    /// `(format, argument, what C printed)`, produced by compiling and running
    /// a C program that calls `snprintf` with exactly these — not by writing
    /// down what C ought to print. The integer cases pass the argument as an
    /// `int` and the float cases as a `double` from `atof`, which is what
    /// `TTLSprintf` does.
    const GOLDEN_INT: &[(&str, i32, &str)] = &[
        ("%d", 42, "42"),
        ("%d", -42, "-42"),
        ("%5d", 42, "   42"),
        ("%-5d|", 42, "42   |"),
        ("%05d", 42, "00042"),
        ("%05d", -42, "-0042"),
        ("%-05d|", 42, "42   |"),
        ("%+d", 42, "+42"),
        ("% d", 42, " 42"),
        ("%.5d", 42, "00042"),
        ("%8.5d", 42, "   00042"),
        ("%.0d", 0, ""),
        ("%.0d", 7, "7"),
        ("%x", 255, "ff"),
        ("%X", 255, "FF"),
        ("%#x", 255, "0xff"),
        ("%#X", 255, "0XFF"),
        ("%#x", 0, "0"),
        ("%o", 8, "10"),
        ("%#o", 8, "010"),
        ("%#o", 0, "0"),
        ("%u", -1, "4294967295"),
        ("%x", -1, "ffffffff"),
        ("%c", 65, "A"),
        ("%5c|", 65, "    A|"),
        ("%i", -7, "-7"),
    ];

    const GOLDEN_FLOAT: &[(&str, &str, &str)] = &[
        ("%f", "1.5", "1.500000"),
        ("%f", "-1.5", "-1.500000"),
        ("%.2f", "3.14159", "3.14"),
        ("%.0f", "2.5", "2"),
        ("%#.0f", "2.5", "2."),
        ("%10.2f", "3.14159", "      3.14"),
        ("%-10.2f|", "3.14159", "3.14      |"),
        ("%010.2f", "3.14159", "0000003.14"),
        ("%e", "1500", "1.500000e+03"),
        ("%E", "1500", "1.500000E+03"),
        ("%.2e", "0.000123", "1.23e-04"),
        ("%e", "0", "0.000000e+00"),
        ("%g", "1500", "1500"),
        ("%g", "0.0001", "0.0001"),
        ("%g", "0.00001", "1e-05"),
        ("%g", "1500000", "1.5e+06"),
        ("%.3g", "1234", "1.23e+03"),
        ("%#g", "1.5", "1.50000"),
        ("%G", "0.00001", "1E-05"),
        ("%g", "100", "100"),
        ("%f", "abc", "0.000000"),
        ("%f", "  2.5xyz", "2.500000"),
        ("%f", "1e3", "1000.000000"),
        ("%a", "1", "0x1p+0"),
        ("%a", "0.5", "0x1p-1"),
        ("%a", "1.5", "0x1.8p+0"),
        ("%A", "1.5", "0X1.8P+0"),
        ("%a", "0", "0x0p+0"),
        ("%.2a", "1.5", "0x1.80p+0"),
    ];

    #[test]
    fn the_integer_conversions_are_what_c_printed() {
        for (spec, arg, want) in GOLDEN_INT {
            assert_eq!(&f(&format!("'{spec}' {arg}")), want, "{spec} {arg}");
        }
    }

    #[test]
    fn the_floating_conversions_are_what_c_printed() {
        for (spec, arg, want) in GOLDEN_FLOAT {
            assert_eq!(&f(&format!("'{spec}' '{arg}'")), want, "{spec} '{arg}'");
        }
    }

    #[test]
    fn the_string_conversion_takes_a_precision_as_a_maximum() {
        assert_eq!(f("'%s' 'hello'"), "hello");
        assert_eq!(f("'%8s|' 'hi'"), "      hi|");
        assert_eq!(f("'%-8s|' 'hi'"), "hi      |");
        assert_eq!(f("'%.2s' 'hello'"), "he");
        assert_eq!(f("'%8.2s|' 'hello'"), "      he|");
        // `%s` will **not** take an integer. `GetStrVal` is the no-conversion
        // form (`ttmparse.cpp:GetStrVal2` with `AutoConversion` false), so a
        // number is a type mismatch and `result` 3 — write `str2int`'s
        // opposite, or use `%d`.
        let h = run("sprintf '%s' 42");
        assert_eq!(h.errors.first().map(|e| e.0), Some(TtlError::TypeMismatch));
    }

    #[test]
    fn a_star_takes_its_width_from_the_argument_list() {
        assert_eq!(f("'%*d' 5 42"), "   42");
        assert_eq!(f("'%-*d|' 5 42"), "42   |");
        assert_eq!(f("'%.*f' 3 '3.14159'"), "3.142");
        assert_eq!(f("'%*.*f' 10 2 '3.14159'"), "      3.14");
        // A negative width is the `-` flag with the magnitude, as in C.
        assert_eq!(f("'%*d|' -5 42"), "42   |");
    }

    #[test]
    fn several_conversions_and_the_literals_between_them() {
        assert_eq!(f("'%s is %d years' 'Bob' 30"), "Bob is 30 years");
        assert_eq!(f("'100%%'"), "100%");
        assert_eq!(f("'%d%%'  50"), "50%");
        // Two `%` in a row where the first one had a spec: the incomplete one
        // is emitted verbatim and the second starts afresh.
        assert_eq!(f("'%5%d' 7"), "%57");
    }

    #[test]
    fn an_unfinished_or_unknown_spec_comes_out_as_it_stands() {
        assert_eq!(f("'abc%'"), "abc%");
        assert_eq!(f("'%5'"), "%5");
        // `%z` is not a conversion, so the `z` joins the spec and the whole
        // thing reaches the end of the format unconverted.
        assert_eq!(f("'%z'"), "%z");
    }

    #[test]
    fn the_result_codes_say_which_argument_went_wrong() {
        // 0 is success, which is the opposite way round from everything else.
        assert_eq!(out("sprintf '%d' 1\ndispstr result"), "0");
        // 2 — a spec the grammar refuses. There are no length modifiers.
        let h = run("sprintf '%ld' 1\ndispstr result");
        assert_eq!(h.errors.first().map(|e| e.0), Some(TtlError::Syntax));
        // 3 — the argument is missing.
        let h = run("sprintf '%d'");
        assert_eq!(h.errors.first().map(|e| e.0), Some(TtlError::Syntax));
        // ...and a type mismatch is 3 as well.
        let h = run("s = 'x'\nsprintf '%d' s");
        assert_eq!(h.errors.first().map(|e| e.0), Some(TtlError::TypeMismatch));
    }

    #[test]
    fn sprintf2_writes_a_variable_and_sprintf_writes_inputstr() {
        assert_eq!(
            out("sprintf2 s '%04d' 7\ndispstr s';'inputstr'!'"),
            "0007;!"
        );
        assert_eq!(out("sprintf '%04d' 7\ndispstr inputstr"), "0007");
        // A first argument that is not a string variable is `result` 4 — a
        // syntax error when it is not an identifier at all, a type mismatch
        // when it is an identifier of the wrong type.
        let h = run("sprintf2 3 '%d' 1");
        assert_eq!(h.errors.first().map(|e| e.0), Some(TtlError::Syntax));
        let h = run("n = 1\nsprintf2 n '%d' 1");
        assert_eq!(h.errors.first().map(|e| e.0), Some(TtlError::TypeMismatch));
    }

    #[test]
    fn arguments_the_format_never_asked_for_are_ignored() {
        // No end-of-line check: the conversions take what they need and the
        // rest of the line is dropped without complaint.
        assert_eq!(f("'%d' 1 2 3"), "1");
        assert_eq!(f("'no conversions' 1 2 3"), "no conversions");
    }

    #[test]
    fn atof_reads_what_c_reads_and_gives_up_where_c_gives_up() {
        assert_eq!(atof(b"1.5"), 1.5);
        assert_eq!(atof(b"  -2.5e2xyz"), -250.0);
        assert_eq!(atof(b"abc"), 0.0);
        assert_eq!(atof(b""), 0.0);
        assert_eq!(atof(b"."), 0.0);
        // An exponent with no digits after it is not part of the number.
        assert_eq!(atof(b"5e"), 5.0);
        assert_eq!(atof(b"5e+"), 5.0);
        assert_eq!(atof(b"inf"), f64::INFINITY);
        assert!(atof(b"-NaN").is_nan());
    }
}

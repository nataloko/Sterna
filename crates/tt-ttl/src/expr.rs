//! The expression evaluator, ported from `ttmparse.cpp`'s eleven precedence
//! levels (`GetFactor` through `GetExpression`, `:979-1633`).
//!
//! Two things about it are worth knowing before reading any of it.
//!
//! **A string short-circuits the whole chain.** Every level begins by asking
//! the level below for an operand, and returns immediately if what comes back
//! is not an integer — so in `a + b` with `a` a string, the `+ b` is never
//! looked at. The caller then finds unconsumed text and reports a syntax
//! error. That is why TTL has `strconcat` and no `+` on strings.
//!
//! **A string's "value" is a reference to where it lives**, not its bytes.
//! Upstream returns the variable id in the same `int` it returns numbers in,
//! and the caller reads the text back out of the table with `StrVarPtr`. It is
//! reproduced because it is observable: an expression cannot build a new
//! string, only name one that already exists.

use crate::error::{TtlError, TtlResult};
use crate::lexer::{Lexer, MAX_STR_LEN};
use crate::rsv::Rsv;
use crate::vars::{VarRef, VarType, Vars};

/// What an expression evaluated to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Eval {
    Int(i32),
    Str(VarRef),
    /// An array named without an index. Every caller turns this into a type
    /// mismatch; it exists so that they can.
    IntArray(usize),
    StrArray(usize),
    /// A label named in an expression.
    ///
    /// Upstream's `GetFactor` has no `case TypLabel`, so it returns the label's
    /// type beside an uninitialised value. Every caller rejects the type before
    /// looking at the value, which is why the read is harmless there and why
    /// carrying no value here loses nothing.
    Label(usize),
}

impl Eval {
    pub fn var_type(self) -> VarType {
        match self {
            Eval::Int(_) => VarType::Integer,
            Eval::Str(_) => VarType::String,
            Eval::IntArray(_) => VarType::IntArray,
            Eval::StrArray(_) => VarType::StrArray,
            Eval::Label(_) => VarType::Label,
        }
    }
}

type Level = fn(&mut Lexer, &mut Vars) -> TtlResult<Option<Eval>>;

/// One binary precedence level: an operand, then any run of accepted operators.
///
/// The shape is upstream's, seven times over — including the two early exits
/// that make a non-integer operand end the level without consuming anything
/// further, and the rewind that puts back an operator this level does not want.
fn binary(
    lx: &mut Lexer,
    vars: &mut Vars,
    next: Level,
    accepts: fn(Rsv) -> bool,
    apply: fn(i32, Rsv, i32) -> TtlResult<i32>,
) -> TtlResult<Option<Eval>> {
    let first = match next(lx, vars)? {
        None => return Ok(None),
        Some(v) => v,
    };
    let Eval::Int(mut acc) = first else {
        return Ok(Some(first));
    };

    loop {
        let p = lx.ptr;
        let Some(op) = lx.operator() else {
            return Ok(Some(Eval::Int(acc)));
        };
        if !accepts(op) {
            lx.ptr = p;
            return Ok(Some(Eval::Int(acc)));
        }
        match next(lx, vars)? {
            None => return Err(TtlError::Syntax),
            Some(Eval::Int(rhs)) => acc = apply(acc, op, rhs)?,
            Some(_) => return Err(TtlError::TypeMismatch),
        }
    }
}

/// Precedence 1 — a variable, a number, a parenthesised expression, or a unary
/// `+ - ~ ! not`.
fn factor(lx: &mut Lexer, vars: &mut Vars) -> TtlResult<Option<Eval>> {
    let p = lx.ptr;
    let r = factor_inner(lx, vars);
    if r.is_err() {
        lx.ptr = p;
    }
    r
}

fn factor_inner(lx: &mut Lexer, vars: &mut Vars) -> TtlResult<Option<Eval>> {
    if let Some(name) = lx.identifier() {
        if let Some(w) = crate::lexer::check_reserved(&name) {
            return unary(lx, vars, w).map(Some);
        }
        let Some((id, ty)) = vars.find(&name) else {
            return Err(TtlError::VarNotInit);
        };
        return Ok(Some(match ty {
            VarType::Integer => Eval::Int(vars.int_at(VarRef::Scalar(id))),
            VarType::String => Eval::Str(VarRef::Scalar(id)),
            VarType::IntArray => match index(lx, vars)? {
                Some(i) => Eval::Int(vars.int_at(vars.elem(id, i)?)),
                None => Eval::IntArray(id),
            },
            VarType::StrArray => match index(lx, vars)? {
                Some(i) => Eval::Str(vars.elem(id, i)?),
                None => Eval::StrArray(id),
            },
            VarType::Label => Eval::Label(id),
            VarType::Unknown | VarType::Logical => Eval::Label(id),
        }));
    }

    if let Some(n) = lx.number() {
        return Ok(Some(Eval::Int(n)));
    }

    if let Some(w) = lx.operator() {
        return unary(lx, vars, w).map(Some);
    }

    if lx.first_char() == b'(' {
        let Some(v) = get_expression(lx, vars)? else {
            return Err(TtlError::Syntax);
        };
        if lx.first_char() != b')' {
            return Err(TtlError::CloseParent);
        }
        return Ok(Some(v));
    }

    Ok(None)
}

/// The unary arm shared by the two ways a prefix operator can be spelled.
///
/// `+` is accepted and does nothing; every reserved word that is not one of the
/// five is a syntax error, which is how `sendln` inside an expression is caught.
fn unary(lx: &mut Lexer, vars: &mut Vars, w: Rsv) -> TtlResult<Eval> {
    if !matches!(w, Rsv::Plus | Rsv::Minus | Rsv::BNot | Rsv::LNot) {
        return Err(TtlError::Syntax);
    }
    let v = match factor(lx, vars)? {
        None => return Err(TtlError::Syntax),
        Some(Eval::Int(v)) => v,
        Some(_) => return Err(TtlError::TypeMismatch),
    };
    Ok(Eval::Int(match w {
        Rsv::Plus => v,
        Rsv::Minus => v.wrapping_neg(),
        Rsv::BNot => !v,
        _ => i32::from(v == 0),
    }))
}

/// Precedence 2 — `* / %`, and the only place a division by zero is caught.
fn multiplication(lx: &mut Lexer, vars: &mut Vars) -> TtlResult<Option<Eval>> {
    binary(
        lx,
        vars,
        factor,
        |w| matches!(w, Rsv::Mul | Rsv::Div | Rsv::Mod),
        |a, w, b| {
            if b == 0 && w != Rsv::Mul {
                return Err(TtlError::DivByZero);
            }
            Ok(match w {
                Rsv::Mul => a.wrapping_mul(b),
                Rsv::Div => a.wrapping_div(b),
                _ => a.wrapping_rem(b),
            })
        },
    )
}

/// Precedence 3 — `+ -`.
fn addition(lx: &mut Lexer, vars: &mut Vars) -> TtlResult<Option<Eval>> {
    binary(
        lx,
        vars,
        multiplication,
        |w| matches!(w, Rsv::Plus | Rsv::Minus),
        |a, w, b| {
            Ok(if w == Rsv::Plus {
                a.wrapping_add(b)
            } else {
                a.wrapping_sub(b)
            })
        },
    )
}

/// Precedence 4 — `<< >> >>>`.
///
/// A shift by more than the width of the type is defined here rather than left
/// to the compiler, and a *negative* shift reverses the direction: `<<` is
/// implemented by negating the count and falling into the same ladder, so
/// `x << -1` is a right shift.
fn bit_shift(lx: &mut Lexer, vars: &mut Vars) -> TtlResult<Option<Eval>> {
    const INT_BIT: i32 = 32;
    binary(
        lx,
        vars,
        addition,
        |w| matches!(w, Rsv::ARShift | Rsv::ALShift | Rsv::LRShift),
        |a, w, b| {
            let b = if w == Rsv::ALShift {
                b.wrapping_neg()
            } else {
                b
            };
            Ok(if b <= -INT_BIT {
                0
            } else if b < 0 {
                a.wrapping_shl((-b) as u32)
            } else if b == 0 {
                a
            } else if b < INT_BIT {
                if w == Rsv::LRShift {
                    ((a as u32) >> b) as i32
                } else {
                    a >> b
                }
            } else if a > 0 || w == Rsv::LRShift {
                0
            } else {
                !0
            })
        },
    )
}

/// Precedence 5 — `&`.
fn bit_and(lx: &mut Lexer, vars: &mut Vars) -> TtlResult<Option<Eval>> {
    binary(lx, vars, bit_shift, |w| w == Rsv::BAnd, |a, _, b| Ok(a & b))
}

/// Precedence 6 — `^`.
fn bit_xor(lx: &mut Lexer, vars: &mut Vars) -> TtlResult<Option<Eval>> {
    binary(lx, vars, bit_and, |w| w == Rsv::BXor, |a, _, b| Ok(a ^ b))
}

/// Precedence 7 — `|`.
fn bit_or(lx: &mut Lexer, vars: &mut Vars) -> TtlResult<Option<Eval>> {
    binary(lx, vars, bit_xor, |w| w == Rsv::BOr, |a, _, b| Ok(a | b))
}

/// Precedence 8 — `< > <= >=`.
fn greater(lx: &mut Lexer, vars: &mut Vars) -> TtlResult<Option<Eval>> {
    binary(
        lx,
        vars,
        bit_or,
        |w| matches!(w, Rsv::Lt | Rsv::Gt | Rsv::Le | Rsv::Ge),
        |a, w, b| {
            Ok(i32::from(match w {
                Rsv::Lt => a < b,
                Rsv::Gt => a > b,
                Rsv::Le => a <= b,
                _ => a >= b,
            }))
        },
    )
}

/// Precedence 9 — `= == <> !=`.
fn equal(lx: &mut Lexer, vars: &mut Vars) -> TtlResult<Option<Eval>> {
    binary(
        lx,
        vars,
        greater,
        |w| matches!(w, Rsv::Eq | Rsv::Ne),
        |a, w, b| Ok(i32::from(if w == Rsv::Eq { a == b } else { a != b })),
    )
}

/// Precedence 10 — `&&`. Both sides are evaluated; there is no short circuit.
fn logical_and(lx: &mut Lexer, vars: &mut Vars) -> TtlResult<Option<Eval>> {
    binary(
        lx,
        vars,
        equal,
        |w| w == Rsv::LAnd,
        |a, _, b| Ok(i32::from(a != 0 && b != 0)),
    )
}

/// Precedence 11 — `||`, and the entry point.
///
/// `RsvLXor` is accepted here and there is no way to write it: `GetOperator`
/// has no punctuation for it and `CheckReservedWord` no name. Kept because the
/// arm is upstream's, and because a name for it would be a language change.
pub fn get_expression(lx: &mut Lexer, vars: &mut Vars) -> TtlResult<Option<Eval>> {
    let start = lx.ptr;
    let r = binary(
        lx,
        vars,
        logical_and,
        |w| matches!(w, Rsv::LOr | Rsv::LXor),
        |a, w, b| {
            Ok(i32::from(if w == Rsv::LOr {
                a != 0 || b != 0
            } else {
                (a != 0) != (b != 0)
            }))
        },
    );
    match r {
        // The whole expression is put back on any error, which is what makes
        // the error dialog able to point at where it started.
        Err(e) => {
            lx.ptr = start;
            Err(e)
        }
        Ok(None) => {
            lx.ptr = start;
            Ok(None)
        }
        ok => ok,
    }
}

/// `GetIndex` — an optional `[expr]` after a name.
///
/// Absent is `Ok(None)` with nothing consumed; a malformed one is an error with
/// nothing consumed either, which is how `a[` is told from `a`.
pub fn index(lx: &mut Lexer, vars: &mut Vars) -> TtlResult<Option<i32>> {
    let p = lx.ptr;
    if lx.first_char() == b'[' {
        match get_int_val(lx, vars) {
            Ok(i) => {
                if lx.first_char() == b']' {
                    return Ok(Some(i));
                }
                lx.ptr = p;
                return Err(TtlError::CloseBracket);
            }
            Err(e) => {
                lx.ptr = p;
                return Err(e);
            }
        }
    }
    lx.ptr = p;
    Ok(None)
}

/// `GetIntVal` — an expression that has to be an integer.
pub fn get_int_val(lx: &mut Lexer, vars: &mut Vars) -> TtlResult<i32> {
    lx.mark();
    match get_expression(lx, vars)? {
        None => Err(TtlError::Syntax),
        Some(Eval::Int(v)) => Ok(v),
        Some(_) => Err(TtlError::TypeMismatch),
    }
}

/// `GetStrVal` — a string literal, or an expression naming a string.
pub fn get_str_val(lx: &mut Lexer, vars: &mut Vars) -> TtlResult<Vec<u8>> {
    lx.mark();
    get_str_val2(lx, vars, false)
}

/// `GetStrVal2` — the same, with the option of accepting an integer and
/// spelling it in decimal. Only the commands that document it pass `true`.
pub fn get_str_val2(lx: &mut Lexer, vars: &mut Vars, auto_conversion: bool) -> TtlResult<Vec<u8>> {
    if let Some(s) = lx.string()? {
        return Ok(s);
    }
    match get_expression(lx, vars)? {
        None => Err(TtlError::Syntax),
        Some(Eval::Str(r)) => {
            let mut s = vars.str_at(r).to_vec();
            // `strncpy_s(..., MaxStrLen, ..., _TRUNCATE)`: reading a string into
            // a command's scratch buffer costs everything past 511 bytes.
            s.truncate(MAX_STR_LEN - 1);
            Ok(s)
        }
        Some(Eval::Int(v)) => {
            if auto_conversion {
                Ok(v.to_string().into_bytes())
            } else {
                Err(TtlError::TypeMismatch)
            }
        }
        Some(_) => Err(TtlError::TypeMismatch),
    }
}

/// `GetIntVar` — a name to assign an integer to, created on the spot if new.
pub fn get_int_var(lx: &mut Lexer, vars: &mut Vars) -> TtlResult<VarRef> {
    let Some(name) = lx.identifier() else {
        return Err(TtlError::Syntax);
    };
    match vars.find(&name) {
        Some((id, VarType::Integer)) => Ok(VarRef::Scalar(id)),
        Some((id, VarType::IntArray)) => match index(lx, vars)? {
            Some(i) => vars.elem(id, i),
            None => Err(TtlError::TypeMismatch),
        },
        Some(_) => Err(TtlError::TypeMismatch),
        None => Ok(VarRef::Scalar(vars.new_int(&name, 0))),
    }
}

/// `GetStrVar` — the same for strings.
pub fn get_str_var(lx: &mut Lexer, vars: &mut Vars) -> TtlResult<VarRef> {
    let Some(name) = lx.identifier() else {
        return Err(TtlError::Syntax);
    };
    match vars.find(&name) {
        Some((id, VarType::String)) => Ok(VarRef::Scalar(id)),
        Some((id, VarType::StrArray)) => match index(lx, vars)? {
            Some(i) => vars.elem(id, i),
            None => Err(TtlError::TypeMismatch),
        },
        Some(_) => Err(TtlError::TypeMismatch),
        None => Ok(VarRef::Scalar(vars.new_str(&name, b""))),
    }
}

/// `GetAryVar` — a name that must already be an array of the wanted kind.
pub fn get_ary_var(lx: &mut Lexer, vars: &mut Vars, want: VarType) -> TtlResult<usize> {
    let Some(name) = lx.identifier() else {
        return Err(TtlError::Syntax);
    };
    match vars.find(&name) {
        Some((id, ty)) if ty == want => Ok(id),
        Some(_) => Err(TtlError::TypeMismatch),
        None => Err(TtlError::VarNotInit),
    }
}

/// `GetVarType` — what `ifdefined` reports, and it never fails.
///
/// A reserved word, an unknown name and an out-of-range element all answer
/// `Unknown`; a bad index is swallowed rather than reported, because upstream
/// clears the error on the way out.
pub fn get_var_type(lx: &mut Lexer, vars: &mut Vars) -> VarType {
    let Some(name) = lx.identifier() else {
        return VarType::Unknown;
    };
    if crate::lexer::check_reserved(&name).is_some() {
        return VarType::Unknown;
    }
    let Some((id, ty)) = vars.find(&name) else {
        return VarType::Unknown;
    };
    match ty {
        VarType::IntArray => match index(lx, vars) {
            Ok(Some(i)) if vars.elem(id, i).is_ok() => VarType::Integer,
            Ok(Some(_)) => VarType::Unknown,
            _ => ty,
        },
        VarType::StrArray => match index(lx, vars) {
            Ok(Some(i)) if vars.elem(id, i).is_ok() => VarType::String,
            Ok(Some(_)) => VarType::Unknown,
            _ => ty,
        },
        _ => ty,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval(src: &str) -> TtlResult<i32> {
        let mut vars = Vars::new();
        eval_with(src, &mut vars)
    }

    fn eval_with(src: &str, vars: &mut Vars) -> TtlResult<i32> {
        let mut lx = Lexer::new();
        lx.set_line(src.as_bytes());
        get_int_val(&mut lx, vars)
    }

    #[test]
    fn arithmetic_and_precedence() {
        assert_eq!(eval("1+2*3"), Ok(7));
        assert_eq!(eval("(1+2)*3"), Ok(9));
        assert_eq!(eval("7/2"), Ok(3));
        assert_eq!(eval("7%2"), Ok(1));
        assert_eq!(eval("-3"), Ok(-3));
        assert_eq!(eval("$10+1"), Ok(17));
    }

    #[test]
    fn dividing_by_zero_is_an_error_and_multiplying_by_it_is_not() {
        assert_eq!(eval("1/0"), Err(TtlError::DivByZero));
        assert_eq!(eval("1%0"), Err(TtlError::DivByZero));
        assert_eq!(eval("1*0"), Ok(0));
    }

    #[test]
    fn comparison_and_logic_yield_one_and_zero() {
        assert_eq!(eval("2>1"), Ok(1));
        assert_eq!(eval("2<1"), Ok(0));
        assert_eq!(eval("1=1"), Ok(1));
        assert_eq!(eval("1==1"), Ok(1));
        assert_eq!(eval("1<>2"), Ok(1));
        assert_eq!(eval("1!=2"), Ok(1));
        assert_eq!(eval("1 && 0"), Ok(0));
        assert_eq!(eval("1 || 0"), Ok(1));
        assert_eq!(eval("!0"), Ok(1));
        assert_eq!(eval("!5"), Ok(0));
    }

    #[test]
    fn bitwise_words_and_punctuation_mean_the_same_thing() {
        assert_eq!(eval("6 and 3"), Ok(2));
        assert_eq!(eval("6 & 3"), Ok(2));
        assert_eq!(eval("6 or 3"), Ok(7));
        assert_eq!(eval("6 xor 3"), Ok(5));
        assert_eq!(eval("not 0"), Ok(-1));
        assert_eq!(eval("~0"), Ok(-1));
    }

    #[test]
    fn shifts_saturate_rather_than_wrap_the_count() {
        assert_eq!(eval("1 << 4"), Ok(16));
        assert_eq!(eval("16 >> 4"), Ok(1));
        assert_eq!(eval("1 << 32"), Ok(0));
        assert_eq!(eval("1 << 100"), Ok(0));
        assert_eq!(eval("0-1 >> 100"), Ok(-1));
        assert_eq!(eval("0-1 >>> 100"), Ok(0));
        // A negative count reverses the direction, which is how `<<` is built.
        assert_eq!(eval("16 << 0-4"), Ok(1));
        assert_eq!(eval("1 >> 0-4"), Ok(16));
    }

    #[test]
    fn a_logical_right_shift_does_not_sign_extend() {
        assert_eq!(eval("0-1 >> 1"), Ok(-1));
        assert_eq!(eval("0-1 >>> 1"), Ok(0x7fff_ffff));
    }

    #[test]
    fn an_unset_name_in_an_expression_is_not_a_zero() {
        assert_eq!(eval("nosuchvar"), Err(TtlError::VarNotInit));
    }

    #[test]
    fn a_command_name_in_an_expression_is_a_syntax_error() {
        assert_eq!(eval("1 + sendln"), Err(TtlError::Syntax));
    }

    #[test]
    fn a_string_ends_the_expression_where_it_stands() {
        let mut vars = Vars::new();
        vars.new_str(b"s", b"hello");
        let mut lx = Lexer::new();
        lx.set_line(b"s + 1");
        let v = get_expression(&mut lx, &mut vars).unwrap().unwrap();
        assert_eq!(v.var_type(), VarType::String);
        // `+ 1` is still sitting there, which is what makes the caller fail.
        assert_eq!(lx.first_char(), b'+');
    }

    #[test]
    fn an_integer_reaches_a_string_slot_only_when_the_command_allows_it() {
        let mut vars = Vars::new();
        let mut lx = Lexer::new();
        lx.set_line(b"42");
        assert_eq!(
            get_str_val2(&mut lx, &mut vars, false),
            Err(TtlError::TypeMismatch)
        );
        lx.set_line(b"42");
        assert_eq!(
            get_str_val2(&mut lx, &mut vars, true).as_deref(),
            Ok(&b"42"[..])
        );
    }

    #[test]
    fn an_array_element_reads_as_its_element_type() {
        let mut vars = Vars::new();
        let id = vars.new_int_array(b"a", 3);
        vars.set_int(VarRef::Elem(id, 1), 7);
        assert_eq!(eval_with("a[1]", &mut vars), Ok(7));
        assert_eq!(eval_with("a[1]*2", &mut vars), Ok(14));
        assert_eq!(eval_with("a[3]", &mut vars), Err(TtlError::OutOfRange));
        // Named without an index it is an array, and an array is not a number.
        assert_eq!(eval_with("a", &mut vars), Err(TtlError::TypeMismatch));
    }

    #[test]
    fn an_index_is_itself_an_expression() {
        let mut vars = Vars::new();
        let id = vars.new_int_array(b"a", 4);
        vars.set_int(VarRef::Elem(id, 3), 9);
        vars.new_int(b"i", 1);
        assert_eq!(eval_with("a[i+2]", &mut vars), Ok(9));
    }

    #[test]
    fn a_missing_bracket_names_itself() {
        let mut vars = Vars::new();
        vars.new_int_array(b"a", 4);
        assert_eq!(eval_with("a[1", &mut vars), Err(TtlError::CloseBracket));
    }

    #[test]
    fn a_missing_parenthesis_names_itself() {
        assert_eq!(eval("(1+2"), Err(TtlError::CloseParent));
    }

    #[test]
    fn ifdefined_answers_for_every_shape_of_name() {
        let mut vars = Vars::new();
        vars.new_int(b"i", 0);
        vars.new_str(b"s", b"");
        vars.new_int_array(b"ia", 2);
        vars.new_str_array(b"sa", 2);
        let mut check = |src: &str| {
            let mut lx = Lexer::new();
            lx.set_line(src.as_bytes());
            get_var_type(&mut lx, &mut vars)
        };
        assert_eq!(check("i"), VarType::Integer);
        assert_eq!(check("s"), VarType::String);
        assert_eq!(check("ia"), VarType::IntArray);
        assert_eq!(check("sa"), VarType::StrArray);
        assert_eq!(check("ia[0]"), VarType::Integer);
        assert_eq!(check("sa[0]"), VarType::String);
        assert_eq!(check("ia[9]"), VarType::Unknown);
        assert_eq!(check("nope"), VarType::Unknown);
        assert_eq!(check("sendln"), VarType::Unknown);
        assert_eq!(check("1"), VarType::Unknown);
    }

    #[test]
    fn a_destination_name_is_created_with_the_type_the_slot_wants() {
        let mut vars = Vars::new();
        let mut lx = Lexer::new();
        lx.set_line(b"fresh");
        let r = get_int_var(&mut lx, &mut vars).unwrap();
        assert_eq!(vars.int_at(r), 0);
        assert_eq!(vars.find(b"fresh").map(|(_, t)| t), Some(VarType::Integer));
        // ...and once it exists, the other kind of slot refuses it.
        lx.set_line(b"fresh");
        assert_eq!(get_str_var(&mut lx, &mut vars), Err(TtlError::TypeMismatch));
    }

    #[test]
    fn an_error_puts_the_whole_expression_back() {
        let mut vars = Vars::new();
        let mut lx = Lexer::new();
        lx.set_line(b"  1/0");
        assert_eq!(get_int_val(&mut lx, &mut vars), Err(TtlError::DivByZero));
        assert_eq!(lx.ptr, 0);
    }
}

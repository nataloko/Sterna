//! The error codes, which are upstream's and are user-visible.
//!
//! `ttmparse.h:52-72` numbers twenty-one of them and `DispErr`
//! (`ttmparse.cpp:124`) turns each into the sentence the error dialog shows.
//! Both are reproduced rather than improved on: a macro that has been failing
//! the same way for twenty years should keep saying so in the same words, and
//! the number is what `ttmparse.h` calls the error in every report about it.

use std::fmt;

/// One of upstream's `Err*` codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum TtlError {
    CloseParent = 1,
    CantCall = 2,
    CantConnect = 3,
    CantOpen = 4,
    DivByZero = 5,
    InvalidCtl = 6,
    LabelAlreadyDef = 7,
    LabelReq = 8,
    LinkFirst = 9,
    StackOver = 10,
    Syntax = 11,
    TooManyLabels = 12,
    TooManyVar = 13,
    TypeMismatch = 14,
    VarNotInit = 15,
    CloseComment = 16,
    OutOfRange = 17,
    CloseBracket = 18,
    FewMemory = 19,
    NotSupported = 20,
    CantExec = 21,
}

impl TtlError {
    /// Upstream's number for this error, as `ttmparse.h` names it.
    pub fn code(self) -> u16 {
        self as u16
    }

    /// The sentence `DispErr` puts in the dialog, verbatim — spelling included.
    pub fn message(self) -> &'static str {
        match self {
            TtlError::CloseParent => "\")\" expected.",
            TtlError::CantCall => "Can't call sub.",
            TtlError::CantConnect => "Can't link macro.",
            TtlError::CantOpen => "Can't open file.",
            TtlError::DivByZero => "Divide by zero.",
            TtlError::InvalidCtl => "Invalid control.",
            TtlError::LabelAlreadyDef => "Label already defined.",
            // Upstream's typo. Kept: it is what users have been reading.
            TtlError::LabelReq => "Label requiered.",
            TtlError::LinkFirst => "Link macro first. Use 'connect' macro.",
            TtlError::StackOver => "Stack overflow.",
            TtlError::Syntax => "Syntax error.",
            TtlError::TooManyLabels => "Too many labels.",
            TtlError::TooManyVar => "Too many variables.",
            TtlError::TypeMismatch => "Type mismatch.",
            TtlError::VarNotInit => "Variable not initialized.",
            TtlError::CloseComment => "\"*/\" expected.",
            TtlError::OutOfRange => "Index out of range.",
            TtlError::CloseBracket => "\"]\" expected.",
            TtlError::FewMemory => "Can't allocate memory.",
            TtlError::NotSupported => "Unknown command.",
            TtlError::CantExec => "Can't execute command.",
        }
    }
}

impl fmt::Display for TtlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message())
    }
}

impl std::error::Error for TtlError {}

/// What every step of the interpreter returns.
pub type TtlResult<T> = Result<T, TtlError>;

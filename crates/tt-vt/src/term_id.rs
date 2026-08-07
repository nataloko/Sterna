//! Terminal identity and the Primary DA response it produces.
//!
//! Names and payloads come from `common/tttypes_termid.cpp` and
//! `vtterm.c:AnswerTerminalType`. The DA string is what a host uses to decide
//! which sequences to send, so a wrong entry here is not cosmetic.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TermId {
    #[default]
    Vt100,
    Vt100J,
    Vt101,
    Vt102,
    Vt102J,
    Vt220,
    Vt220J,
    Vt282,
    Vt320,
    Vt382,
    Vt420,
    Vt520,
    Vt525,
    Dumb,
}

impl TermId {
    pub fn parse(name: &str) -> Option<TermId> {
        Some(match name.to_ascii_lowercase().as_str() {
            "vt100" => TermId::Vt100,
            "vt100j" => TermId::Vt100J,
            "vt101" => TermId::Vt101,
            "vt102" => TermId::Vt102,
            "vt102j" => TermId::Vt102J,
            "vt220" => TermId::Vt220,
            "vt220j" => TermId::Vt220J,
            "vt282" => TermId::Vt282,
            "vt320" => TermId::Vt320,
            "vt382" => TermId::Vt382,
            "vt420" => TermId::Vt420,
            "vt520" => TermId::Vt520,
            "vt525" => TermId::Vt525,
            "dumb" => TermId::Dumb,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            TermId::Vt100 => "vt100",
            TermId::Vt100J => "vt100j",
            TermId::Vt101 => "vt101",
            TermId::Vt102 => "vt102",
            TermId::Vt102J => "vt102j",
            TermId::Vt220 => "vt220",
            TermId::Vt220J => "vt220j",
            TermId::Vt282 => "vt282",
            TermId::Vt320 => "vt320",
            TermId::Vt382 => "vt382",
            TermId::Vt420 => "vt420",
            TermId::Vt520 => "vt520",
            TermId::Vt525 => "vt525",
            TermId::Dumb => "dumb",
        }
    }

    /// The body of the Primary DA reply, between `ESC [ ?` and `c`.
    ///
    /// `dumb` has no case in the upstream switch, so it answers with an empty
    /// body — `ESC [ ? c`. That is upstream's behaviour, not an oversight here.
    pub fn primary_da(self) -> &'static str {
        match self {
            TermId::Vt100 => "1;2",
            TermId::Vt100J => "5;2",
            TermId::Vt101 => "1;0",
            TermId::Vt102 => "6",
            TermId::Vt102J => "15",
            TermId::Vt220 => "62;1;2;6;7;8;9",
            TermId::Vt220J => "62;1;2;5;6;7;8",
            TermId::Vt282 => "62;1;2;4;5;6;7;8;10;11",
            TermId::Vt320 => "63;1;2;6;7;8;9",
            TermId::Vt382 => "63;1;2;4;5;6;7;8;10;15",
            TermId::Vt420 => "64;1;2;6;7;8;9;15;18;19;21",
            TermId::Vt520 => "65;1;2;7;9;12;18;19;21;23;24;42;44;45;46",
            TermId::Vt525 => "65;1;2;7;9;12;18;19;21;22;23;24;42;44;45;46",
            TermId::Dumb => "",
        }
    }
}

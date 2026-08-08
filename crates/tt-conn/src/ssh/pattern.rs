//! OpenSSH's host patterns, which `known_hosts` and `ssh_config` share.
//!
//! `*` and `?` only — no character classes, no `**`. That is what
//! OpenSSH's `match_pattern` supports and what Tera Term's copy of it
//! (`ttssh2/matcher/matcher.c`) supports, and adding more would mean patterns
//! that work here and nowhere else.
//!
//! Case-insensitive, because host names are. Tera Term's matcher is not, which
//! is why a host reached by a differently-cased name is a host it has never
//! seen.

/// True when `pattern` matches `s`.
pub(crate) fn matches_glob(pattern: &str, s: &str) -> bool {
    let p: Vec<char> = pattern.chars().flat_map(|c| c.to_lowercase()).collect();
    let h: Vec<char> = s.chars().flat_map(|c| c.to_lowercase()).collect();
    glob(&p, &h)
}

fn glob(p: &[char], h: &[char]) -> bool {
    match p.first() {
        None => h.is_empty(),
        Some('*') => {
            // Greedy matching would need backtracking anyway; try every split.
            (0..=h.len()).any(|i| glob(&p[1..], &h[i..]))
        }
        Some('?') => !h.is_empty() && glob(&p[1..], &h[1..]),
        Some(c) => h.first() == Some(c) && glob(&p[1..], &h[1..]),
    }
}

/// A list of patterns where any `!pattern` that matches vetoes the whole list,
/// and at least one positive pattern has to match.
///
/// This is the rule in both files. Its consequence is worth stating: a list of
/// *only* negative patterns never matches anything, because there is nothing
/// positive to satisfy. OpenSSH behaves the same way, and a config written
/// expecting `!bastion` to mean "everything except bastion" quietly applies to
/// nothing at all.
pub(crate) fn matches_list<'a>(patterns: impl Iterator<Item = &'a str>, s: &str) -> bool {
    let mut matched = false;
    for entry in patterns {
        let (negated, pattern) = match entry.strip_prefix('!') {
            Some(p) => (true, p),
            None => (false, entry),
        };
        if matches_glob(pattern, s) {
            if negated {
                return false;
            }
            matched = true;
        }
    }
    matched
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_star_spans_dots() {
        // Which is why `!` exists: `*.example.com` cannot be narrowed by
        // writing a more specific pattern, only by excluding one.
        assert!(matches_glob("*.example.com", "a.b.example.com"));
        assert!(matches_glob("*", "anything"));
    }

    #[test]
    fn a_question_mark_is_exactly_one_character() {
        assert!(matches_glob("h?st", "host"));
        assert!(!matches_glob("h?st", "hst"));
        assert!(!matches_glob("h?st", "hoost"));
    }

    #[test]
    fn a_negative_vetoes_a_positive_wherever_it_appears() {
        let list = |s: &str, h: &str| matches_list(s.split_whitespace(), h);
        assert!(list(
            "*.example.com !secret.example.com",
            "host.example.com"
        ));
        assert!(!list(
            "*.example.com !secret.example.com",
            "secret.example.com"
        ));
        // Order does not matter — OpenSSH scans the whole list.
        assert!(!list(
            "!secret.example.com *.example.com",
            "secret.example.com"
        ));
    }

    #[test]
    fn negatives_alone_match_nothing() {
        assert!(!matches_list(["!bastion"].into_iter(), "anything"));
    }
}

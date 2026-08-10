//! Tera Term `.lng` catalogs.
//!
//! A language file is an INI file, so it goes through [`tt_config::Ini`]
//! rather than through a second parser with slightly different duplicate,
//! quoting or empty-value rules. Upstream does the same thing: `GetI18nStrWW`
//! calls `hGetPrivateProfileStringW`, then only restores the four backslash
//! escapes used by labels, messages and file-dialog filters.

use std::io;
use std::path::Path;

use tt_config::Ini;

/// One loaded `.lng` catalog.
pub struct Catalog {
    ini: Ini,
}

impl Catalog {
    /// Parse a catalog from the bytes on disk.
    ///
    /// The compatible INI reader accepts the UTF-8 BOM used by the vendored
    /// catalogs as well as UTF-16LE and legacy byte-preserving files, so a
    /// user's existing language file does not need converting first.
    pub fn parse(bytes: &[u8]) -> Catalog {
        Catalog {
            ini: Ini::parse(bytes),
        }
    }

    /// Read a catalog. Unlike settings, a missing language file is an error:
    /// the caller already has its built-in English fallback and needs to know
    /// that the selected translation was not installed.
    pub fn load(path: &Path) -> io::Result<Catalog> {
        std::fs::read(path).map(|bytes| Catalog::parse(&bytes))
    }

    /// Look up and unescape one value.
    ///
    /// The returned string may contain NULs. Upstream uses them in common-file
    /// dialog filters (`name\0pattern\0\0`), and flattening them to a C string
    /// here would make the loader unsuitable for those callers later.
    pub fn get(&self, section: &str, key: &str) -> Option<String> {
        self.ini.get(section, key).map(restore_escapes)
    }

    /// Look up one value, returning the caller's source-language text when the
    /// catalog has no translation for it. `Default.lng` intentionally contains
    /// only font choices, so this fallback is the normal English path rather
    /// than an error case.
    pub fn get_or(&self, section: &str, key: &str, fallback: &str) -> String {
        self.get(section, key)
            .unwrap_or_else(|| fallback.to_owned())
    }

    /// The translator-facing language name from `[Info]`, when present.
    pub fn language(&self) -> Option<String> {
        self.get("Info", "language")
    }
}

/// `RestoreNewLineW` from `common/ttlib_static_cpp.cpp`: exactly four escapes
/// are special, and an unknown escape keeps its backslash for the next byte.
fn restore_escapes(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('\\') => out.push('\\'),
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('0') => out.push('\0'),
            Some(next) => {
                out.push('\\');
                out.push(next);
            }
            None => out.push('\\'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_win32_ini_rules_before_unescaping() {
        let catalog = Catalog::parse(
            b"[Tera Term]\r\nKey='one\\ntwo'\r\nKey=ignored\r\n\\Key=a\\qb\\\\c\\td\\0e\r\n",
        );
        assert_eq!(catalog.get("tera term", "key").as_deref(), Some("one\ntwo"));
        assert_eq!(
            catalog.get("Tera Term", "\\Key").as_deref(),
            Some("a\\qb\\c\td\0e")
        );
        assert_eq!(
            catalog.get_or("Tera Term", "missing", "fallback"),
            "fallback"
        );
    }

    #[test]
    fn all_vendored_catalogs_load_verbatim() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/lang");
        let names = [
            "Default.lng",
            "de_DE.lng",
            "en_US.lng",
            "es_ES.lng",
            "fr_FR.lng",
            "it_IT.lng",
            "ja_JP.lng",
            "ko_KR.lng",
            "pt_BR.lng",
            "ru_RU.lng",
            "ta_IN.lng",
            "tr_TR.lng",
            "zh_CN.lng",
            "zh_TW.lng",
        ];
        for name in names {
            let catalog = Catalog::load(&root.join(name)).unwrap();
            assert!(catalog.language().is_some(), "{name} has no language name");
        }

        let japanese = Catalog::load(&root.join("ja_JP.lng")).unwrap();
        assert_eq!(japanese.language().as_deref(), Some("Japanese(日本語)"));
        assert_eq!(
            japanese.get("Tera Term", "MENU_FILE").as_deref(),
            Some("ファイル(&F)")
        );

        let english = Catalog::load(&root.join("en_US.lng")).unwrap();
        assert_eq!(
            english
                .get("Tera Term", "MSG_LOGFILE_WRITE_ERROR")
                .as_deref(),
            Some("Cannot write log file.\n%s")
        );
        assert_eq!(
            english
                .get("Tera Term", "FILEDLG_OPEN_LOGFILE_FILTER")
                .as_deref(),
            Some("all(*.*)\0*.*\0\0")
        );
    }

    #[test]
    fn a_missing_catalog_is_an_error() {
        let error = Catalog::load(Path::new("this-catalog-does-not-exist.lng"))
            .err()
            .expect("missing catalog should fail");
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
    }
}

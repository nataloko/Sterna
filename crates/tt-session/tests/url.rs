//! URL extraction from the AttrURL runs produced by the VT write path.

use tt_config::Settings;
use tt_session::Session;
use tt_vt::Config;

#[test]
fn url_at_returns_the_marked_run_and_nothing_around_it() {
    let mut session = Session::new(Config {
        cols: 30,
        rows: 2,
        ..Config::default()
    });
    session.feed(b"x http://example.test end");

    assert_eq!(session.url_at(0, 5).as_deref(), Some("http://example.test"));
    assert_eq!(session.url_at(0, 0), None);
    assert_eq!(session.url_at(0, 22), None);
    assert_eq!(session.url_at(99, 0), None);
}

#[test]
fn split_url_uses_upstreams_continued_copy_setting() {
    let mut settings = Settings {
        terminal_cols: 10,
        terminal_rows: 3,
        ..Settings::default()
    };
    let mut session = Session::from_settings(settings.clone());
    session.feed(b"xxxhttp://x");

    assert_eq!(session.url_at(0, 5).as_deref(), Some("http://\r\r\nx"));
    settings.clipboard_continued_line_copy = true;
    session.set_settings(settings);
    assert_eq!(session.url_at(0, 5).as_deref(), Some("http://x"));
    assert_eq!(session.url_at(1, 0).as_deref(), Some("http://x"));
}

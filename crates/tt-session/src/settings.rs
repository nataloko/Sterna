//! `TERATERM.INI` on one side, a running terminal on the other.
//!
//! `tt-config` deliberately does not depend on `tt-vt`: the schema is a
//! description of a *file*, and one that knew about the parser would be two
//! things at once. So the map between them lives here, in the one crate that
//! already holds both — and it is a map, not a copy. Three groups come out of
//! [`Settings`] and only the first reaches the core:
//!
//! 1. **The terminal's**, which is everything below: sizes, the terminal ID,
//!    the CR modes, the colour flags, the window and mouse gates.
//! 2. **The window's** — the five colour pairs, the cursor shape as something
//!    to *draw*, the word delimiters a double-click uses, the title. The
//!    frontend reads those by name through the metadata table rather than
//!    through a second struct that would have to be kept in step.
//! 3. **Nobody's yet** — settings that are read and written faithfully and act
//!    on nothing, because the subsystem they belong to is a later stage.
//!
//! The third group is the reason this file names every field it uses rather
//! than deriving the mapping: a setting that silently does nothing should be
//! visible as an absence here.

use crate::bell::BellLimits;
use crate::log::{LogMode, LogOptions, Timestamp};
use std::time::Duration;
use tt_config::{
    hex_decode, BellMode, ClipboardRemoteAccess, CursorShape, DebugModes as FileDebugModes,
    KeyboardBackspace, LogTimestampType, Settings, TerminalCrReceive, TerminalCrSend,
    WindowTitleChange, WindowTitleReport,
};
use tt_vt::{
    palette::Rgb, valid_terminal_uid, Beep, ClipboardAccess, ColorFlags, Config, CrReceive, CrSend,
    DebugModes, ShiftFlags, TabStopFlags, TermId, TitleChange, TitleReport, DEFAULT_TERMINAL_UID,
};

/// `vtdisp.c:GetIndex256From16`: `ts.ANSIColor` keeps the legacy table order,
/// while the renderer's 256-colour table swaps its bright and dim halves.
const ANSI_256_FROM_16: [usize; 16] = [0, 9, 10, 11, 12, 13, 14, 15, 8, 1, 2, 3, 4, 5, 6, 7];

/// `CRSend` → the engine's, named because `TCPCRSend` restores this one.
pub(crate) fn cr_send_of(s: TerminalCrSend) -> CrSend {
    match s {
        TerminalCrSend::Cr => CrSend::Cr,
        TerminalCrSend::CrLf => CrSend::CrLf,
        TerminalCrSend::Lf => CrSend::Lf,
    }
}

/// Build the terminal's configuration from the settings.
///
/// `base` supplies everything the schema has no key for — the cell size the
/// frontend measured, `decrqcra` for the conformance harness, and `japanese`,
/// which is a Stage 4 question — so that applying settings to a running
/// terminal does not quietly reset what nobody asked about. Pass
/// `&Config::default()` at startup and `vt.config()` afterwards.
///
/// That list is the whole of it: every other field below is named, and a field
/// that stops being named is a setting the file can no longer reach.
pub fn vt_config(s: &Settings, base: &Config) -> Config {
    let file_debug_modes = FileDebugModes::parse_ini(&s.debug_modes);
    Config {
        cols: s.terminal_cols.max(1) as usize,
        rows: rows(s),
        scrollback_max: scrollback_max(s),
        term_id: term_id(s),
        cr_receive: match s.terminal_cr_receive {
            TerminalCrReceive::Cr => CrReceive::Cr,
            TerminalCrReceive::CrLf => CrReceive::CrLf,
            TerminalCrReceive::Lf => CrReceive::Lf,
            TerminalCrReceive::Auto => CrReceive::Auto,
        },
        debug_enabled: s.debug_enabled && !file_debug_modes.is_empty(),
        debug_modes: DebugModes::from_bits(file_debug_modes.bits()),
        cr_send: cr_send_of(s.terminal_cr_send),
        local_echo: s.terminal_local_echo,
        bs_key_is_bs: s.keyboard_backspace == KeyboardBackspace::Bs,
        disable_app_keypad: s.keyboard_disable_app_keypad,
        disable_app_cursor: s.keyboard_disable_app_cursor,
        color_flags: ColorFlags {
            xterm256: s.color_xterm_256,
            aixterm16: s.color_aixterm_16,
            pc_bold16: s.color_pc_bold_16,
            ansi_color: s.color_ansi_enabled,
        },
        palette: ansi_palette(&s.color_ansi_palette, base.palette),
        color_normal: color_pair(s.color_normal),
        color_bold: color_pair(s.color_bold),
        color_blink: color_pair(s.color_blink),
        color_reverse: color_pair(s.color_reverse),
        color_url: color_pair(s.color_url),
        color_underline: color_pair(s.color_underline),
        color_tek: color_pair(s.tek_color),
        // DECSCUSR's numbering, which is what `Config::cursor_shape` holds and
        // what DECRQSS answers with: `vtterm.c:4270` maps IdBlkCur to 1,
        // IdHCur to 3 and IdVCur to **5**, so the enum's own order is not it.
        cursor_shape: match s.cursor_shape {
            CursorShape::Block => 1,
            CursorShape::Horizontal => 3,
            CursorShape::Vertical => 5,
        },
        nonblinking_cursor: s.cursor_nonblinking,
        window_change: s.window_change_allowed,
        window_report: s.window_report_allowed,
        title: s.terminal_title.clone(),
        iso2022_flags: ShiftFlags::parse_ini(&s.terminal_iso2022_shifts),
        title_report: match s.window_title_report {
            WindowTitleReport::Ignore => TitleReport::Ignore,
            WindowTitleReport::Accept => TitleReport::Accept,
            WindowTitleReport::Empty => TitleReport::Empty,
        },
        accept_title_change: match s.window_title_change {
            WindowTitleChange::Off => TitleChange::Off,
            WindowTitleChange::Overwrite => TitleChange::Overwrite,
            WindowTitleChange::Ahead => TitleChange::Ahead,
            WindowTitleChange::Last => TitleChange::Last,
        },
        printer_ctrl_sequence: s.printer_control_sequences,
        // Only whether a device was named ever reaches the engine — the name
        // itself is the frontend's, because everything it addresses is.
        printer_direct: !s.printer_passthrough_port.is_empty(),
        cursor_ctrl_sequence: s.window_cursor_ctrl_allowed,
        accept_8bit_ctrl: s.window_accept_8bit_ctrl,
        send_8bit_ctrl: s.window_send_8bit_ctrl,
        alt_screen_enabled: s.window_alt_screen,
        remote_clears_buffer: s.window_remote_clears_buffer,
        clear_on_resize: s.terminal_clear_on_resize,
        home_erase_clears_screen: s.terminal_home_erase_clears_screen,
        mouse_tracking_enabled: s.mouse_tracking,
        disable_mouse_tracking_by_ctrl: s.mouse_ctrl_disables_tracking,
        translate_wheel_to_cursor: s.mouse_wheel_to_cursor,
        disable_wheel_to_cursor_by_ctrl: s.mouse_ctrl_disables_wheel_to_cursor,
        // A log setting in the terminal's configuration, because that is where
        // upstream reads it: the tap it gates is `vtterm.c`'s, and it feeds a
        // macro's received-line buffer as well as the log.
        log_plain_text: s.log_plain_text,
        // And a clipboard setting in it, for the same reason: the half of it
        // upstream spends in `vtterm.c` is the `logFlag` on `CarriageReturn`
        // and `LineFeed`, which decides whether a wrapped line reaches the log
        // and a macro as one line or as two.
        continued_line_copy: s.clipboard_continued_line_copy,
        // The terminal's whole share of the bell family: whether BEL asks for
        // one at all. The governor in front of it is four more settings and a
        // clock, and it lives in [`crate::Session`] for want of the clock.
        beep: match s.bell_mode {
            BellMode::Off => Beep::Off,
            BellMode::On => Beep::On,
            BellMode::Visual => Beep::Visual,
        },
        // Decoded here rather than held decoded, because `$xx` is the *file's*
        // spelling of the bytes and the round-trip has to give the user back
        // what they wrote. 32 is the C buffer this filled (`tttypes.h:350`),
        // and it truncates rather than refusing, as it does there.
        answerback: hex_decode(&s.terminal_answerback, 32),
        back_wrap: s.terminal_back_wrap,
        vt_compat_tab: s.terminal_vt_compat_tab,
        tab_stop_modify: TabStopFlags::parse_ini(&s.terminal_tab_stop_modify),
        invalid_decrqss: s.terminal_invalid_decrqss,
        // Validated here rather than held validated, the way the answerback's
        // `$xx` is decoded here rather than stored decoded: the file keeps
        // whatever the user wrote and the terminal answers with the form
        // `ttset.c:1691` would have produced. An invalid value takes the key's
        // own default, which is what upstream's fallback is.
        terminal_uid: valid_terminal_uid(&s.terminal_uid)
            .unwrap_or_else(|| DEFAULT_TERMINAL_UID.to_string()),
        lock_uid: s.terminal_lock_uid,
        auto_invoke: s.terminal_auto_invoke,
        // `GetPrivateProfileInt` cannot return a negative (`ini-audit/`), so
        // the floor is only for a caller that set the field by hand.
        max_osc_buffer: s.terminal_max_osc_buffer.max(0) as usize,
        clipboard_access: match s.clipboard_remote_access {
            ClipboardRemoteAccess::Off => ClipboardAccess::Off,
            ClipboardRemoteAccess::Read => ClipboardAccess::Read,
            ClipboardRemoteAccess::Write => ClipboardAccess::Write,
            ClipboardRemoteAccess::ReadWrite => ClipboardAccess::ReadWrite,
        },
        notify_clipboard_access: s.clipboard_remote_notify,
        ..*base
    }
}

/// One of the schema's `color2` values — six bytes, foreground then background
/// — as the pair the terminal holds. These are the *configured* colours a
/// `OSC 110`-style reset returns to; the live ones are `tt_vt::Colors`.
fn color_pair(value: [u8; 6]) -> [Rgb; 2] {
    [
        (value[0], value[1], value[2]),
        (value[3], value[4], value[5]),
    ]
}

/// Parse `ANSIColor` as `ttset.c:797` does, including its narrow buffers and
/// integer casts.
///
/// The whole value lands in `char Temp[MAX_PATH]`, each field then lands in
/// `char T[15]`, and only complete groups of four are read. IDs are masked to
/// four bits, channels narrow to `BYTE`, and a duplicate ID overwrites the
/// earlier group. Starting from the live palette matters for a partial value:
/// upstream assigns only the entries it was actually handed.
fn ansi_palette(value: &str, mut palette: [Rgb; 256]) -> [Rgb; 256] {
    let bytes = value.as_bytes();
    let bytes = &bytes[..bytes.len().min(259)]; // `MAX_PATH` minus the NUL.
    let mut fields = bytes.split(|&byte| byte == b',');

    while let Some(id) = fields.next() {
        let Some(red) = fields.next() else { break };
        let Some(green) = fields.next() else { break };
        let Some(blue) = fields.next() else { break };

        let number = |field: &[u8]| {
            let field = &field[..field.len().min(14)]; // `T[15]` minus the NUL.
            tt_config::services::scanf_int(field).unwrap_or(0)
        };
        let legacy = (number(id) & 15) as usize;
        palette[ANSI_256_FROM_16[legacy]] =
            (number(red) as u8, number(green) as u8, number(blue) as u8);
    }

    palette
}

/// The three numbers behind `RingBell`'s governor (`vtterm.c:5791`).
///
/// Read at every bell rather than held, which is what upstream does — the
/// governor's state is three file statics and the limits come straight out of
/// `ts` inside the function, so a setting changed in the dialog applies to the
/// next BEL and not to the next connection.
///
/// Negative is not reachable through the dialog and is reachable by hand:
/// `GetPrivateProfileInt` refuses a leading `-` and answers with the default
/// (`ini-audit/win32.txt`), so a `BeepOverUsedTime=-1` never gets this far. The
/// saturating casts are for a caller that set the field directly.
pub fn bell_limits(s: &Settings) -> BellLimits {
    BellLimits {
        count: s.bell_over_used_count.max(0) as u32,
        over_used: Duration::from_secs(s.bell_over_used_time.max(0) as u64),
        suppress: Duration::from_secs(s.bell_suppress_time.max(0) as u64),
    }
}

/// Build the log's options from the settings — `LogStart`'s reading of `ts`
/// (`filesys_log.cpp:387`), plus the timestamp chain `ttset.c` leaves half in
/// one key and half in another.
///
/// Not applied to an open log: upstream reads `ts` when the log *opens*, so a
/// setting changed mid-capture takes effect on the next one. The two
/// exceptions are `logrotate`'s three arms, which reconfigure a running log on
/// purpose and go through [`SessionLog`](crate::SessionLog) directly.
pub fn log_options(s: &Settings) -> LogOptions {
    // `FixLogOption` (`filesys_log.cpp:243`) clears the timestamp for a binary
    // log, and `SessionLog::open` does the same thing on the way in — so this
    // does not repeat it, and `LogOptions` cannot claim a timestamp that will
    // never be written.
    LogOptions {
        mode: if s.log_binary {
            LogMode::Raw
        } else {
            LogMode::Text
        },
        timestamp: timestamp(s),
        append: s.log_append,
        rotate_size: rotate_size(s),
        rotate_keep: rotate_keep(s),
        format: s.log_timestamp_format.clone(),
        // Upstream's text log always writes CR LF (`vtterm.c:361` sets
        // `log_cr_type` to 0) and has no key for it, so this is the one field
        // here the schema does not decide. See `LogOptions::crlf` for why the
        // default is the other way round.
        crlf: LogOptions::default().crlf,
    }
}

/// `ttset.c:1000`'s chain, which is two keys and an `else`.
fn timestamp(s: &Settings) -> Timestamp {
    if !s.log_timestamp {
        return Timestamp::None;
    }
    match s.log_timestamp_type {
        // No `LogTimestampType` at all: Tera Term 4's `LogTimestampUTC`
        // answers instead (`ttset.c:1007`), which is the whole reason the
        // schema keeps an empty spelling apart from `Local`.
        LogTimestampType::Unset if s.log_timestamp_utc => Timestamp::Utc,
        LogTimestampType::Unset | LogTimestampType::Local => Timestamp::Local,
        LogTimestampType::Utc => Timestamp::Utc,
        LogTimestampType::LoggingElapsed => Timestamp::Elapsed,
        LogTimestampType::ConnectionElapsed => Timestamp::ElapsedConnection,
    }
}

/// `LogRotate` is the switch and `LogRotateSize` is the threshold, and neither
/// is bounded on the way in.
///
/// **The size is already in bytes.** `LogRotateSizeType` is the unit the
/// dialog *shows*, and `log_pp.cpp:471` multiplies by 1024 per unit before
/// storing — so scaling it again here would turn the 1 MB somebody asked for
/// into a terabyte, and their log would never rotate.
fn rotate_size(s: &Settings) -> u64 {
    // `filesys_log.cpp:513`: `ROTATE_SIZE` is 1, `ROTATE_NONE` is 0, and
    // anything else falls out of the `if` and rotates nothing.
    if s.log_rotate != 1 {
        return 0;
    }
    s.log_rotate_size.max(0) as u64
}

/// How many generations, where upstream's zero is not "none".
///
/// `filesys_log.cpp:507` leaves `loopmax` at a hardcoded 10000 when
/// `LogRotateStep` is unset, so rotation with no step keeps ten thousand
/// files. Reproduced rather than tidied: a file that says nothing must behave
/// the way the user's own Tera Term behaves, and `LogOptions::rotate_keep`
/// gets the number rather than the meaning of the zero.
fn rotate_keep(s: &Settings) -> u32 {
    if s.log_rotate != 1 {
        return 0;
    }
    match s.log_rotate_step {
        n if n > 0 => n as u32,
        _ => 10_000,
    }
}

/// `TerminalID`, which the schema spells and `tt-vt` also spells.
///
/// Two lists of the same fourteen names is one more than there should be, and
/// they cannot be shared: the schema's exists so the *file* can be parsed
/// without depending on the terminal. So they are joined by the INI's own
/// spelling — the string upstream itself compares — and
/// `every_terminal_id_is_a_name_both_crates_know` fails if either list grows a
/// name the other does not have.
fn term_id(s: &Settings) -> TermId {
    TermId::parse(s.terminal_id.as_ini()).unwrap_or_default()
}

/// How many rows the page has, which `MaxBuffSize` can cut down.
///
/// `buffer.c:4977` caps `BuffChangeTerminalSize`'s own `Ny` with
/// `ts.ScrollBuffMax` before it does anything else, so the ceiling on the
/// *buffer* is a ceiling on the terminal too: `MaxBuffSize=10` is a ten-row
/// terminal in a window of any size. It only bites below `TermHeightMax`'s
/// 500, and the key ships at 10000, so this is a cap nobody meets by accident
/// — which is exactly why it has to be here rather than assumed away.
fn rows(s: &Settings) -> usize {
    let rows = s.terminal_rows.max(1) as usize;
    rows.min(buffer_max(s))
}

/// How deep the history goes, which is **not** what `ScrollBuffSize` says.
///
/// Upstream's is the whole buffer, page included, and it is clamped up to the
/// page rather than the page being clamped down to it (`buffer.c:641`, and
/// again at `:4983`). With `EnableScrollBuff` off the buffer *is* the page.
/// `Grid` counts the lines beyond the page, so the page comes back off.
///
/// `MaxBuffSize` is the ceiling over the whole thing (`buffer.c:511`), applied
/// to the page and the buffer separately because that is the order upstream
/// applies it in — the rows are cut first and the total after, so a small
/// ceiling gives no history rather than negative history.
fn scrollback_max(s: &Settings) -> usize {
    if !s.terminal_scrollback_enabled {
        return 0;
    }
    let lines = (s.terminal_scrollback_lines.max(0) as usize).min(buffer_max(s));
    lines.saturating_sub(rows(s))
}

/// `ts.ScrollBuffMax`, as a number of lines that is never zero.
///
/// The schema already refuses anything under 24 on the way in — `ttset.c:1214`
/// takes the *default* below that rather than the floor — so this only guards
/// the `Settings` a caller built by hand.
fn buffer_max(s: &Settings) -> usize {
    s.terminal_buffer_max_lines.max(1) as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use tt_config::{Ini, TerminalId};

    #[test]
    fn the_defaults_map_onto_the_terminals_defaults() {
        let mapped = vt_config(&Settings::default(), &Config::default());
        let d = Config::default();
        // Everything except the history, which upstream ships at 100 lines
        // *including* the page while `Config::default` carries `MaxBuffSize`.
        assert_eq!(mapped.scrollback_max, 100 - 24);
        // ...the title, whose `Title=` default is upstream's own product name,
        // and receive CR, whose product default is deliberately Auto while
        // the compatibility engine stays on upstream's CR. These are the
        // documented differences rather than transcription slips.
        assert_eq!(mapped.title, "Tera Term");
        assert_eq!(mapped.cr_receive, CrReceive::Auto);
        assert_eq!(
            Config {
                scrollback_max: d.scrollback_max,
                title: d.title.clone(),
                cr_receive: d.cr_receive,
                ..mapped
            },
            d,
            "a setting whose default disagrees with the terminal's is one of \
             them being wrong about upstream"
        );
    }

    #[test]
    fn every_terminal_id_is_a_name_both_crates_know() {
        for name in ["VT100", "VT102J", "VT220", "VT320", "VT525", "dumb"] {
            let id = TerminalId::from_ini(name);
            assert_eq!(id.as_ini(), name, "the schema lost {name}");
            assert!(TermId::parse(name).is_some(), "tt-vt lost {name}");
        }
        let s = Settings {
            terminal_id: TerminalId::Vt320,
            ..Settings::default()
        };
        assert_eq!(term_id(&s), TermId::Vt320);
    }

    #[test]
    fn the_history_is_the_buffer_minus_the_page() {
        let mut s = Settings {
            terminal_rows: 24,
            terminal_scrollback_lines: 1000,
            ..Settings::default()
        };
        assert_eq!(scrollback_max(&s), 976);

        // Upstream grows the buffer to hold the page rather than shrinking the
        // page to the buffer, so this is a terminal with no history, not one
        // with negative history or 10 rows.
        s.terminal_scrollback_lines = 10;
        assert_eq!(scrollback_max(&s), 0);

        s.terminal_scrollback_lines = 1000;
        s.terminal_scrollback_enabled = false;
        assert_eq!(scrollback_max(&s), 0);
    }

    /// `MaxBuffSize` is a ceiling over both halves, and it is the row count
    /// that makes it worth having: a file can ask for a terminal upstream will
    /// not give it, and the window has to agree about the size.
    #[test]
    fn the_buffer_ceiling_cuts_the_page_as_well_as_the_history() {
        let of = |bytes: &[u8]| vt_config(&Settings::load(&Ini::parse(bytes)), &Config::default());

        // Nothing set: the shipped 10000 is far above both, so neither moves.
        let plain = of(b"[Tera Term]\r\nTerminalSize=80,50\r\nScrollBuffSize=2000\r\n");
        assert_eq!((plain.rows, plain.scrollback_max), (50, 1950));

        // A ceiling between the two: the history is cut to it and the page is
        // not, which is the whole of what the setting normally does.
        let capped =
            of(b"[Tera Term]\r\nTerminalSize=80,50\r\nScrollBuffSize=2000\r\nMaxBuffSize=500\r\n");
        assert_eq!((capped.rows, capped.scrollback_max), (50, 450));

        // ...and below the page, where it takes the terminal down with it.
        // `buffer.c:4977` cuts `Ny` before the buffer is sized at all.
        let tiny =
            of(b"[Tera Term]\r\nTerminalSize=80,50\r\nScrollBuffSize=2000\r\nMaxBuffSize=30\r\n");
        assert_eq!((tiny.rows, tiny.scrollback_max), (30, 0));

        // Under 24 is not a floor — `ttset.c:1214` takes the default — so this
        // is a 50-row terminal and not a one-row one.
        let refused =
            of(b"[Tera Term]\r\nTerminalSize=80,50\r\nScrollBuffSize=2000\r\nMaxBuffSize=1\r\n");
        assert_eq!((refused.rows, refused.scrollback_max), (50, 1950));
    }

    /// The shift list is a string in the schema and a bitmask in the terminal,
    /// and the conversion is the one place that can be wrong.
    #[test]
    fn the_shift_list_reaches_the_terminal() {
        let of = |bytes: &[u8]| vt_config(&Settings::load(&Ini::parse(bytes)), &Config::default());
        assert_eq!(of(b"[Tera Term]\r\n").iso2022_flags, ShiftFlags::ALL);
        assert_eq!(
            of(b"[Tera Term]\r\nISO2022ShiftFunction=off\r\n").iso2022_flags,
            ShiftFlags::NONE
        );
        assert_eq!(
            of(b"[Tera Term]\r\nISO2022ShiftFunction=SI,SO\r\n").iso2022_flags,
            ShiftFlags(ShiftFlags::SI | ShiftFlags::SO)
        );
    }

    #[test]
    fn the_ansi_palette_reaches_the_terminals_table_order() {
        let base = Config::default();
        let c = vt_config(
            &Settings::load(&Ini::parse(b"[Tera Term]\r\nANSIColor=1,1,2,3,9,4,5,6\r\n")),
            &base,
        );

        // Legacy 1 is bright index 9 in the drawing table; legacy 9 is dim
        // index 1. A partial list leaves every entry it did not name alone.
        assert_eq!(c.palette[9], (1, 2, 3));
        assert_eq!(c.palette[1], (4, 5, 6));
        assert_eq!(c.palette[10], base.palette[10]);
    }

    #[test]
    fn the_ansi_palette_keeps_upstreams_narrowing_quirks() {
        let base = Config::default().palette;
        let p = ansi_palette("2,-1,256,257,17,1,2,3,1,4,5,6,3,nope,7,8,4,9", base);

        assert_eq!(p[10], (255, 0, 1), "channels narrow to BYTE");
        assert_eq!(p[9], (4, 5, 6), "IDs mask and the last group wins");
        assert_eq!(p[11], (0, 7, 8), "a failed %d conversion is zero");
        assert_eq!(p[12], base[12], "an incomplete group is not read");

        // `GetNthNum` copies only fourteen bytes of an individual field.
        let narrow = format!("4,{}9,2,3", " ".repeat(14));
        assert_eq!(ansi_palette(&narrow, base)[12], (0, 2, 3));

        // `GetPrivateProfileString` first truncates the entire value to 259
        // bytes. Without that boundary the final group would overwrite 0.
        let mut long = "0,1,2,3,".to_string();
        long.push_str(&" ".repeat(251));
        long.push_str("1,9,9,9");
        assert_eq!(ansi_palette(&long, base)[0], (1, 2, 3));
    }

    /// The two of the parser's special options that are not scalars, and the
    /// conversion each needs on the way through.
    #[test]
    fn the_tab_stop_list_and_the_unit_id_reach_the_terminal() {
        let of = |bytes: &[u8]| vt_config(&Settings::load(&Ini::parse(bytes)), &Config::default());

        assert_eq!(of(b"[Tera Term]\r\n").tab_stop_modify, TabStopFlags::ALL);
        assert_eq!(
            of(b"[Tera Term]\r\nTabStopModifySequence=off\r\n").tab_stop_modify,
            TabStopFlags::NONE
        );
        assert_eq!(
            of(b"[Tera Term]\r\nTabStopModifySequence=HTS7,TBC\r\n").tab_stop_modify,
            TabStopFlags(TabStopFlags::HTS7 | TabStopFlags::TBC)
        );

        // Eight hex digits, upper-cased; anything else is the key's default,
        // which is upstream's own fallback rather than a refusal.
        assert_eq!(of(b"[Tera Term]\r\n").terminal_uid, "FFFFFFFF");
        assert_eq!(
            of(b"[Tera Term]\r\nTerminalUID=0f1e2d3c\r\n").terminal_uid,
            "0F1E2D3C"
        );
        for bad in [
            &b"[Tera Term]\r\nTerminalUID=123456789\r\n"[..],
            b"[Tera Term]\r\nTerminalUID=1234567\r\n",
            b"[Tera Term]\r\nTerminalUID=zzzzzzzz\r\n",
            b"[Tera Term]\r\nTerminalUID=\r\n",
        ] {
            assert_eq!(of(bad).terminal_uid, "FFFFFFFF");
        }
        // And the file keeps what the user wrote, which is the reason the
        // validation is here and not in the schema.
        assert_eq!(
            Settings::load(&Ini::parse(b"[Tera Term]\r\nTerminalUID=nope\r\n")).terminal_uid,
            "nope"
        );
    }

    /// Four settings the terminal already honoured and the file could not say.
    #[test]
    fn the_four_that_had_no_key_have_one() {
        let d = vt_config(&Settings::default(), &Config::default());
        assert!(d.color_flags.ansi_color);
        assert!(!d.disable_app_keypad);
        assert!(!d.disable_app_cursor);

        let c = vt_config(
            &Settings::load(&Ini::parse(
                b"[Tera Term]\r\n\
                  EnableANSIColor=off\r\n\
                  DisableAppKeypad=on\r\n\
                  DisableAppCursor=on\r\n",
            )),
            &Config::default(),
        );
        assert!(!c.color_flags.ansi_color);
        assert!(c.disable_app_keypad);
        assert!(c.disable_app_cursor);
    }

    /// The timestamp is two keys, and which one answers depends on whether the
    /// *first* is there at all.
    #[test]
    fn the_timestamp_type_falls_back_to_tera_term_4s_key() {
        let of = |bytes: &[u8]| log_options(&Settings::load(&Ini::parse(bytes))).timestamp;

        // The switch is a key of its own: a type with `LogTimestamp=off` is
        // no stamp at all, which is what makes the setting pair usable.
        assert_eq!(
            of(b"[Tera Term]\r\nLogTimestampType=UTC\r\n"),
            Timestamp::None
        );

        let on = b"[Tera Term]\r\nLogTimestamp=on\r\n";
        assert_eq!(of(on), Timestamp::Local, "the `else` of the chain");

        // A Tera Term 4 file: no type key, and the compatibility key answers.
        let mut tt4 = on.to_vec();
        tt4.extend_from_slice(b"LogTimestampUTC=on\r\n");
        assert_eq!(of(&tt4), Timestamp::Utc);

        // ...and the same file after a Tera Term 5 has saved it, which writes
        // the new key and leaves the old one behind. Local wins, because the
        // key it would have consulted is no longer absent.
        let mut both = tt4.clone();
        both.extend_from_slice(b"LogTimestampType=Local\r\n");
        assert_eq!(of(&both), Timestamp::Local);

        let mut elapsed = on.to_vec();
        elapsed.extend_from_slice(b"LogTimestampType=ConnectionElapsed\r\n");
        assert_eq!(of(&elapsed), Timestamp::ElapsedConnection);
    }

    /// Three keys decide rotation and two of them are traps: the size is
    /// already in bytes, and a step of zero is ten thousand generations.
    #[test]
    fn rotation_reads_bytes_and_a_zero_step_is_not_none() {
        let of = |bytes: &[u8]| log_options(&Settings::load(&Ini::parse(bytes)));

        // Off by default, and the size alone does not switch it on.
        let sized = of(b"[Tera Term]\r\nLogRotateSize=1048576\r\n");
        assert_eq!((sized.rotate_size, sized.rotate_keep), (0, 0));

        // `LogRotateSizeType=2` says the *dialog* shows this as 2 MB. The
        // stored number is still bytes, so nothing here multiplies it.
        let on =
            of(b"[Tera Term]\r\nLogRotate=1\r\nLogRotateSize=2097152\r\nLogRotateSizeType=2\r\n");
        assert_eq!(on.rotate_size, 2_097_152);
        assert_eq!(on.rotate_keep, 10_000, "filesys_log.cpp:507's loopmax");

        let stepped =
            of(b"[Tera Term]\r\nLogRotate=1\r\nLogRotateSize=4096\r\nLogRotateStep=3\r\n");
        assert_eq!((stepped.rotate_size, stepped.rotate_keep), (4096, 3));

        // Neither 0 nor 1: upstream's `if` chain falls out and rotates
        // nothing, so a range that clamped this to 1 would switch rotation on
        // for a file that had it off.
        let odd = of(b"[Tera Term]\r\nLogRotate=2\r\nLogRotateSize=4096\r\nLogRotateStep=3\r\n");
        assert_eq!((odd.rotate_size, odd.rotate_keep), (0, 0));
    }

    #[test]
    fn a_binary_log_is_the_mode_and_not_a_second_flag() {
        let s = Settings::load(&Ini::parse(
            b"[Tera Term]\r\nLogBinary=on\r\nLogAppend=on\r\nLogTimestamp=on\r\n",
        ));
        let o = log_options(&s);
        assert_eq!(o.mode, LogMode::Raw);
        assert!(o.append);
        // Asked for and dropped, but by `SessionLog::open` rather than here —
        // so the options a caller passes are the options they wrote, and the
        // ones the log reports back are the ones in force.
        assert_eq!(o.timestamp, Timestamp::Local);
    }

    #[test]
    fn a_file_reaches_the_terminal() {
        let ini = Ini::parse(
            b"[Tera Term]\r\n\
              TerminalSize=132,50\r\n\
              TerminalID=VT320\r\n\
              CRReceive=CRLF\r\n\
              BSKey=DEL\r\n\
              Xterm256Color=off\r\n\
              CursorShape=vertical\r\n",
        );
        let c = vt_config(&Settings::load(&ini), &Config::default());
        assert_eq!((c.cols, c.rows), (132, 50));
        assert_eq!(c.term_id, TermId::Vt320);
        assert_eq!(c.cr_receive, CrReceive::CrLf);
        assert!(!c.bs_key_is_bs);
        assert!(!c.color_flags.xterm256);
        assert_eq!(c.cursor_shape, 5, "DECSCUSR's number, not IdVCur's 2");
    }
}

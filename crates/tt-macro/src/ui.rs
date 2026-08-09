//! The half of a macro's world that is not the session.
//!
//! [`tt_ttl::ScriptHost`] is one trait covering everything a command needs from
//! outside, and most of it is answerable from a [`Session`](tt_session::Session)
//! — `send`, `connect`, `logopen`, the transfers. What is left is a window and a
//! person in front of it: eleven dialogs, the clipboard, the menu, the title
//! bar. A frontend implements this; the rest it gets for free.
//!
//! Every method refuses by default, which is the same bargain `ScriptHost`
//! makes and for the same reason: a frontend that has three dialogs is useful,
//! and the commands it has not answered report "Unknown command" rather than
//! quietly succeeding. [`NullUi`] is that state made explicit, and it is what
//! the tests here run against — a macro with no window at all, which is a real
//! configuration once `ttpmacro script.ttl` exists.

use tt_conn::Transport;
use tt_session::open::Target;
use tt_ttl::host::{
    BeepSound, DialogEnd, DialogPos, ListBoxOpts, MacroWindow, ShowWindow, WindowGeometry,
};
use tt_ttl::TtlError;

/// An error report, owned.
///
/// [`tt_ttl::host::ErrorReport`] borrows the source line out of the
/// interpreter's buffer, which is on the macro's thread and gone by the time
/// the dialog closes. Nothing is borrowed across this boundary — that is the
/// rule the channel is built on, and the SSH host-key prompt is the worked
/// example of what breaking it costs.
/// It carries the *sentence* rather than the code alone, because there is more
/// than one language now. `tt-ttl`'s errors are `ttmparse.h`'s twenty-one, each
/// with upstream's wording; a Lua error is a traceback and belongs to nothing
/// upstream numbered. A frontend shows `message` either way and reads `code`
/// only if it wants to tell them apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroError {
    /// `ttmparse.h`'s number, or **0** for an error that is not one of
    /// upstream's — which is every error from a language other than TTL.
    pub code: u32,
    pub message: String,
    pub line: Vec<u8>,
    pub line_no: usize,
    pub start: usize,
    pub end: usize,
    pub file: String,
}

impl MacroError {
    pub fn from_report(r: &tt_ttl::host::ErrorReport<'_>) -> MacroError {
        MacroError {
            code: r.error.code().into(),
            message: r.error.message().to_string(),
            line: r.line.to_vec(),
            line_no: r.line_no,
            start: r.start,
            end: r.end,
            file: r.file.to_string(),
        }
    }

    /// An error from a language that is not TTL, which has no line to point at
    /// and no number to report.
    ///
    /// The message is expected to name its own position — a Lua error opens
    /// with `chunk:line:`, which is why nothing here tries to take that apart
    /// and hand the pieces over separately.
    pub fn elsewhere(message: String, file: String) -> MacroError {
        MacroError {
            code: 0,
            message,
            line: Vec::new(),
            line_no: 0,
            start: 0,
            end: 0,
            file,
        }
    }
}

/// What a running macro asks of the window and the user.
pub trait MacroUi {
    /// `DispErr` — the error dialog. **`true` stops the macro**; upstream's
    /// two buttons are Stop (`IDOK`) and Continue (`IDCANCEL`), and Continue is
    /// the one that is not the default.
    ///
    /// A frontend with no dialog should return `true`: a script that has hit a
    /// syntax error and cannot say so is better stopped than left running.
    fn error(&mut self, err: &MacroError) -> bool {
        let _ = err;
        true
    }

    /// `messagebox` — one OK button. [`DialogEnd::Closed`] is the window's
    /// close box, which upstream distinguishes from Cancel.
    fn message_box(&mut self, text: &[u8], title: &[u8]) -> Result<DialogEnd, TtlError> {
        let _ = (text, title);
        Err(TtlError::NotSupported)
    }

    /// `yesnobox`.
    fn yes_no_box(&mut self, text: &[u8], title: &[u8]) -> Result<DialogEnd, TtlError> {
        let _ = (text, title);
        Err(TtlError::NotSupported)
    }

    /// `statusbox` — a modeless box the macro updates as it goes. Calling it
    /// again with a box already up replaces its text rather than opening a
    /// second.
    fn status_box(&mut self, text: &[u8], title: &[u8]) -> Result<(), TtlError> {
        let _ = (text, title);
        Err(TtlError::NotSupported)
    }

    /// `closesbox`. Closing one that is not open is not an error.
    fn close_status_box(&mut self) -> Result<(), TtlError> {
        Ok(())
    }

    /// `bringupbox` — raise the status box.
    fn bringup_status_box(&mut self) -> Result<(), TtlError> {
        Err(TtlError::NotSupported)
    }

    /// `listbox`. The answer is an index into `items`.
    fn list_box(
        &mut self,
        text: &[u8],
        title: &[u8],
        items: &[Vec<u8>],
        selected: usize,
        opts: &ListBoxOpts,
    ) -> Result<DialogEnd<usize>, TtlError> {
        let _ = (text, title, items, selected, opts);
        Err(TtlError::NotSupported)
    }

    /// `inputbox`, and `passwordbox` when `password` is set.
    fn input_box(
        &mut self,
        text: &[u8],
        title: &[u8],
        default: &[u8],
        password: bool,
    ) -> Result<DialogEnd<Vec<u8>>, TtlError> {
        let _ = (text, title, default, password);
        Err(TtlError::NotSupported)
    }

    /// `filenamebox` — `None` is cancelled.
    fn filename_box(
        &mut self,
        title: &[u8],
        save: bool,
        init_dir: &[u8],
    ) -> Result<Option<Vec<u8>>, TtlError> {
        let _ = (title, save, init_dir);
        Err(TtlError::NotSupported)
    }

    /// `dirnamebox` — `None` is cancelled.
    fn dirname_box(&mut self, title: &[u8], init_dir: &[u8]) -> Result<Option<Vec<u8>>, TtlError> {
        let _ = (title, init_dir);
        Err(TtlError::NotSupported)
    }

    /// `setdlgpos`. A preference with no user in it, so it cannot fail.
    fn set_dialog_pos(&mut self, pos: Option<DialogPos>) {
        let _ = pos;
    }

    /// `dispstr` — write text to the terminal *locally*, as though the far end
    /// had sent it. Not the same as `send`, which puts it on the wire.
    ///
    /// The session could do this on its own with `feed`, and that is exactly
    /// what [`crate::SessionHost`] does; it is here as well because a frontend
    /// with no session — `ttpmacro` run against nothing — still has somewhere
    /// to put it.
    fn disp_str(&mut self, s: &[u8]) -> Result<(), TtlError> {
        let _ = s;
        Err(TtlError::NotSupported)
    }

    /// `beep`.
    fn beep(&mut self, sound: BeepSound) -> Result<(), TtlError> {
        let _ = sound;
        Err(TtlError::NotSupported)
    }

    /// `callmenu` — invoke a menu item by its Windows command id.
    ///
    /// The ids are `teraterm.rc`'s and there are about ninety of them, so a
    /// port answers the ones it has a menu item for and refuses the rest. That
    /// is honest: a macro asking for "Setup > TEK window" on a build with no
    /// TEK window has not been misunderstood, it has asked for something that
    /// is not there.
    fn call_menu(&mut self, id: i32) -> Result<(), TtlError> {
        let _ = id;
        Err(TtlError::NotSupported)
    }

    /// `showtt` — hide, minimise or restore the terminal window.
    fn show_window(&mut self, which: ShowWindow) -> Result<(), TtlError> {
        let _ = which;
        Err(TtlError::NotSupported)
    }

    /// `show` — the same for the macro's own control window.
    fn show_macro_window(&mut self, how: MacroWindow) -> Result<(), TtlError> {
        let _ = how;
        Err(TtlError::NotSupported)
    }

    /// `getttpos` — where the window is, in screen pixels.
    fn terminal_geometry(&mut self) -> Result<Option<WindowGeometry>, TtlError> {
        Ok(None)
    }

    /// `enablekeyb` — lock the keyboard so a script's prompts are not typed
    /// over.
    fn enable_keyboard(&mut self, on: bool) -> Result<(), TtlError> {
        let _ = on;
        Err(TtlError::NotSupported)
    }

    /// `clipb2var` — `None` when the clipboard holds no text, which is what
    /// upstream's failed `GetClipboardData` amounts to.
    fn clipboard_text(&mut self) -> Option<Vec<u8>> {
        None
    }

    /// `var2clipb`. `false` is upstream's failure, which the command reports.
    fn set_clipboard_text(&mut self, text: &[u8]) -> bool {
        let _ = text;
        false
    }

    /// `setexitcode` — what the process exits with once the macro ends.
    fn set_exit_code(&mut self, code: i32) {
        let _ = code;
    }

    /// The one connection a macro's `connect` cannot open for itself.
    ///
    /// `target` is always [`Target::Ssh`], because [`Target::open`] opens the
    /// other three and refuses that one: a host key or a password is a
    /// **prompt**, and a prompt belongs to whoever owns a window. Upstream
    /// agrees about where it goes — TTSSH puts its dialogs on the terminal's
    /// thread while the macro that asked sleeps — and this call is on the
    /// frontend's thread for exactly that reason, so an implementation may spin
    /// a nested event loop the way the dialogs above do.
    ///
    /// `Ok(None)` is a connection that did not come up, which the macro reads
    /// as `result` 1 — and it is the **default**, rather than the refusal every
    /// other method here makes. A `connect` that answered "Unknown command"
    /// would be the larger lie: the command exists, it works for the other
    /// three transports, and "not connected" is an outcome the documentation
    /// already promises. A frontend with SSH dialogs implements this; one
    /// without it leaves `connect '… /ssh'` reporting 1, which a script can
    /// test for.
    fn connect_ssh(&mut self, target: &Target) -> Result<Option<Box<dyn Transport>>, TtlError> {
        let _ = target;
        Ok(None)
    }
}

/// A frontend that is not there: every dialog refuses and nothing is shown.
///
/// This is what `ttpmacro script.ttl` with no window would use, and it is what
/// the tests in this crate run against — so everything they prove is about the
/// session half and not about a dialog somebody stubbed.
#[derive(Debug, Default, Clone, Copy)]
pub struct NullUi;

impl MacroUi for NullUi {}

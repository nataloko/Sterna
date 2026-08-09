//! The dialogs.
//!
//! Every one of these blocks, which is why the script must not be on the UI
//! thread: a frontend answers them by spinning its own event loop. That is
//! upstream's shape too — its dialogs are `ttpmacro.exe`'s own windows, put up
//! on the thread running the macro — and it is the same rule `tt.wait` already
//! imposes for a different reason.
//!
//! **The one divergence runs through all of them: a dialog does not end the
//! script here.** Upstream's `messagebox` ends the macro when the user closes
//! the window rather than pressing OK, and it has to: the dialog is the only
//! control a person has over a running `ttpmacro.exe`. Here there is an End
//! button, and it reaches a script that is not in a dialog at all — so the
//! answer is returned and the script decides. A script that wants upstream's
//! behaviour writes `if tt.messagebox(...) == 'closed' then return end`.

use mlua::{BString, Scope, Table, Value};
use tt_ttl::{DialogAnchor, DialogEnd, DialogOrigin, DialogPos, ListBoxOpts};

use crate::{choice, lua_err, Host};

/// How the three ways out of a dialog are spelled in Lua.
///
/// `'ok'`, `'cancel'` and `'closed'` rather than two booleans, because they
/// are three states and `Closed` is the one upstream treats as an instruction
/// rather than an answer.
fn end_name(e: &DialogEnd<()>) -> &'static str {
    match e {
        DialogEnd::Ok(()) => "ok",
        DialogEnd::Cancel => "cancel",
        DialogEnd::Closed => "closed",
    }
}

/// `listbox`'s keyword parameters, out of an options table.
///
/// All of them are hints about the window rather than about the choice, so a
/// host that ignores the lot still implements `listbox` correctly — which is
/// why an unknown key here is quietly ignored rather than being an error.
fn list_opts(t: Option<&Table>) -> mlua::Result<(ListBoxOpts, usize)> {
    let mut o = ListBoxOpts::default();
    let mut selected = 1usize;
    let Some(t) = t else {
        return Ok((o, selected));
    };
    o.double_click = t.get::<Option<bool>>("dblclick")?.unwrap_or(false);
    o.min_max_button = t.get::<Option<bool>>("minmaxbutton")?.unwrap_or(false);
    o.minimized = t.get::<Option<bool>>("minimize")?.unwrap_or(false);
    o.maximized = t.get::<Option<bool>>("maximize")?.unwrap_or(false);
    if let (Some(w), Some(h)) = (
        t.get::<Option<u32>>("width")?,
        t.get::<Option<u32>>("height")?,
    ) {
        o.size = Some((w, h));
    }
    if let Some(n) = t.get::<Option<i64>>("selected")? {
        selected = n.max(1) as usize;
    }
    Ok((o, selected))
}

/// `setdlgpos`'s argument, out of a table.
fn dialog_pos(t: &Table) -> mlua::Result<DialogPos> {
    let anchor = match t.get::<Option<String>>("anchor")? {
        None => None,
        Some(a) => {
            let corner = choice(
                &a,
                "anchor",
                &[
                    ("topleft", DialogAnchor::TopLeft),
                    ("topright", DialogAnchor::TopRight),
                    ("bottomleft", DialogAnchor::BottomLeft),
                    ("bottomright", DialogAnchor::BottomRight),
                    ("center", DialogAnchor::Center),
                ],
            )?;
            let origin = match t.get::<Option<String>>("origin")? {
                None => DialogOrigin::Display,
                Some(o) => choice(
                    &o,
                    "origin",
                    &[
                        ("display", DialogOrigin::Display),
                        ("window", DialogOrigin::VtWindow),
                    ],
                )?,
            };
            Some((corner, origin))
        }
    };
    Ok(DialogPos {
        x: t.get::<Option<i32>>("x")?.unwrap_or(0),
        y: t.get::<Option<i32>>("y")?.unwrap_or(0),
        anchor,
        offset_x: t.get::<Option<i32>>("offsetx")?.unwrap_or(0),
        offset_y: t.get::<Option<i32>>("offsety")?.unwrap_or(0),
    })
}

pub(crate) fn install<'s, 'e>(
    scope: &'s Scope<'s, 'e>,
    tt: &Table,
    host: &'e Host<'e>,
) -> mlua::Result<()> {
    tt.set(
        "messagebox",
        scope.create_function(move |_, (text, title): (BString, Option<BString>)| {
            let title = title.unwrap_or_default();
            let e = host
                .borrow_mut()
                .message_box(&text, &title)
                .map_err(lua_err)?;
            Ok(end_name(&e))
        })?,
    )?;

    // `true` for Yes, `false` for No, `nil` for the window's close button.
    // Both of the last two are falsy, so `if tt.yesnobox(...) then` reads
    // correctly without knowing there are three answers — and a script that
    // cares can tell them apart, which upstream's `result` cannot.
    tt.set(
        "yesnobox",
        scope.create_function(move |_, (text, title): (BString, Option<BString>)| {
            let title = title.unwrap_or_default();
            let e = host
                .borrow_mut()
                .yes_no_box(&text, &title)
                .map_err(lua_err)?;
            Ok(match e {
                DialogEnd::Ok(()) => Value::Boolean(true),
                DialogEnd::Cancel => Value::Boolean(false),
                DialogEnd::Closed => Value::Nil,
            })
        })?,
    )?;

    // One dialog, not one per call: the second `statusbox` retitles the first,
    // and only `closesbox` closes it. It does not block.
    tt.set(
        "statusbox",
        scope.create_function(move |_, (text, title): (BString, Option<BString>)| {
            let title = title.unwrap_or_default();
            host.borrow_mut().status_box(&text, &title).map_err(lua_err)
        })?,
    )?;
    tt.set(
        "closesbox",
        scope
            .create_function(move |_, ()| host.borrow_mut().close_status_box().map_err(lua_err))?,
    )?;
    tt.set(
        "bringupbox",
        scope.create_function(move |_, ()| {
            host.borrow_mut().bringup_status_box().map_err(lua_err)
        })?,
    )?;

    // The chosen item's index, 1-based both ways — Lua's convention and the
    // one `opts.selected` is read in, so nothing here is off by one. `nil` is
    // Cancel; `false` is the close button, which is upstream's -2 and the one
    // answer a `nil` alone could not carry.
    tt.set(
        "listbox",
        scope.create_function(
            move |_, (text, title, items, opts): (BString, BString, Table, Option<Table>)| {
                let items: Vec<Vec<u8>> = items
                    .sequence_values::<BString>()
                    .map(|v| v.map(Vec::from))
                    .collect::<mlua::Result<_>>()?;
                if items.is_empty() {
                    return Err(mlua::Error::runtime("listbox needs at least one item"));
                }
                let (opts, selected) = list_opts(opts.as_ref())?;
                // Upstream folds an out-of-range index to the first item
                // (`ttl_gui.cpp:512`) rather than refusing, so the host is
                // promised a valid one.
                let selected = if selected <= items.len() {
                    selected - 1
                } else {
                    0
                };
                let refs: Vec<&[u8]> = items.iter().map(|v| v.as_slice()).collect();
                let e = host
                    .borrow_mut()
                    .list_box(&text, &title, &refs, selected, &opts)
                    .map_err(lua_err)?;
                Ok(match e {
                    DialogEnd::Ok(i) => Value::Integer(i as i64 + 1),
                    DialogEnd::Cancel => Value::Nil,
                    DialogEnd::Closed => Value::Boolean(false),
                })
            },
        )?,
    )?;

    // Escape is `nil`. Upstream cannot tell it from OK — the dialog reports
    // the same code for both and `inputstr` is left holding an uninitialised
    // stack buffer, which is one of the defects on file — so answering it as
    // an empty string is what the documentation promises and this is the
    // honest version of that.
    tt.set(
        "inputbox",
        scope.create_function(
            move |lua, (text, title, default): (BString, Option<BString>, Option<BString>)| {
                let e = host
                    .borrow_mut()
                    .input_box(
                        &text,
                        &title.unwrap_or_default(),
                        &default.unwrap_or_default(),
                        false,
                    )
                    .map_err(lua_err)?;
                Ok(match e {
                    DialogEnd::Ok(s) => Value::String(lua.create_string(s)?),
                    _ => Value::Nil,
                })
            },
        )?,
    )?;
    tt.set(
        "passwordbox",
        scope.create_function(move |lua, (text, title): (BString, Option<BString>)| {
            let e = host
                .borrow_mut()
                .input_box(&text, &title.unwrap_or_default(), b"", true)
                .map_err(lua_err)?;
            Ok(match e {
                DialogEnd::Ok(s) => Value::String(lua.create_string(s)?),
                _ => Value::Nil,
            })
        })?,
    )?;

    tt.set(
        "filenamebox",
        scope.create_function(
            move |lua, (title, save, dir): (BString, Option<bool>, Option<BString>)| {
                let r = host
                    .borrow_mut()
                    .filename_box(&title, save.unwrap_or(false), &dir.unwrap_or_default())
                    .map_err(lua_err)?;
                Ok(match r {
                    Some(p) => Value::String(lua.create_string(p)?),
                    None => Value::Nil,
                })
            },
        )?,
    )?;
    tt.set(
        "dirnamebox",
        scope.create_function(move |lua, (title, dir): (BString, Option<BString>)| {
            let r = host
                .borrow_mut()
                .dirname_box(&title, &dir.unwrap_or_default())
                .map_err(lua_err)?;
            Ok(match r {
                Some(p) => Value::String(lua.create_string(p)?),
                None => Value::Nil,
            })
        })?,
    )?;

    // No argument is upstream's no-argument form: `CW_USEDEFAULT` in both
    // coordinates, which every dialog reads as "centre me".
    tt.set(
        "setdlgpos",
        scope.create_function(move |_, where_: Option<Table>| {
            let pos = match where_ {
                None => None,
                Some(t) => Some(dialog_pos(&t)?),
            };
            host.borrow_mut().set_dialog_pos(pos);
            Ok(())
        })?,
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::tests::run;
    use crate::Script;
    use tt_ttl::{DialogEnd, RecordingHost};

    fn answering(replies: Vec<DialogEnd>, src: &str) -> RecordingHost {
        let mut host = RecordingHost::new();
        host.linked = true;
        host.msg_replies = replies.into();
        Script::new("t.lua", src.as_bytes().to_vec())
            .run(&mut host)
            .unwrap();
        host
    }

    #[test]
    fn a_message_box_reports_how_it_was_dismissed() {
        let h = answering(
            vec![DialogEnd::Ok(()), DialogEnd::Closed],
            "tt.dispstr(tt.messagebox('a'), tt.messagebox('b'))",
        );
        assert_eq!(h.output, b"okclosed");
    }

    /// The divergence: upstream stops the macro on `Closed`.
    #[test]
    fn a_closed_box_does_not_end_the_script() {
        let h = answering(
            vec![DialogEnd::Closed],
            "tt.messagebox('a'); tt.dispstr('still here')",
        );
        assert_eq!(h.output, b"still here");
    }

    #[test]
    fn yesnobox_tells_no_from_the_close_button() {
        let h = answering(
            vec![DialogEnd::Cancel, DialogEnd::Closed],
            "tt.dispstr(tostring(tt.yesnobox('a')), tostring(tt.yesnobox('b')))",
        );
        assert_eq!(h.output, b"falsenil");
    }

    #[test]
    fn a_listbox_index_is_one_based_both_ways() {
        let mut host = RecordingHost::new();
        host.linked = true;
        host.list_replies = vec![DialogEnd::Ok(2)].into();
        let src = "tt.dispstr(tostring(tt.listbox('pick', 'T', {'a','b','c'}, {selected=3})))";
        Script::new("t.lua", src.as_bytes().to_vec())
            .run(&mut host)
            .unwrap();
        assert_eq!(host.output, b"3");
        // The host is handed the 0-based index upstream promises it.
        assert!(host.dialogs[0].contains("sel=2"), "{:?}", host.dialogs);
    }

    #[test]
    fn an_empty_listbox_is_refused_rather_than_shown() {
        let (_, r) = run("tt.listbox('pick', 'T', {})");
        assert!(r.unwrap_err().to_string().contains("at least one item"));
    }

    #[test]
    fn a_cancelled_inputbox_is_nil_rather_than_an_empty_string() {
        let mut host = RecordingHost::new();
        host.linked = true;
        host.input_replies = vec![DialogEnd::Cancel].into();
        Script::new(
            "t.lua",
            b"tt.dispstr(tostring(tt.inputbox('name?')))".to_vec(),
        )
        .run(&mut host)
        .unwrap();
        assert_eq!(host.output, b"nil");
    }

    #[test]
    fn a_password_box_asks_for_one() {
        let mut host = RecordingHost::new();
        host.linked = true;
        host.input_replies = vec![DialogEnd::Ok(b"hunter2".to_vec())].into();
        Script::new("t.lua", b"tt.dispstr(tt.passwordbox('pw?'))".to_vec())
            .run(&mut host)
            .unwrap();
        assert_eq!(host.output, b"hunter2");
        assert!(host.dialogs[0].contains("password"), "{:?}", host.dialogs);
    }
}

//! Long-lived Lua plugins.
//!
//! A macro owns one chunk and ends with it. A plugin runs its top-level chunk
//! once to declare actions and hooks, then keeps the VM — and therefore its
//! globals and closures — for the window's lifetime. The terminal command
//! surface is installed only while a callback runs. That makes top-level code
//! registration rather than a hidden startup macro, and lets the frontend run
//! callbacks through the same worker/host boundary as an ordinary script.

use std::cell::RefCell;
use std::path::Path;

use mlua::{Function, Lua, LuaOptions, RegistryKey, StdLib, Table, Value};
use tt_ttl::ScriptHost;

use crate::{conn, dlg, env, install_cancel_hook, install_print, log, seal, serial, term, xfer};
use crate::{Cancelled, Host, Recv};

/// A callback in one [`Plugin`].
///
/// The number is stable for the plugin's lifetime and deliberately opaque to
/// callers. It is the small value the C ABI and Qt actions can carry without
/// holding a Lua registry object.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CallbackId(pub usize);

/// One item the plugin wants in the window's menu bar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuItem {
    /// A slash-separated path such as `Control` or `Tools/Serial`.
    pub menu: String,
    pub label: String,
    /// Qt's portable shortcut spelling, for example `Ctrl+Alt+R`.
    pub shortcut: Option<String>,
    pub callback: CallbackId,
}

/// A shortcut which does not need a visible menu item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyBinding {
    /// Qt's portable shortcut spelling, for example `Ctrl+Alt+R`.
    pub sequence: String,
    pub callback: CallbackId,
}

/// A session lifecycle edge a plugin can observe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hook {
    Connect,
    Disconnect,
}

impl Hook {
    fn parse(name: &str) -> mlua::Result<Self> {
        match name {
            "connect" => Ok(Self::Connect),
            "disconnect" => Ok(Self::Disconnect),
            _ => Err(mlua::Error::runtime(format!(
                "hook '{name}' is not one of: connect, disconnect"
            ))),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Connect => "connect",
            Self::Disconnect => "disconnect",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct HookItem {
    hook: Hook,
    callback: CallbackId,
}

#[derive(Default)]
struct Declarations {
    callbacks: Vec<RegistryKey>,
    menus: Vec<MenuItem>,
    keys: Vec<KeyBinding>,
    hooks: Vec<HookItem>,
}

impl Declarations {
    fn remember(&mut self, lua: &Lua, callback: Function) -> mlua::Result<CallbackId> {
        let id = CallbackId(self.callbacks.len());
        self.callbacks.push(lua.create_registry_value(callback)?);
        Ok(id)
    }
}

/// A loaded plugin and its persistent Lua state.
///
/// Loading executes the file once with a `sterna` registration table and no
/// terminal attached. [`invoke`](Self::invoke) and [`emit`](Self::emit) attach
/// the ordinary `tt` command surface for one callback. The VM is `Send` (the
/// `mlua/send` feature is intentional), so a frontend can load and retain it
/// on the same worker which services those callbacks.
pub struct Plugin {
    name: String,
    lua: Lua,
    callbacks: Vec<RegistryKey>,
    menus: Vec<MenuItem>,
    keys: Vec<KeyBinding>,
    hooks: Vec<HookItem>,
    recv: RefCell<Recv>,
}

impl Plugin {
    /// Load one plugin from bytes.
    ///
    /// The top-level chunk may use Lua's safe standard library and `require`
    /// neighbours of the plugin file. It cannot use `tt`: no session event has
    /// selected a terminal yet. Registration closes when this returns, so a
    /// callback cannot mutate the menu tree behind the frontend's back.
    pub fn load(name: impl Into<String>, body: Vec<u8>) -> mlua::Result<Self> {
        let name = name.into();
        let lua = Lua::new_with(StdLib::ALL_SAFE, LuaOptions::default())?;
        let declarations = std::sync::Arc::new(std::sync::Mutex::new(Declarations::default()));
        let sterna = lua.create_table()?;

        install_menu(&lua, &sterna, declarations.clone())?;
        install_key(&lua, &sterna, declarations.clone())?;
        install_hook(&lua, &sterna, declarations.clone())?;
        seal_sterna(&lua, &sterna)?;
        lua.globals().set("sterna", &sterna)?;

        // A plugin is part of a window process, just as a macro is. Letting a
        // top-level typo call `os.exit` would close every tab in it.
        let os: Table = lua.globals().get("os")?;
        os.set("exit", Value::Nil)?;
        reach_neighbours(&lua, &name)?;

        let result = lua.load(&body[..]).set_name(format!("@{name}")).exec();

        // These functions are load-time declarations, not a mutable runtime
        // API. Removing them also releases their Rc before the declarations
        // are taken below.
        sterna.raw_set("menu", Value::Nil)?;
        sterna.raw_set("key", Value::Nil)?;
        sterna.raw_set("on", Value::Nil)?;
        result?;

        let mut declarations = declarations.lock().expect("plugin declarations poisoned");
        Ok(Self {
            name,
            lua,
            callbacks: std::mem::take(&mut declarations.callbacks),
            menus: std::mem::take(&mut declarations.menus),
            keys: std::mem::take(&mut declarations.keys),
            hooks: std::mem::take(&mut declarations.hooks),
            recv: RefCell::new(Recv::default()),
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn menus(&self) -> &[MenuItem] {
        &self.menus
    }

    pub fn keys(&self) -> &[KeyBinding] {
        &self.keys
    }

    pub fn has_hook(&self, hook: Hook) -> bool {
        self.hooks.iter().any(|item| item.hook == hook)
    }

    /// Run one menu or key callback.
    pub fn invoke(&self, callback: CallbackId, host: &mut dyn ScriptHost) -> mlua::Result<()> {
        self.call(callback, None, host)
    }

    /// Run every callback registered for a lifecycle edge, in declaration
    /// order. The hook name is passed as its only argument as well as being the
    /// selector, which keeps one shared callback useful for both edges.
    pub fn emit(&self, hook: Hook, host: &mut dyn ScriptHost) -> mlua::Result<()> {
        for item in &self.hooks {
            if item.hook == hook {
                self.call(item.callback, Some(hook.as_str()), host)?;
            }
        }
        Ok(())
    }

    fn call(
        &self,
        callback: CallbackId,
        argument: Option<&str>,
        host: &mut dyn ScriptHost,
    ) -> mlua::Result<()> {
        let key = self.callbacks.get(callback.0).ok_or_else(|| {
            mlua::Error::runtime(format!("callback {} does not exist", callback.0))
        })?;
        let cell: Host<'_> = RefCell::new(host);

        self.lua.scope(|scope| {
            let tt = self.lua.create_table()?;
            conn::install(scope, &tt, &cell, &self.recv)?;
            serial::install(scope, &tt, &cell)?;
            xfer::install(scope, &tt, &cell)?;
            dlg::install(scope, &tt, &cell)?;
            log::install(scope, &tt, &cell)?;
            term::install(scope, &tt, &cell)?;
            env::install(scope, &tt, &cell)?;
            tt.set("args", self.lua.create_table()?)?;
            tt.set("name", self.lua.create_string(&self.name)?)?;
            seal(&self.lua, &tt)?;

            let old_print: Value = self.lua.globals().get("print")?;
            self.lua.globals().set("tt", &tt)?;
            install_print(&self.lua, scope, &cell)?;
            install_cancel_hook(&self.lua, scope, &cell)?;

            let function: Function = self.lua.registry_value(key)?;
            let result = match argument {
                Some(value) => function.call::<()>(value),
                None => function.call::<()>(()),
            };

            // Scoped functions must stop being reachable before the scope
            // closes. The plugin's own print and the absence of `tt` are both
            // restored even when its callback failed.
            self.lua.remove_hook();
            let _ = self.lua.unset_named_registry_value(crate::CANCEL_KEY);
            self.lua.globals().set("tt", Value::Nil)?;
            self.lua.globals().set("print", old_print)?;

            match result {
                Ok(()) if cell.borrow_mut().cancelled() => Err(mlua::Error::external(Cancelled)),
                other => other,
            }
        })
    }
}

fn install_menu(
    lua: &Lua,
    sterna: &Table,
    declarations: std::sync::Arc<std::sync::Mutex<Declarations>>,
) -> mlua::Result<()> {
    sterna.set(
        "menu",
        lua.create_function(move |lua, spec: Table| {
            let menu: String = required(&spec, "menu")?;
            let label: String = required(&spec, "label")?;
            let shortcut: Option<String> = spec.get("shortcut")?;
            let action: Function = required(&spec, "action")?;
            if menu.trim_matches('/').is_empty() {
                return Err(mlua::Error::runtime("menu path is empty"));
            }
            if label.trim().is_empty() {
                return Err(mlua::Error::runtime("menu label is empty"));
            }
            let mut declarations = declarations.lock().expect("plugin declarations poisoned");
            let callback = declarations.remember(lua, action)?;
            declarations.menus.push(MenuItem {
                menu,
                label,
                shortcut,
                callback,
            });
            Ok(callback.0 + 1)
        })?,
    )
}

fn install_key(
    lua: &Lua,
    sterna: &Table,
    declarations: std::sync::Arc<std::sync::Mutex<Declarations>>,
) -> mlua::Result<()> {
    sterna.set(
        "key",
        lua.create_function(move |lua, (sequence, action): (String, Function)| {
            if sequence.trim().is_empty() {
                return Err(mlua::Error::runtime("key sequence is empty"));
            }
            let mut declarations = declarations.lock().expect("plugin declarations poisoned");
            let callback = declarations.remember(lua, action)?;
            declarations.keys.push(KeyBinding { sequence, callback });
            Ok(callback.0 + 1)
        })?,
    )
}

fn install_hook(
    lua: &Lua,
    sterna: &Table,
    declarations: std::sync::Arc<std::sync::Mutex<Declarations>>,
) -> mlua::Result<()> {
    sterna.set(
        "on",
        lua.create_function(move |lua, (name, action): (String, Function)| {
            let hook = Hook::parse(&name)?;
            let mut declarations = declarations.lock().expect("plugin declarations poisoned");
            let callback = declarations.remember(lua, action)?;
            declarations.hooks.push(HookItem { hook, callback });
            Ok(callback.0 + 1)
        })?,
    )
}

fn required<T: mlua::FromLua>(table: &Table, name: &str) -> mlua::Result<T> {
    table
        .get::<Option<T>>(name)?
        .ok_or_else(|| mlua::Error::runtime(format!("menu field '{name}' is required")))
}

fn seal_sterna(lua: &Lua, sterna: &Table) -> mlua::Result<()> {
    let meta = lua.create_table()?;
    meta.set(
        "__index",
        lua.create_function(|_, (_, key): (Table, String)| -> mlua::Result<Value> {
            Err(mlua::Error::runtime(format!("sterna.{key} does not exist")))
        })?,
    )?;
    sterna.set_metatable(Some(meta))?;
    Ok(())
}

fn reach_neighbours(lua: &Lua, name: &str) -> mlua::Result<()> {
    let Some(dir) = Path::new(name).parent() else {
        return Ok(());
    };
    if dir.as_os_str().is_empty() {
        return Ok(());
    }
    let package: Table = lua.globals().get("package")?;
    let existing: String = package.get("path")?;
    let dir = dir.display();
    package.set("path", format!("{dir}/?.lua;{dir}/?/init.lua;{existing}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tt_ttl::RecordingHost;

    fn load(source: &str) -> Plugin {
        Plugin::load("counter.lua", source.as_bytes().to_vec()).unwrap()
    }

    #[test]
    fn declares_menus_keys_and_hooks() {
        let plugin = load(
            r#"
            sterna.menu {
              menu = 'Control/Examples', label = 'Reconnect',
              shortcut = 'Ctrl+Alt+R', action = function() end,
            }
            sterna.key('Ctrl+Alt+K', function() end)
            sterna.on('connect', function() end)
            "#,
        );
        assert_eq!(
            plugin.menus(),
            &[MenuItem {
                menu: "Control/Examples".into(),
                label: "Reconnect".into(),
                shortcut: Some("Ctrl+Alt+R".into()),
                callback: CallbackId(0),
            }]
        );
        assert_eq!(
            plugin.keys(),
            &[KeyBinding {
                sequence: "Ctrl+Alt+K".into(),
                callback: CallbackId(1),
            }]
        );
        assert!(plugin.has_hook(Hook::Connect));
        assert!(!plugin.has_hook(Hook::Disconnect));
    }

    #[test]
    fn a_callback_reaches_the_terminal_and_keeps_its_state() {
        let plugin = load(
            r#"
            local count = 0
            sterna.menu { menu = 'Control', label = 'Count', action = function()
              count = count + 1
              tt.sendln(tostring(count))
            end }
            "#,
        );
        let mut host = RecordingHost::new();
        host.linked = true;
        plugin
            .invoke(plugin.menus()[0].callback, &mut host)
            .unwrap();
        plugin
            .invoke(plugin.menus()[0].callback, &mut host)
            .unwrap();
        assert_eq!(host.sent, b"1\r2\r");
    }

    #[test]
    fn lifecycle_hooks_run_in_declaration_order() {
        let plugin = load(
            r#"
            sterna.on('connect', function(kind) tt.send(kind, ':one') end)
            sterna.on('disconnect', function(kind) tt.send(kind) end)
            sterna.on('connect', function(kind) tt.send(kind, ':two') end)
            "#,
        );
        let mut host = RecordingHost::new();
        host.linked = true;
        plugin.emit(Hook::Connect, &mut host).unwrap();
        assert_eq!(host.sent, b"connect:oneconnect:two");
    }

    #[test]
    fn registration_is_closed_after_loading() {
        let plugin = load(
            r#"
            sterna.menu { menu = 'Control', label = 'Late', action = function()
              sterna.menu { menu = 'Control', label = 'Too late', action = function() end }
            end }
            "#,
        );
        let mut host = RecordingHost::new();
        host.linked = true;
        let error = plugin
            .invoke(plugin.menus()[0].callback, &mut host)
            .unwrap_err()
            .to_string();
        assert!(error.contains("sterna.menu does not exist"), "{error}");
    }

    #[test]
    fn unknown_hooks_are_rejected_at_load_time() {
        let error = Plugin::load("bad.lua", b"sterna.on('resize', function() end)".to_vec())
            .err()
            .expect("plugin should fail")
            .to_string();
        assert!(error.contains("connect, disconnect"), "{error}");
    }

    #[test]
    fn a_loaded_plugin_can_move_to_its_worker() {
        fn assert_send<T: Send>() {}
        assert_send::<Plugin>();
    }
}

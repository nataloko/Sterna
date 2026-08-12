//! Long-lived Lua plugins.
//!
//! A macro owns one chunk and ends with it. A plugin runs its top-level chunk
//! once to declare actions and hooks, then keeps the VM — and therefore its
//! globals and closures — for the window's lifetime. The terminal command
//! surface is installed only while a callback runs. That makes top-level code
//! registration rather than a hidden startup macro, and lets the frontend run
//! callbacks through the same worker/host boundary as an ordinary script.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use mlua::{Function, HookTriggers, Lua, LuaOptions, RegistryKey, StdLib, Table, Value, VmState};
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

/// Which half of the terminal byte stream a fast-path filter sees.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamDirection {
    Input,
    Output,
}

impl StreamDirection {
    fn parse(name: &str) -> mlua::Result<Self> {
        match name {
            "input" => Ok(Self::Input),
            "output" => Ok(Self::Output),
            _ => Err(mlua::Error::runtime(format!(
                "filter direction '{name}' is not one of: input, output"
            ))),
        }
    }
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

#[derive(Clone, Debug)]
enum ControlValue {
    Boolean(bool),
    Integer(i64),
    Number(f64),
    String(Vec<u8>),
}

impl ControlValue {
    fn from_lua(value: Value) -> mlua::Result<Option<Self>> {
        match value {
            Value::Nil => Ok(None),
            Value::Boolean(value) => Ok(Some(Self::Boolean(value))),
            Value::Integer(value) => Ok(Some(Self::Integer(value))),
            Value::Number(value) => Ok(Some(Self::Number(value))),
            Value::String(value) => Ok(Some(Self::String(value.as_bytes().to_vec()))),
            other => Err(mlua::Error::runtime(format!(
                "filter control values must be nil, boolean, number, or string, not {}",
                other.type_name()
            ))),
        }
    }

    fn to_lua(&self, lua: &Lua) -> mlua::Result<Value> {
        Ok(match self {
            Self::Boolean(value) => Value::Boolean(*value),
            Self::Integer(value) => Value::Integer(*value),
            Self::Number(value) => Value::Number(*value),
            Self::String(value) => Value::String(lua.create_string(value)?),
        })
    }
}

/// The scalar state shared by an action VM and its isolated stream VM.
///
/// A filter cannot run in the ordinary callback VM: that VM may be blocked in
/// `tt.wait` or a dialog while bytes still have to cross the terminal. The
/// proxy returned by `sterna.filter` is the deliberately small bridge between
/// the two. Scalars make a lock cheap and make it impossible to smuggle a Lua
/// object from one state into another.
#[derive(Clone, Debug)]
struct FilterControl {
    direction: StreamDirection,
    values: Arc<Mutex<HashMap<String, ControlValue>>>,
}

impl FilterControl {
    fn new(direction: StreamDirection) -> Self {
        let mut values = HashMap::new();
        values.insert("enabled".into(), ControlValue::Boolean(true));
        Self {
            direction,
            values: Arc::new(Mutex::new(values)),
        }
    }

    fn enabled(&self) -> bool {
        matches!(
            self.values
                .lock()
                .expect("filter control poisoned")
                .get("enabled"),
            Some(ControlValue::Boolean(true))
        )
    }

    fn disable(&self) {
        self.values
            .lock()
            .expect("filter control poisoned")
            .insert("enabled".into(), ControlValue::Boolean(false));
    }
}

struct FilterItem {
    direction: StreamDirection,
    callback: RegistryKey,
    control: FilterControl,
}

#[derive(Default)]
struct Declarations {
    callbacks: Vec<RegistryKey>,
    menus: Vec<MenuItem>,
    keys: Vec<KeyBinding>,
    hooks: Vec<HookItem>,
    filters: Vec<FilterItem>,
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
    filters: Vec<FilterControl>,
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
        let (lua, mut declarations) = load_declarations(&name, &body, None)?;
        let filters = declarations
            .filters
            .drain(..)
            .map(|filter| {
                // The executable copy belongs to the isolated stream state.
                // Retaining this one would only keep unreachable closures
                // alive in the ordinary callback VM.
                lua.remove_registry_value(filter.callback)?;
                Ok(filter.control)
            })
            .collect::<mlua::Result<Vec<_>>>()?;
        Ok(Self {
            name,
            lua,
            callbacks: std::mem::take(&mut declarations.callbacks),
            menus: std::mem::take(&mut declarations.menus),
            keys: std::mem::take(&mut declarations.keys),
            hooks: std::mem::take(&mut declarations.hooks),
            filters,
            recv: RefCell::new(Recv::default()),
        })
    }

    /// Build the isolated, non-blocking copy of this plugin's filters.
    ///
    /// Lua functions and their upvalues cannot cross states, so the top level
    /// runs a second time. Writes to the shared control proxies are suppressed
    /// during that declaration pass: defaults from the real load are not
    /// applied twice, but filter callbacks can update the controls afterwards.
    pub fn load_stream(&self, body: &[u8]) -> mlua::Result<Option<StreamPlugin>> {
        if self.filters.is_empty() {
            return Ok(None);
        }
        let (lua, mut declarations) =
            load_declarations(&self.name, body, Some(self.filters.clone()))?;
        for callback in declarations.callbacks.drain(..) {
            lua.remove_registry_value(callback)?;
        }
        Ok(Some(StreamPlugin {
            name: self.name.clone(),
            lua,
            filters: declarations.filters,
        }))
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

    pub fn has_filter(&self, direction: StreamDirection) -> bool {
        self.filters
            .iter()
            .any(|filter| filter.direction == direction)
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

const FILTER_HOOK_EVERY: u32 = 1_000;
const FILTER_INSTRUCTIONS: usize = 100_000;
const FILTER_MAX_OUTPUT: usize = 1024 * 1024;

/// The result of one pass through all filters for a direction.
///
/// A bad filter is disabled and its input is passed on. Errors are returned
/// beside the usable bytes so a terminal can report the problem without
/// dropping a connection or losing the data which exposed it.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct StreamFilterResult {
    pub bytes: Vec<u8>,
    pub errors: Vec<String>,
}

/// One plugin's filter callbacks in the isolated hot-path Lua state.
pub struct StreamPlugin {
    name: String,
    lua: Lua,
    filters: Vec<FilterItem>,
}

impl StreamPlugin {
    fn filter(&self, direction: StreamDirection, bytes: &[u8]) -> StreamFilterResult {
        let mut current = bytes.to_vec();
        let mut errors = Vec::new();
        for item in &self.filters {
            if item.direction != direction || !item.control.enabled() {
                continue;
            }
            match self.call(item, &current) {
                Ok(Some(filtered)) => current = filtered,
                Ok(None) => current.clear(),
                Err(error) => {
                    item.control.disable();
                    errors.push(format!("{}: {error}", self.name));
                }
            }
        }
        StreamFilterResult {
            bytes: current,
            errors,
        }
    }

    fn call(&self, item: &FilterItem, bytes: &[u8]) -> mlua::Result<Option<Vec<u8>>> {
        let remaining = Arc::new(AtomicUsize::new(FILTER_INSTRUCTIONS));
        let probe = remaining.clone();
        self.lua.set_hook(
            HookTriggers::new().every_nth_instruction(FILTER_HOOK_EVERY),
            move |_, _| {
                if probe.fetch_sub(FILTER_HOOK_EVERY as usize, Ordering::Relaxed)
                    <= FILTER_HOOK_EVERY as usize
                {
                    return Err(mlua::Error::runtime(
                        "stream filter exceeded its instruction limit",
                    ));
                }
                Ok(VmState::Continue)
            },
        )?;

        let result = (|| {
            let function: Function = self.lua.registry_value(&item.callback)?;
            let value = self.lua.create_string(bytes)?;
            let output: Option<mlua::LuaString> = function.call(value)?;
            let output = output.map(|value| value.as_bytes().to_vec());
            if output
                .as_ref()
                .is_some_and(|value| value.len() > FILTER_MAX_OUTPUT)
            {
                return Err(mlua::Error::runtime(format!(
                    "stream filter returned more than {FILTER_MAX_OUTPUT} bytes"
                )));
            }
            Ok(output)
        })();
        self.lua.remove_hook();
        result
    }
}

/// Every stream plugin for one terminal tab, in filename order.
#[derive(Default)]
pub struct StreamFilters {
    plugins: Vec<StreamPlugin>,
}

impl StreamFilters {
    pub fn new(plugins: Vec<StreamPlugin>) -> Self {
        Self { plugins }
    }

    pub fn filter(&self, direction: StreamDirection, bytes: &[u8]) -> StreamFilterResult {
        let mut result = StreamFilterResult {
            bytes: bytes.to_vec(),
            errors: Vec::new(),
        };
        for plugin in &self.plugins {
            let next = plugin.filter(direction, &result.bytes);
            result.bytes = next.bytes;
            result.errors.extend(next.errors);
        }
        result
    }

    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }
}

/// Execute one plugin's load-time declarations.
///
/// `controls` is `None` for the ordinary callback VM and is the first pass's
/// filter list for the isolated stream VM. The latter must declare the same
/// filters in the same order; a top level whose declarations depend on time or
/// other external state is rejected instead of connecting the wrong control.
fn load_declarations(
    name: &str,
    body: &[u8],
    controls: Option<Vec<FilterControl>>,
) -> mlua::Result<(Lua, Declarations)> {
    let lua = Lua::new_with(StdLib::ALL_SAFE, LuaOptions::default())?;
    let declarations = Arc::new(Mutex::new(Declarations::default()));
    let sterna = lua.create_table()?;
    let control_writes = Arc::new(AtomicBool::new(controls.is_none()));

    install_menu(&lua, &sterna, declarations.clone())?;
    install_key(&lua, &sterna, declarations.clone())?;
    install_hook(&lua, &sterna, declarations.clone())?;
    install_filter(
        &lua,
        &sterna,
        declarations.clone(),
        controls.clone(),
        control_writes.clone(),
    )?;
    seal_sterna(&lua, &sterna)?;
    lua.globals().set("sterna", &sterna)?;

    // A plugin is part of a window process, just as a macro is. Letting a
    // top-level typo call `os.exit` would close every tab in it.
    let os: Table = lua.globals().get("os")?;
    os.set("exit", Value::Nil)?;
    reach_neighbours(&lua, name)?;

    let result = lua.load(body).set_name(format!("@{name}")).exec();

    // These functions are load-time declarations, not a mutable runtime API.
    sterna.raw_set("menu", Value::Nil)?;
    sterna.raw_set("key", Value::Nil)?;
    sterna.raw_set("on", Value::Nil)?;
    sterna.raw_set("filter", Value::Nil)?;
    result?;

    let mut declarations = declarations.lock().expect("plugin declarations poisoned");
    if let Some(controls) = controls {
        if controls.len() != declarations.filters.len() {
            return Err(mlua::Error::runtime(
                "plugin declared a different filter list on its stream load",
            ));
        }
        control_writes.store(true, Ordering::Release);
    }
    let declarations = std::mem::take(&mut *declarations);
    Ok((lua, declarations))
}

fn install_filter(
    lua: &Lua,
    sterna: &Table,
    declarations: Arc<Mutex<Declarations>>,
    controls: Option<Vec<FilterControl>>,
    control_writes: Arc<AtomicBool>,
) -> mlua::Result<()> {
    sterna.set(
        "filter",
        lua.create_function(move |lua, (name, action): (String, Function)| {
            let direction = StreamDirection::parse(&name)?;
            let mut declarations = declarations.lock().expect("plugin declarations poisoned");
            let index = declarations.filters.len();
            let control = match controls.as_ref().and_then(|controls| controls.get(index)) {
                Some(control) if control.direction == direction => control.clone(),
                Some(_) => {
                    return Err(mlua::Error::runtime(
                        "plugin changed a filter direction on its stream load",
                    ));
                }
                None if controls.is_some() => {
                    return Err(mlua::Error::runtime(
                        "plugin declared an extra filter on its stream load",
                    ));
                }
                None => FilterControl::new(direction),
            };
            let callback = lua.create_registry_value(action)?;
            declarations.filters.push(FilterItem {
                direction,
                callback,
                control: control.clone(),
            });
            filter_control_table(lua, control, control_writes.clone())
        })?,
    )
}

fn filter_control_table(
    lua: &Lua,
    control: FilterControl,
    writes: Arc<AtomicBool>,
) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let meta = lua.create_table()?;
    let read = control.clone();
    meta.set(
        "__index",
        lua.create_function(move |lua, (_, key): (Table, String)| {
            if key == "direction" {
                return Ok(Value::String(lua.create_string(match read.direction {
                    StreamDirection::Input => "input",
                    StreamDirection::Output => "output",
                })?));
            }
            match read
                .values
                .lock()
                .expect("filter control poisoned")
                .get(&key)
            {
                Some(value) => value.to_lua(lua),
                None => Ok(Value::Nil),
            }
        })?,
    )?;
    meta.set(
        "__newindex",
        lua.create_function(move |_, (_, key, value): (Table, String, Value)| {
            if key == "direction" {
                return Err(mlua::Error::runtime("filter direction is read-only"));
            }
            let value = ControlValue::from_lua(value)?;
            if key == "enabled" && !matches!(value, Some(ControlValue::Boolean(_))) {
                return Err(mlua::Error::runtime("filter enabled must be a boolean"));
            }
            if !writes.load(Ordering::Acquire) {
                return Ok(());
            }
            let mut values = control.values.lock().expect("filter control poisoned");
            match value {
                Some(value) => {
                    values.insert(key, value);
                }
                None => {
                    values.remove(&key);
                }
            }
            Ok(())
        })?,
    )?;
    table.set_metatable(Some(meta))?;
    Ok(table)
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
    fn stream_filters_are_binary_safe_and_keep_fast_path_state() {
        let source = br#"
            local held = ''
            sterna.filter('input', function(bytes)
              held = held .. bytes
              if #held < 3 then return nil end
              local out = held
              held = ''
              return string.upper(out)
            end)
            sterna.filter('output', function(bytes)
              return string.reverse(bytes)
            end)
        "#;
        let plugin = Plugin::load("filters.lua", source.to_vec()).unwrap();
        assert!(plugin.has_filter(StreamDirection::Input));
        assert!(plugin.has_filter(StreamDirection::Output));
        let stream = plugin.load_stream(source).unwrap().unwrap();

        let first = stream.filter(StreamDirection::Input, &[0, b'a']);
        assert_eq!(first.bytes, b"");
        assert!(first.errors.is_empty());
        assert_eq!(
            stream.filter(StreamDirection::Input, b"b").bytes,
            vec![0, b'A', b'B']
        );
        assert_eq!(
            stream.filter(StreamDirection::Output, &[0, 1, 2]).bytes,
            vec![2, 1, 0]
        );
    }

    #[test]
    fn actions_can_reconfigure_a_filter_without_sharing_a_lua_state() {
        let source = br#"
            local input
            input = sterna.filter('input', function(bytes)
              return input.prefix .. bytes
            end)
            input.prefix = 'before:'
            sterna.menu { menu = 'Control', label = 'Change filter', action = function()
              input.prefix = 'after:'
            end }
        "#;
        let plugin = Plugin::load("controlled.lua", source.to_vec()).unwrap();
        let stream = plugin.load_stream(source).unwrap().unwrap();
        assert_eq!(
            stream.filter(StreamDirection::Input, b"x").bytes,
            b"before:x"
        );

        let mut host = RecordingHost::new();
        host.linked = true;
        plugin
            .invoke(plugin.menus()[0].callback, &mut host)
            .unwrap();
        assert_eq!(
            stream.filter(StreamDirection::Input, b"x").bytes,
            b"after:x"
        );
    }

    #[test]
    fn a_bad_filter_fails_open_and_disables_itself() {
        let source = br#"
            sterna.filter('input', function(bytes)
              error('broken on purpose')
            end)
        "#;
        let plugin = Plugin::load("broken.lua", source.to_vec()).unwrap();
        let stream = plugin.load_stream(source).unwrap().unwrap();

        let first = stream.filter(StreamDirection::Input, b"kept");
        assert_eq!(first.bytes, b"kept");
        assert_eq!(first.errors.len(), 1);
        assert!(first.errors[0].contains("broken on purpose"));
        let second = stream.filter(StreamDirection::Input, b"still kept");
        assert_eq!(second.bytes, b"still kept");
        assert!(second.errors.is_empty());
    }

    #[test]
    fn invalid_filter_declarations_fail_during_load() {
        let direction = Plugin::load(
            "bad.lua",
            b"sterna.filter('sideways', function(bytes) return bytes end)".to_vec(),
        )
        .err()
        .expect("direction should fail")
        .to_string();
        assert!(direction.contains("input, output"), "{direction}");

        let control = Plugin::load(
            "bad.lua",
            br#"
                local f = sterna.filter('input', function(bytes) return bytes end)
                f.enabled = 'yes'
            "#
            .to_vec(),
        )
        .err()
        .expect("control type should fail")
        .to_string();
        assert!(control.contains("enabled must be a boolean"), "{control}");
    }

    #[test]
    fn a_loaded_plugin_can_move_to_its_worker() {
        fn assert_send<T: Send>() {}
        assert_send::<Plugin>();
        assert_send::<StreamFilters>();
    }
}

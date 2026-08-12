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

/// Values read from a plugin's sections of `sterna.ini` before its top level
/// runs.
///
/// The INI layer owns its deliberately Win32-compatible parsing. This small
/// map keeps `tt-lua` independent of that crate while still letting a plugin
/// see persisted values during declaration rather than one callback later.
#[derive(Clone, Debug, Default)]
pub struct StoredSettings {
    values: HashMap<(String, String), String>,
}

impl StoredSettings {
    pub fn insert(&mut self, section: &str, key: &str, value: impl Into<String>) {
        self.values
            .entry(setting_address(section, key))
            .or_insert_with(|| value.into());
    }

    fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.values
            .get(&setting_address(section, key))
            .map(String::as_str)
    }
}

fn setting_address(section: &str, key: &str) -> (String, String) {
    (
        section.trim().to_ascii_lowercase(),
        key.trim().to_ascii_lowercase(),
    )
}

/// The four controls a portable plugin settings page can draw.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SettingKind {
    Bool,
    Integer { min: i32, max: i32 },
    String,
    Enum(Vec<String>),
}

/// One field declared on a plugin settings page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingField {
    /// The property on the Lua control proxy.
    pub name: String,
    /// The key written in the page's INI section.
    pub key: String,
    pub label: String,
    pub description: String,
    pub kind: SettingKind,
    default: SettingValue,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum SettingValue {
    Bool(bool),
    Integer(i32),
    String(String),
}

impl SettingValue {
    fn spelling(&self) -> String {
        match self {
            Self::Bool(true) => "on".into(),
            Self::Bool(false) => "off".into(),
            Self::Integer(value) => value.to_string(),
            Self::String(value) => value.clone(),
        }
    }

    fn to_lua(&self, lua: &Lua) -> mlua::Result<Value> {
        Ok(match self {
            Self::Bool(value) => Value::Boolean(*value),
            Self::Integer(value) => Value::Integer(i64::from(*value)),
            Self::String(value) => Value::String(lua.create_string(value)?),
        })
    }
}

/// A settings page and its live values.
///
/// Clones share the value store. The callback VM, isolated stream VM, C ABI
/// descriptor and Qt dialog can therefore all retain this cheap handle without
/// moving a Lua object between threads or states.
#[derive(Clone, Debug)]
pub struct SettingPage {
    pub title: String,
    pub section: String,
    pub fields: Vec<SettingField>,
    values: Arc<Mutex<Vec<SettingValue>>>,
}

impl SettingPage {
    pub fn value(&self, index: usize) -> Option<String> {
        self.values
            .lock()
            .expect("plugin settings poisoned")
            .get(index)
            .map(SettingValue::spelling)
    }

    pub fn default_value(&self, index: usize) -> Option<String> {
        self.fields.get(index).map(|field| field.default.spelling())
    }

    pub fn set(&self, index: usize, value: &str) -> Result<(), String> {
        let field = self
            .fields
            .get(index)
            .ok_or_else(|| "plugin setting index out of range".to_string())?;
        let value = field.parse_text(value)?;
        self.values.lock().expect("plugin settings poisoned")[index] = value;
        Ok(())
    }

    fn same_declaration(&self, other: &Self) -> bool {
        self.title == other.title && self.section == other.section && self.fields == other.fields
    }
}

impl SettingField {
    fn parse_stored(&self, value: &str) -> SettingValue {
        self.parse_text(value)
            .unwrap_or_else(|_| self.default.clone())
    }

    fn parse_text(&self, value: &str) -> Result<SettingValue, String> {
        match &self.kind {
            SettingKind::Bool => match value.to_ascii_lowercase().as_str() {
                "on" => Ok(SettingValue::Bool(true)),
                "off" => Ok(SettingValue::Bool(false)),
                _ => Err("expected on or off".into()),
            },
            SettingKind::Integer { min, max } => {
                let value = value
                    .trim()
                    .parse::<i32>()
                    .map_err(|_| "expected an integer".to_string())?;
                if value < *min || value > *max {
                    return Err(format!("expected an integer from {min} to {max}"));
                }
                Ok(SettingValue::Integer(value))
            }
            SettingKind::String => {
                if value.contains('\r') || value.contains('\n') {
                    return Err("a plugin setting cannot contain a line break".into());
                }
                Ok(SettingValue::String(value.to_string()))
            }
            SettingKind::Enum(choices) => {
                if choices.iter().any(|choice| choice == value) {
                    Ok(SettingValue::String(value.to_string()))
                } else {
                    Err(format!("expected one of: {}", choices.join(", ")))
                }
            }
        }
    }
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
    settings: Vec<SettingPage>,
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
    settings: Vec<SettingPage>,
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
        Self::load_with_settings(name, body, StoredSettings::default())
    }

    /// Load one plugin with values read from its declared INI sections.
    ///
    /// They are installed as each page is declared, so later top-level code
    /// sees the persisted value rather than briefly seeing the default.
    pub fn load_with_settings(
        name: impl Into<String>,
        body: Vec<u8>,
        stored: StoredSettings,
    ) -> mlua::Result<Self> {
        let name = name.into();
        let (lua, mut declarations) =
            load_declarations(&name, &body, None, None, Arc::new(stored))?;
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
            settings: std::mem::take(&mut declarations.settings),
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
        let (lua, mut declarations) = load_declarations(
            &self.name,
            body,
            Some(self.filters.clone()),
            Some(self.settings.clone()),
            Arc::new(StoredSettings::default()),
        )?;
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

    pub fn settings(&self) -> &[SettingPage] {
        &self.settings
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
    setting_pages: Option<Vec<SettingPage>>,
    stored: Arc<StoredSettings>,
) -> mlua::Result<(Lua, Declarations)> {
    let lua = Lua::new_with(StdLib::ALL_SAFE, LuaOptions::default())?;
    let declarations = Arc::new(Mutex::new(Declarations::default()));
    let sterna = lua.create_table()?;
    let control_writes = Arc::new(AtomicBool::new(controls.is_none()));

    install_menu(&lua, &sterna, declarations.clone())?;
    install_key(&lua, &sterna, declarations.clone())?;
    install_hook(&lua, &sterna, declarations.clone())?;
    install_settings(
        &lua,
        &sterna,
        declarations.clone(),
        setting_pages.clone(),
        stored,
        control_writes.clone(),
    )?;
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
    sterna.raw_set("settings", Value::Nil)?;
    sterna.raw_set("filter", Value::Nil)?;
    result?;

    let mut declarations = declarations.lock().expect("plugin declarations poisoned");
    if let Some(controls) = controls {
        if controls.len() != declarations.filters.len() {
            return Err(mlua::Error::runtime(
                "plugin declared a different filter list on its stream load",
            ));
        }
    }
    if let Some(setting_pages) = setting_pages {
        if setting_pages.len() != declarations.settings.len() {
            return Err(mlua::Error::runtime(
                "plugin declared a different settings page list on its stream load",
            ));
        }
    }
    control_writes.store(true, Ordering::Release);
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

fn install_settings(
    lua: &Lua,
    sterna: &Table,
    declarations: Arc<Mutex<Declarations>>,
    existing: Option<Vec<SettingPage>>,
    stored: Arc<StoredSettings>,
    writes: Arc<AtomicBool>,
) -> mlua::Result<()> {
    sterna.set(
        "settings",
        lua.create_function(move |lua, spec: Table| {
            let title: String = spec
                .get::<Option<String>>("title")?
                .ok_or_else(|| mlua::Error::runtime("settings field 'title' is required"))?;
            let section: String = spec
                .get::<Option<String>>("section")?
                .ok_or_else(|| mlua::Error::runtime("settings field 'section' is required"))?;
            let field_specs: Table = spec
                .get::<Option<Table>>("fields")?
                .ok_or_else(|| mlua::Error::runtime("settings field 'fields' is required"))?;

            if title.trim().is_empty() {
                return Err(mlua::Error::runtime("settings title is empty"));
            }
            validate_ini_section(&section)?;

            let mut fields = Vec::with_capacity(field_specs.raw_len());
            for index in 1..=field_specs.raw_len() {
                let field: Table = field_specs.get(index)?;
                fields.push(parse_setting_field(&field)?);
            }
            if fields.is_empty() {
                return Err(mlua::Error::runtime("settings page has no fields"));
            }

            let mut names = HashMap::<String, ()>::new();
            let mut keys = HashMap::<String, ()>::new();
            for field in &fields {
                if names.insert(field.name.clone(), ()).is_some() {
                    return Err(mlua::Error::runtime(format!(
                        "settings property '{}' is declared twice",
                        field.name
                    )));
                }
                let key = field.key.to_ascii_lowercase();
                if keys.insert(key, ()).is_some() {
                    return Err(mlua::Error::runtime(format!(
                        "settings key '{}' is declared twice",
                        field.key
                    )));
                }
            }

            let defaults = fields
                .iter()
                .map(|field| {
                    stored
                        .get(&section, &field.key)
                        .map_or_else(|| field.default.clone(), |value| field.parse_stored(value))
                })
                .collect();
            let proposed = SettingPage {
                title,
                section,
                fields,
                values: Arc::new(Mutex::new(defaults)),
            };

            let mut declarations = declarations.lock().expect("plugin declarations poisoned");
            let page_index = declarations.settings.len();
            let page = match existing.as_ref().and_then(|pages| pages.get(page_index)) {
                Some(page) if page.same_declaration(&proposed) => page.clone(),
                Some(_) => {
                    return Err(mlua::Error::runtime(
                        "plugin changed a settings page on its stream load",
                    ));
                }
                None if existing.is_some() => {
                    return Err(mlua::Error::runtime(
                        "plugin declared an extra settings page on its stream load",
                    ));
                }
                None => proposed,
            };

            for declared in &declarations.settings {
                for field in &declared.fields {
                    for new_field in &page.fields {
                        if setting_address(&declared.section, &field.key)
                            == setting_address(&page.section, &new_field.key)
                        {
                            return Err(mlua::Error::runtime(format!(
                                "settings address [{}] {} is declared twice",
                                page.section, new_field.key
                            )));
                        }
                    }
                }
            }
            declarations.settings.push(page.clone());
            drop(declarations);
            setting_control_table(lua, page, writes.clone())
        })?,
    )
}

fn validate_ini_section(section: &str) -> mlua::Result<()> {
    if section.trim().is_empty()
        || section.contains(']')
        || section.contains('\r')
        || section.contains('\n')
    {
        return Err(mlua::Error::runtime("settings INI section is invalid"));
    }
    Ok(())
}

fn validate_ini_key(key: &str) -> mlua::Result<()> {
    if key.trim().is_empty() || key.contains('=') || key.contains('\r') || key.contains('\n') {
        return Err(mlua::Error::runtime("settings INI key is invalid"));
    }
    Ok(())
}

fn parse_setting_field(spec: &Table) -> mlua::Result<SettingField> {
    let name: String = spec
        .get::<Option<String>>("name")?
        .ok_or_else(|| mlua::Error::runtime("settings field 'name' is required"))?;
    let key: String = spec
        .get::<Option<String>>("key")?
        .unwrap_or_else(|| name.clone());
    let label: String = spec
        .get::<Option<String>>("label")?
        .ok_or_else(|| mlua::Error::runtime("settings field 'label' is required"))?;
    let description: String = spec
        .get::<Option<String>>("description")?
        .unwrap_or_default();
    let kind: String = spec
        .get::<Option<String>>("kind")?
        .ok_or_else(|| mlua::Error::runtime("settings field 'kind' is required"))?;
    let default: Value = spec.get("default")?;

    if name.trim().is_empty() {
        return Err(mlua::Error::runtime("settings property name is empty"));
    }
    validate_ini_key(&key)?;
    if label.trim().is_empty() {
        return Err(mlua::Error::runtime("settings label is empty"));
    }

    let (kind, default) = match kind.as_str() {
        "bool" => {
            let Value::Boolean(default) = default else {
                return Err(mlua::Error::runtime(
                    "a bool setting's default must be a boolean",
                ));
            };
            (SettingKind::Bool, SettingValue::Bool(default))
        }
        "int" => {
            let min: i32 = spec
                .get::<Option<i32>>("min")?
                .ok_or_else(|| mlua::Error::runtime("an int setting requires min"))?;
            let max: i32 = spec
                .get::<Option<i32>>("max")?
                .ok_or_else(|| mlua::Error::runtime("an int setting requires max"))?;
            if min > max {
                return Err(mlua::Error::runtime(
                    "an int setting's min is greater than its max",
                ));
            }
            let Value::Integer(default) = default else {
                return Err(mlua::Error::runtime(
                    "an int setting's default must be an integer",
                ));
            };
            let default = i32::try_from(default)
                .map_err(|_| mlua::Error::runtime("an int setting's default is out of range"))?;
            if default < min || default > max {
                return Err(mlua::Error::runtime(
                    "an int setting's default is outside its bounds",
                ));
            }
            (
                SettingKind::Integer { min, max },
                SettingValue::Integer(default),
            )
        }
        "string" => {
            let Value::String(default) = default else {
                return Err(mlua::Error::runtime(
                    "a string setting's default must be a string",
                ));
            };
            let default = default
                .to_str()
                .map_err(|_| mlua::Error::runtime("a string setting must be UTF-8"))?
                .to_string();
            if default.contains('\r') || default.contains('\n') {
                return Err(mlua::Error::runtime(
                    "a string setting's default cannot contain a line break",
                ));
            }
            (SettingKind::String, SettingValue::String(default))
        }
        "enum" => {
            let choices: Table = spec
                .get::<Option<Table>>("choices")?
                .ok_or_else(|| mlua::Error::runtime("an enum setting requires choices"))?;
            let mut values = Vec::with_capacity(choices.raw_len());
            for index in 1..=choices.raw_len() {
                let choice: String = choices.get(index)?;
                if choice.contains('\r') || choice.contains('\n') {
                    return Err(mlua::Error::runtime(
                        "an enum choice cannot contain a line break",
                    ));
                }
                if values.contains(&choice) {
                    return Err(mlua::Error::runtime(format!(
                        "enum choice '{choice}' is declared twice"
                    )));
                }
                values.push(choice);
            }
            if values.is_empty() {
                return Err(mlua::Error::runtime("an enum setting has no choices"));
            }
            let Value::String(default) = default else {
                return Err(mlua::Error::runtime(
                    "an enum setting's default must be a string",
                ));
            };
            let default = default
                .to_str()
                .map_err(|_| mlua::Error::runtime("an enum setting must be UTF-8"))?
                .to_string();
            if !values.contains(&default) {
                return Err(mlua::Error::runtime(
                    "an enum setting's default is not one of its choices",
                ));
            }
            (SettingKind::Enum(values), SettingValue::String(default))
        }
        _ => {
            return Err(mlua::Error::runtime(format!(
                "settings kind '{kind}' is not one of: bool, int, string, enum"
            )));
        }
    };

    Ok(SettingField {
        name,
        key,
        label,
        description,
        kind,
        default,
    })
}

fn setting_control_table(
    lua: &Lua,
    page: SettingPage,
    writes: Arc<AtomicBool>,
) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let meta = lua.create_table()?;
    let read = page.clone();
    meta.set(
        "__index",
        lua.create_function(move |lua, (_, key): (Table, String)| {
            let index = read
                .fields
                .iter()
                .position(|field| field.name == key)
                .ok_or_else(|| {
                    mlua::Error::runtime(format!("plugin setting '{key}' does not exist"))
                })?;
            read.values.lock().expect("plugin settings poisoned")[index].to_lua(lua)
        })?,
    )?;
    meta.set(
        "__newindex",
        lua.create_function(move |_, (_, key, value): (Table, String, Value)| {
            let index = page
                .fields
                .iter()
                .position(|field| field.name == key)
                .ok_or_else(|| {
                    mlua::Error::runtime(format!("plugin setting '{key}' does not exist"))
                })?;
            let field = &page.fields[index];
            let value = setting_value_from_lua(field, value)?;
            if writes.load(Ordering::Acquire) {
                page.values.lock().expect("plugin settings poisoned")[index] = value;
            }
            Ok(())
        })?,
    )?;
    table.set_metatable(Some(meta))?;
    Ok(table)
}

fn setting_value_from_lua(field: &SettingField, value: Value) -> mlua::Result<SettingValue> {
    match (&field.kind, value) {
        (SettingKind::Bool, Value::Boolean(value)) => Ok(SettingValue::Bool(value)),
        (SettingKind::Integer { min, max }, Value::Integer(value)) => {
            let value = i32::try_from(value)
                .map_err(|_| mlua::Error::runtime("plugin setting integer is out of range"))?;
            if value < *min || value > *max {
                return Err(mlua::Error::runtime(format!(
                    "plugin setting integer must be from {min} to {max}"
                )));
            }
            Ok(SettingValue::Integer(value))
        }
        (SettingKind::String, Value::String(value)) => {
            let value = value
                .to_str()
                .map_err(|_| mlua::Error::runtime("plugin setting string must be UTF-8"))?
                .to_string();
            field.parse_text(&value).map_err(mlua::Error::runtime)
        }
        (SettingKind::Enum(_), Value::String(value)) => {
            let value = value
                .to_str()
                .map_err(|_| mlua::Error::runtime("plugin enum value must be UTF-8"))?
                .to_string();
            field.parse_text(&value).map_err(mlua::Error::runtime)
        }
        (kind, value) => Err(mlua::Error::runtime(format!(
            "plugin setting expected {}, not {}",
            match kind {
                SettingKind::Bool => "a boolean",
                SettingKind::Integer { .. } => "an integer",
                SettingKind::String | SettingKind::Enum(_) => "a string",
            },
            value.type_name()
        ))),
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
    fn settings_are_typed_and_persisted_before_the_top_level_continues() {
        let source = br#"
            local preferences = sterna.settings {
              title = 'Router tools', section = 'Lua Router Tools', fields = {
                { name = 'enabled', label = 'Enabled', kind = 'bool', default = true },
                { name = 'retries', label = 'Retries', kind = 'int',
                  min = 1, max = 10, default = 3 },
                { name = 'prefix', key = 'PromptPrefix', label = 'Prefix',
                  description = 'Text before each command', kind = 'string', default = '>' },
                { name = 'mode', label = 'Mode', kind = 'enum',
                  choices = {'fast', 'safe'}, default = 'fast' },
              }
            }
            local loaded = preferences.prefix
            sterna.menu { menu = 'Setup', label = 'Read settings', action = function()
              tt.send(loaded, '/', preferences.prefix, '/', tostring(preferences.enabled),
                      '/', tostring(preferences.retries), '/', preferences.mode)
            end }
        "#;
        let mut stored = StoredSettings::default();
        stored.insert("lua router tools", "promptprefix", "saved");
        stored.insert("Lua Router Tools", "enabled", "off");
        stored.insert("Lua Router Tools", "retries", "7");
        stored.insert("Lua Router Tools", "mode", "safe");
        let plugin = Plugin::load_with_settings("settings.lua", source.to_vec(), stored).unwrap();
        let page = &plugin.settings()[0];
        assert_eq!(page.title, "Router tools");
        assert_eq!(page.section, "Lua Router Tools");
        assert_eq!(page.value(0).as_deref(), Some("off"));
        assert_eq!(page.value(1).as_deref(), Some("7"));
        assert_eq!(page.value(2).as_deref(), Some("saved"));
        assert_eq!(page.value(3).as_deref(), Some("safe"));
        assert_eq!(page.default_value(2).as_deref(), Some(">"));
        assert!(matches!(
            page.fields[1].kind,
            SettingKind::Integer { min: 1, max: 10 }
        ));

        page.set(2, "live").unwrap();
        assert!(page.set(1, "11").unwrap_err().contains("1 to 10"));
        let mut host = RecordingHost::new();
        host.linked = true;
        plugin
            .invoke(plugin.menus()[0].callback, &mut host)
            .unwrap();
        assert_eq!(host.sent, b"saved/live/false/7/safe");
    }

    #[test]
    fn settings_are_shared_with_the_isolated_filter_vm() {
        let source = br#"
            local preferences = sterna.settings {
              title = 'Prefix', section = 'Lua Prefix', fields = {
                { name = 'prefix', label = 'Prefix', kind = 'string', default = 'before:' },
              }
            }
            sterna.filter('input', function(bytes) return preferences.prefix .. bytes end)
            sterna.menu { menu = 'Setup', label = 'Change', action = function()
              preferences.prefix = 'after:'
            end }
        "#;
        let plugin = Plugin::load("settings-filter.lua", source.to_vec()).unwrap();
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
    fn invalid_settings_declarations_fail_at_load_time() {
        let bad_default = Plugin::load(
            "bad.lua",
            br#"sterna.settings { title='Bad', section='Lua Bad', fields={
              {name='mode', label='Mode', kind='enum', choices={'one'}, default='two'}
            } }"#
                .to_vec(),
        )
        .err()
        .expect("enum default should fail")
        .to_string();
        assert!(bad_default.contains("default is not one"), "{bad_default}");

        let duplicate = Plugin::load(
            "bad.lua",
            br#"
            sterna.settings { title='One', section='Lua Bad', fields={
              {name='one', key='Same', label='One', kind='bool', default=true}
            } }
            sterna.settings { title='Two', section='lua bad', fields={
              {name='two', key='same', label='Two', kind='bool', default=true}
            } }
            "#
            .to_vec(),
        )
        .err()
        .expect("duplicate address should fail")
        .to_string();
        assert!(duplicate.contains("declared twice"), "{duplicate}");
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

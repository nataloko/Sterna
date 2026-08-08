//! The variable store, ported from `ttmparse.cpp`'s `Variables` array.
//!
//! TTL has no scopes and no declarations: a name springs into existence with a
//! type the first time it is assigned, and keeps that type for the run. Labels
//! live in the same table as variables, which is why `:foo` and a variable
//! `foo` collide with `ErrLabelAlreadyDef`.

use crate::error::{TtlError, TtlResult};

/// `TVariableType` (`ttmparse.h:74`).
///
/// The numbers are user-visible: `ifdefined` assigns one to `result`, so a
/// macro can and does compare against the literal 3 for "string".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum VarType {
    Unknown = 0,
    Integer = 1,
    /// Upstream declares it and never uses it. Kept so the numbers line up.
    Logical = 2,
    String = 3,
    Label = 4,
    IntArray = 5,
    StrArray = 6,
}

impl VarType {
    pub fn code(self) -> i32 {
        self as i32
    }
}

/// Where a value lives: a whole variable, or one element of an array.
///
/// Upstream packs both into a `DWORD` as `((index + 1) << 16) | element` and
/// tells them apart with `VarId >> 16`, which caps an array at 65536 elements
/// and the variable table at 65535 entries. Two fields cost nothing and cap
/// neither.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VarRef {
    Scalar(usize),
    Elem(usize, usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Int(i32),
    Str(Vec<u8>),
    /// A byte offset into the macro buffer, and which include level it is in.
    Label {
        pos: usize,
        level: usize,
    },
    IntArray(Vec<i32>),
    StrArray(Vec<Vec<u8>>),
}

impl Value {
    pub fn var_type(&self) -> VarType {
        match self {
            Value::Int(_) => VarType::Integer,
            Value::Str(_) => VarType::String,
            Value::Label { .. } => VarType::Label,
            Value::IntArray(_) => VarType::IntArray,
            Value::StrArray(_) => VarType::StrArray,
        }
    }
}

#[derive(Debug, Clone)]
struct Variable {
    name: Vec<u8>,
    value: Value,
}

/// Every variable and label in the run.
#[derive(Debug, Default, Clone)]
pub struct Vars {
    vars: Vec<Variable>,
    /// Lowercased name to index. Upstream scans the array with `_stricmp`;
    /// this is the same lookup, and rebuilt whenever indices shift.
    index: std::collections::HashMap<Vec<u8>, usize>,
}

impl Vars {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.vars.len()
    }

    pub fn is_empty(&self) -> bool {
        self.vars.is_empty()
    }

    /// `CheckVar` — find a name, case-insensitively.
    pub fn find(&self, name: &[u8]) -> Option<(usize, VarType)> {
        let key = name.to_ascii_lowercase();
        self.index
            .get(&key)
            .map(|&i| (i, self.vars[i].value.var_type()))
    }

    pub fn value(&self, id: usize) -> &Value {
        &self.vars[id].value
    }

    pub fn name(&self, id: usize) -> &[u8] {
        &self.vars[id].name
    }

    fn push(&mut self, name: &[u8], value: Value) -> usize {
        let id = self.vars.len();
        self.index.insert(name.to_ascii_lowercase(), id);
        self.vars.push(Variable {
            name: name.to_vec(),
            value,
        });
        id
    }

    /// `NewIntVar`. Assumes the caller has already checked the name is free,
    /// which every upstream caller does.
    pub fn new_int(&mut self, name: &[u8], val: i32) -> usize {
        self.push(name, Value::Int(val))
    }

    /// `NewStrVar`.
    pub fn new_str(&mut self, name: &[u8], val: &[u8]) -> usize {
        self.push(name, Value::Str(val.to_vec()))
    }

    /// `NewLabVar`.
    pub fn new_label(&mut self, name: &[u8], pos: usize, level: usize) -> usize {
        self.push(name, Value::Label { pos, level })
    }

    /// `NewIntAryVar` — `calloc`, so every element starts at zero.
    pub fn new_int_array(&mut self, name: &[u8], size: usize) -> usize {
        self.push(name, Value::IntArray(vec![0; size]))
    }

    /// `NewStrAryVar` — `calloc` of pointers, and `StrVarPtr` reads a NULL one
    /// back as `""`, so an empty string is the same starting state.
    pub fn new_str_array(&mut self, name: &[u8], size: usize) -> usize {
        self.push(name, Value::StrArray(vec![Vec::new(); size]))
    }

    /// `DelLabVar` — drop every label defined at or below an include level,
    /// which is what closing an included file does to its labels.
    pub fn del_labels_from(&mut self, level: usize) {
        self.vars.retain(|v| match v.value {
            Value::Label { level: l, .. } => l < level,
            _ => true,
        });
        self.index.clear();
        for (i, v) in self.vars.iter().enumerate() {
            self.index.insert(v.name.to_ascii_lowercase(), i);
        }
    }

    /// `CopyLabel`.
    pub fn label(&self, id: usize) -> Option<(usize, usize)> {
        match self.vars[id].value {
            Value::Label { pos, level } => Some((pos, level)),
            _ => None,
        }
    }

    pub fn array_len(&self, id: usize) -> usize {
        match &self.vars[id].value {
            Value::IntArray(v) => v.len(),
            Value::StrArray(v) => v.len(),
            _ => 0,
        }
    }

    /// `GetIntVarFromArray` / `GetStrVarFromArray` — bound the index and make a
    /// reference to the element.
    pub fn elem(&self, id: usize, index: i32) -> TtlResult<VarRef> {
        let n = self.array_len(id);
        if index < 0 || index as usize >= n {
            return Err(TtlError::OutOfRange);
        }
        Ok(VarRef::Elem(id, index as usize))
    }

    /// `CopyIntVal`. A reference to the wrong kind of thing reads as zero,
    /// which upstream reaches by reading the union's other arm.
    pub fn int_at(&self, r: VarRef) -> i32 {
        match r {
            VarRef::Scalar(i) => match &self.vars[i].value {
                Value::Int(v) => *v,
                _ => 0,
            },
            VarRef::Elem(i, j) => match &self.vars[i].value {
                Value::IntArray(v) => v.get(j).copied().unwrap_or(0),
                _ => 0,
            },
        }
    }

    /// `StrVarPtr`.
    pub fn str_at(&self, r: VarRef) -> &[u8] {
        match r {
            VarRef::Scalar(i) => match &self.vars[i].value {
                Value::Str(v) => v,
                _ => b"",
            },
            VarRef::Elem(i, j) => match &self.vars[i].value {
                Value::StrArray(v) => v.get(j).map(|s| s.as_slice()).unwrap_or(b""),
                _ => b"",
            },
        }
    }

    /// `SetIntVal`.
    pub fn set_int(&mut self, r: VarRef, val: i32) {
        match r {
            VarRef::Scalar(i) => {
                if let Value::Int(v) = &mut self.vars[i].value {
                    *v = val;
                }
            }
            VarRef::Elem(i, j) => {
                if let Value::IntArray(v) = &mut self.vars[i].value {
                    if let Some(slot) = v.get_mut(j) {
                        *slot = val;
                    }
                }
            }
        }
    }

    /// `SetStrVal`.
    pub fn set_str(&mut self, r: VarRef, val: &[u8]) {
        match r {
            VarRef::Scalar(i) => {
                if let Value::Str(v) = &mut self.vars[i].value {
                    v.clear();
                    v.extend_from_slice(val);
                }
            }
            VarRef::Elem(i, j) => {
                if let Value::StrArray(v) = &mut self.vars[i].value {
                    if let Some(slot) = v.get_mut(j) {
                        slot.clear();
                        slot.extend_from_slice(val);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_is_matched_without_regard_to_case() {
        let mut v = Vars::new();
        v.new_int(b"Result", 0);
        assert_eq!(v.find(b"result").map(|(_, t)| t), Some(VarType::Integer));
        assert_eq!(v.find(b"RESULT").map(|(_, t)| t), Some(VarType::Integer));
        assert_eq!(v.find(b"resul"), None);
    }

    #[test]
    fn ifdefineds_numbers_are_upstreams() {
        assert_eq!(VarType::Unknown.code(), 0);
        assert_eq!(VarType::Integer.code(), 1);
        assert_eq!(VarType::String.code(), 3);
        assert_eq!(VarType::IntArray.code(), 5);
        assert_eq!(VarType::StrArray.code(), 6);
    }

    #[test]
    fn an_index_is_bounded_at_both_ends() {
        let mut v = Vars::new();
        let id = v.new_int_array(b"a", 3);
        assert_eq!(v.elem(id, 0), Ok(VarRef::Elem(id, 0)));
        assert_eq!(v.elem(id, 2), Ok(VarRef::Elem(id, 2)));
        assert_eq!(v.elem(id, 3), Err(TtlError::OutOfRange));
        assert_eq!(v.elem(id, -1), Err(TtlError::OutOfRange));
    }

    #[test]
    fn closing_an_include_takes_its_labels_and_leaves_its_variables() {
        let mut v = Vars::new();
        v.new_int(b"keep", 1);
        v.new_label(b"outer", 0, 0);
        v.new_label(b"inner", 0, 1);
        v.del_labels_from(1);
        assert!(v.find(b"inner").is_none());
        assert!(v.find(b"outer").is_some());
        // ...and the surviving names still resolve after the indices shift.
        assert_eq!(v.find(b"keep").map(|(_, t)| t), Some(VarType::Integer));
        let (id, _) = v.find(b"outer").unwrap();
        assert_eq!(v.label(id), Some((0, 0)));
    }
}

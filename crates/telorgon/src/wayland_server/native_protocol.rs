use std::collections::BTreeMap;
use std::ffi::{CString, c_char};
use std::fmt;
use std::ptr;

use crate::wayland_server::ffi::{wl_interface, wl_message};
use crate::wayland_server::{ArgumentType, InterfaceSchema, ProtocolSchema};

/// Stable native descriptors built directly from one official Wayland XML schema.
///
/// The backing allocations live as long as this owner. A global/resource created with one of its
/// interface pointers must not outlive it.
pub struct NativeProtocol {
    schema: ProtocolSchema,
    interfaces: Box<[wl_interface]>,
    names: Vec<CString>,
    signatures: Vec<CString>,
    methods: Vec<Box<[wl_message]>>,
    events: Vec<Box<[wl_message]>>,
    argument_types: Vec<Box<[*const wl_interface]>>,
    indices: BTreeMap<String, usize>,
}

impl NativeProtocol {
    pub fn new(schema: ProtocolSchema) -> Result<Self, NativeProtocolError> {
        if schema.interfaces.len() > i32::MAX as usize {
            return Err(NativeProtocolError::CountOverflow);
        }
        let mut indices = BTreeMap::new();
        for (index, interface) in schema.interfaces.iter().enumerate() {
            if indices.insert(interface.name.clone(), index).is_some() {
                return Err(NativeProtocolError::DuplicateInterface);
            }
        }

        let mut names = Vec::new();
        for interface in &schema.interfaces {
            names.push(c_string(&interface.name)?);
        }
        let mut interfaces: Box<[wl_interface]> = schema
            .interfaces
            .iter()
            .enumerate()
            .map(|(index, interface)| {
                Ok(wl_interface {
                    name: names[index].as_ptr(),
                    version: native_count(interface.version as usize)?,
                    method_count: native_count(interface.requests.len())?,
                    methods: ptr::null(),
                    event_count: native_count(interface.events.len())?,
                    events: ptr::null(),
                })
            })
            .collect::<Result<Vec<_>, NativeProtocolError>>()?
            .into_boxed_slice();

        let interface_base = interfaces.as_ptr();
        let mut signatures = Vec::new();
        let mut argument_types = Vec::new();
        let mut methods = Vec::with_capacity(schema.interfaces.len());
        let mut events = Vec::with_capacity(schema.interfaces.len());
        for interface in &schema.interfaces {
            methods.push(build_messages(
                &interface.requests,
                &indices,
                interface_base,
                &mut names,
                &mut signatures,
                &mut argument_types,
            )?);
            events.push(build_messages(
                &interface.events,
                &indices,
                interface_base,
                &mut names,
                &mut signatures,
                &mut argument_types,
            )?);
        }
        for index in 0..interfaces.len() {
            interfaces[index].methods = methods[index].as_ptr();
            interfaces[index].events = events[index].as_ptr();
        }

        Ok(Self {
            schema,
            interfaces,
            names,
            signatures,
            methods,
            events,
            argument_types,
            indices,
        })
    }

    pub fn schema(&self) -> &ProtocolSchema {
        &self.schema
    }

    pub fn interface(&self, name: &str) -> Option<&wl_interface> {
        self.indices.get(name).map(|index| &self.interfaces[*index])
    }

    pub fn interface_schema(&self, name: &str) -> Option<&InterfaceSchema> {
        self.schema.interface(name)
    }

    pub fn retained_allocation_count(&self) -> usize {
        self.names.len()
            + self.signatures.len()
            + self.methods.len()
            + self.events.len()
            + self.argument_types.len()
    }
}

impl fmt::Debug for NativeProtocol {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NativeProtocol")
            .field("name", &self.schema.name)
            .field("interfaces", &self.schema.interfaces.len())
            .finish_non_exhaustive()
    }
}

fn build_messages(
    schemas: &[crate::wayland_server::MessageSchema],
    indices: &BTreeMap<String, usize>,
    interface_base: *const wl_interface,
    names: &mut Vec<CString>,
    signatures: &mut Vec<CString>,
    argument_types: &mut Vec<Box<[*const wl_interface]>>,
) -> Result<Box<[wl_message]>, NativeProtocolError> {
    let mut messages = Vec::with_capacity(schemas.len());
    for schema in schemas {
        names.push(c_string(&schema.name)?);
        let name = names.last().expect("pushed").as_ptr();
        signatures.push(c_string(&schema.native_signature())?);
        let signature = signatures.last().expect("pushed").as_ptr();
        let types: Box<[*const wl_interface]> = schema
            .arguments
            .iter()
            .map(|argument| {
                if !matches!(
                    argument.argument_type,
                    ArgumentType::Object | ArgumentType::NewId
                ) {
                    return ptr::null();
                }
                argument
                    .interface
                    .as_ref()
                    .and_then(|name| indices.get(name))
                    .map_or(ptr::null(), |index| unsafe { interface_base.add(*index) })
            })
            .collect::<Vec<_>>()
            .into_boxed_slice();
        let types_pointer = if types.is_empty() {
            ptr::null()
        } else {
            types.as_ptr()
        };
        argument_types.push(types);
        messages.push(wl_message {
            name,
            signature,
            types: types_pointer,
        });
    }
    Ok(messages.into_boxed_slice())
}

fn c_string(value: &str) -> Result<CString, NativeProtocolError> {
    CString::new(value).map_err(|_| NativeProtocolError::InteriorNul)
}

fn native_count(value: usize) -> Result<i32, NativeProtocolError> {
    i32::try_from(value).map_err(|_| NativeProtocolError::CountOverflow)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeProtocolError {
    DuplicateInterface,
    InteriorNul,
    CountOverflow,
}

impl fmt::Display for NativeProtocolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "cannot create native Wayland protocol descriptors: {self:?}"
        )
    }
}

impl std::error::Error for NativeProtocolError {}

// The wl_interface ABI uses const char pointers; this assertion catches accidental type drift.
const _: fn(*const c_char) = |_| {};

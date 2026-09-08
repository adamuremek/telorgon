//! Build-only XML model. No parser or filesystem access is linked into the runtime.
use std::collections::BTreeSet;

use roxmltree::{Document, Node, ParsingOptions};

pub const MAX_SOURCE_BYTES: usize = 16 * 1024 * 1024;
const MAX_INTERFACES: usize = 1024;
const MAX_MESSAGES: usize = 4096;
// libwayland closure ABI permits at most 20 wire arguments, after new_id expansion.
const MAX_ARGUMENTS: usize = 20;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Protocol {
    pub name: String,
    pub interfaces: Vec<Interface>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Interface {
    pub name: String,
    pub version: u32,
    pub requests: Vec<Message>,
    pub events: Vec<Message>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub name: String,
    pub since: u32,
    pub destructor: bool,
    pub arguments: Vec<Argument>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Argument {
    pub name: String,
    pub kind: &'static str,
    pub interface: Option<String>,
    pub allow_null: bool,
}

impl Message {
    pub fn signature(&self) -> String {
        let mut result = if self.since > 1 {
            self.since.to_string()
        } else {
            String::new()
        };
        for arg in &self.arguments {
            if arg.allow_null {
                result.push('?');
            }
            result.push(match arg.kind {
                "Int" => 'i',
                "Uint" => 'u',
                "Fixed" => 'f',
                "String" => 's',
                "Object" => 'o',
                "NewId" => 'n',
                "Array" => 'a',
                "Fd" => 'h',
                _ => unreachable!(),
            });
        }
        result
    }

    pub fn contract(&self, interface: &str, kind: &str, opcode: usize) -> String {
        let types = self
            .arguments
            .iter()
            .map(|arg| arg.interface.as_deref().unwrap_or("-"))
            .collect::<Vec<_>>()
            .join(",");
        let signature = self.signature();
        let signature = if signature.is_empty() {
            "-"
        } else {
            &signature
        };
        let types = if types.is_empty() { "-" } else { &types };
        format!(
            "{interface} {kind} {opcode} {} {signature} {} {types}",
            self.name, self.destructor
        )
    }
}

fn identifier(node: Node<'_, '_>, attribute: &str) -> Result<String, String> {
    let value = node
        .attribute(attribute)
        .ok_or_else(|| format!("<{}> missing {attribute}", node.tag_name().name()))?;
    if value.is_empty()
        || !value
            .bytes()
            .enumerate()
            .all(|(i, c)| c == b'_' || c.is_ascii_alphabetic() || (i > 0 && c.is_ascii_digit()))
    {
        return Err(format!(
            "invalid {attribute} {value:?} in <{}>",
            node.tag_name().name()
        ));
    }
    Ok(value.to_owned())
}

fn version(value: &str) -> Result<u32, String> {
    value
        .parse::<u32>()
        .ok()
        .filter(|value| *value > 0 && *value <= i32::MAX as u32)
        .ok_or_else(|| format!("invalid native version {value:?}: expected 1..=i32::MAX"))
}

pub fn parse(source: &str) -> Result<Protocol, String> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err("XML source exceeds 16 MiB limit".into());
    }
    let doc = Document::parse_with_options(
        source,
        ParsingOptions {
            allow_dtd: false,
            nodes_limit: 1_000_000,
            ..Default::default()
        },
    )
    .map_err(|error| format!("malformed XML: {error}"))?;
    // Validate nesting even in nodes not needed for descriptors, so misplaced messages cannot
    // silently disappear. Documentation and enum values do not affect the wire ABI.
    for node in doc.descendants().filter(Node::is_element) {
        let parent = node.parent_element().map(|p| p.tag_name().name());
        let allowed = match node.tag_name().name() {
            "protocol" => parent.is_none(),
            "interface" | "copyright" => parent == Some("protocol"),
            "request" | "event" | "enum" => parent == Some("interface"),
            "arg" => matches!(parent, Some("request" | "event")),
            "entry" => parent == Some("enum"),
            "description" => matches!(
                parent,
                Some("protocol" | "interface" | "request" | "event" | "arg" | "enum" | "entry")
            ),
            _ => false,
        };
        if !allowed {
            return Err(format!(
                "invalid XML element/nesting: <{}> under {parent:?}",
                node.tag_name().name()
            ));
        }
    }
    let root = doc.root_element();
    if root.tag_name().name() != "protocol" {
        return Err("missing protocol root".into());
    }
    let mut protocol = Protocol {
        name: identifier(root, "name")?,
        interfaces: Vec::new(),
    };
    let mut names = BTreeSet::new();
    for node in root
        .children()
        .filter(|node| node.has_tag_name("interface"))
    {
        let name = identifier(node, "name")?;
        if !names.insert(name.clone()) {
            return Err(format!("duplicate interface {name}"));
        }
        let mut interface = Interface {
            name,
            version: version(
                node.attribute("version")
                    .ok_or("interface missing version")?,
            )?,
            requests: Vec::new(),
            events: Vec::new(),
        };
        for (tag, messages) in [
            ("request", &mut interface.requests),
            ("event", &mut interface.events),
        ] {
            let mut names = BTreeSet::new();
            for node in node.children().filter(|node| node.has_tag_name(tag)) {
                let name = identifier(node, "name")?;
                if !names.insert(name.clone()) {
                    return Err(format!("duplicate {tag} {}.{name}", interface.name));
                }
                let since = version(node.attribute("since").unwrap_or("1"))?;
                if since > interface.version {
                    return Err(format!(
                        "{}.{name}: since {since} exceeds interface version {}",
                        interface.name, interface.version
                    ));
                }
                let destructor = match node.attribute("type") {
                    None => false,
                    Some("destructor") => true,
                    Some(value) => {
                        return Err(format!(
                            "{}.{name}: invalid message type {value}",
                            interface.name
                        ));
                    }
                };
                let mut message = Message {
                    name,
                    since,
                    destructor,
                    arguments: Vec::new(),
                };
                let mut args = BTreeSet::new();
                let mut new_ids = 0;
                for arg in node.children().filter(|node| node.has_tag_name("arg")) {
                    let name = identifier(arg, "name")?;
                    if !args.insert(name.clone()) {
                        return Err(format!("duplicate argument {name}"));
                    }
                    let kind = match arg.attribute("type") {
                        Some("int") => "Int",
                        Some("uint") => "Uint",
                        Some("fixed") => "Fixed",
                        Some("string") => "String",
                        Some("object") => "Object",
                        Some("new_id") => "NewId",
                        Some("array") => "Array",
                        Some("fd") => "Fd",
                        value => return Err(format!("argument {name}: unknown type {value:?}")),
                    };
                    let interface = arg
                        .attribute("interface")
                        .map(|_| identifier(arg, "interface"))
                        .transpose()?;
                    if interface.is_some() && !matches!(kind, "Object" | "NewId") {
                        return Err(format!(
                            "argument {name}: interface only valid on object/new_id"
                        ));
                    }
                    let allow_null = match arg.attribute("allow-null") {
                        None | Some("false") => false,
                        Some("true") => true,
                        value => {
                            return Err(format!("argument {name}: invalid allow-null {value:?}"));
                        }
                    };
                    if allow_null && !matches!(kind, "Object" | "String") {
                        return Err(format!("argument {name}: {kind} cannot be nullable"));
                    }
                    if kind == "NewId" {
                        new_ids += 1;
                        if new_ids > 1 {
                            return Err("message has multiple new_id arguments".into());
                        }
                        if interface.is_none() {
                            if tag == "event" {
                                return Err("event new_id requires an interface".into());
                            }
                            // libwayland dispatches the expanded wire arguments, not XML arguments.
                            for (suffix, kind) in [("interface", "String"), ("version", "Uint")] {
                                message.arguments.push(Argument {
                                    name: format!("{name}_{suffix}"),
                                    kind,
                                    interface: None,
                                    allow_null: false,
                                });
                            }
                        }
                    }
                    message.arguments.push(Argument {
                        name,
                        kind,
                        interface,
                        allow_null,
                    });
                }
                if message.arguments.len() > MAX_ARGUMENTS {
                    return Err("message wire argument count exceeds libwayland limit of 20".into());
                }
                messages.push(message);
            }
            if messages.len() > MAX_MESSAGES {
                return Err("interface message count exceeds 4096".into());
            }
        }
        protocol.interfaces.push(interface);
    }
    if protocol.interfaces.is_empty() || protocol.interfaces.len() > MAX_INTERFACES {
        return Err("protocol interface count must be 1..=1024".into());
    }
    Ok(protocol)
}

use std::fmt;

const MAX_SOURCE_BYTES: usize = 16 * 1024 * 1024;
const MAX_INTERFACES: usize = 1024;
const MAX_MESSAGES_PER_INTERFACE: usize = 4096;
const MAX_ARGUMENTS_PER_MESSAGE: usize = 128;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProtocolSchema {
    pub name: String,
    pub interfaces: Vec<InterfaceSchema>,
}

impl ProtocolSchema {
    /// Parses the XML message description format used by Wayland's official scanner.
    pub fn parse(source: &str) -> Result<Self, ProtocolSchemaError> {
        if source.len() > MAX_SOURCE_BYTES {
            return Err(ProtocolSchemaError::SourceTooLarge);
        }
        Parser::new(source).parse()
    }

    pub fn interface(&self, name: &str) -> Option<&InterfaceSchema> {
        self.interfaces
            .iter()
            .find(|interface| interface.name == name)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InterfaceSchema {
    pub name: String,
    pub version: u32,
    pub requests: Vec<MessageSchema>,
    pub events: Vec<MessageSchema>,
}

impl InterfaceSchema {
    pub fn request(&self, opcode: u32) -> Option<&MessageSchema> {
        self.requests.get(opcode as usize)
    }

    pub fn request_named(&self, name: &str) -> Option<(u32, &MessageSchema)> {
        self.requests
            .iter()
            .enumerate()
            .find(|(_, message)| message.name == name)
            .map(|(opcode, message)| (opcode as u32, message))
    }

    pub fn event_named(&self, name: &str) -> Option<(u32, &MessageSchema)> {
        self.events
            .iter()
            .enumerate()
            .find(|(_, message)| message.name == name)
            .map(|(opcode, message)| (opcode as u32, message))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageKind {
    Request,
    Event,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageSchema {
    pub name: String,
    pub since: u32,
    pub destructor: bool,
    pub kind: MessageKind,
    pub arguments: Vec<ArgumentSchema>,
}

impl MessageSchema {
    /// libwayland signature string derived from the official XML argument declarations.
    pub fn native_signature(&self) -> String {
        let mut signature = String::new();
        if self.since > 1 {
            signature.push_str(&self.since.to_string());
        }
        for argument in &self.arguments {
            if argument.allow_null {
                signature.push('?');
            }
            signature.push(argument.argument_type.signature());
        }
        signature
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArgumentSchema {
    pub name: String,
    pub argument_type: ArgumentType,
    pub interface: Option<String>,
    pub allow_null: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArgumentType {
    Int,
    Uint,
    Fixed,
    String,
    Object,
    NewId,
    Array,
    Fd,
}

impl ArgumentType {
    const fn signature(self) -> char {
        match self {
            Self::Int => 'i',
            Self::Uint => 'u',
            Self::Fixed => 'f',
            Self::String => 's',
            Self::Object => 'o',
            Self::NewId => 'n',
            Self::Array => 'a',
            Self::Fd => 'h',
        }
    }

    fn parse(value: &str) -> Result<Self, ProtocolSchemaError> {
        match value {
            "int" => Ok(Self::Int),
            "uint" => Ok(Self::Uint),
            "fixed" => Ok(Self::Fixed),
            "string" => Ok(Self::String),
            "object" => Ok(Self::Object),
            "new_id" => Ok(Self::NewId),
            "array" => Ok(Self::Array),
            "fd" => Ok(Self::Fd),
            _ => Err(ProtocolSchemaError::UnknownArgumentType),
        }
    }
}

struct Parser<'source> {
    source: &'source str,
    cursor: usize,
    protocol: Option<ProtocolSchema>,
    interface: Option<InterfaceSchema>,
    message: Option<MessageSchema>,
}

impl<'source> Parser<'source> {
    fn new(source: &'source str) -> Self {
        Self {
            source,
            cursor: 0,
            protocol: None,
            interface: None,
            message: None,
        }
    }

    fn parse(mut self) -> Result<ProtocolSchema, ProtocolSchemaError> {
        while let Some(tag) = self.next_tag()? {
            if tag.closing {
                self.close(&tag.name)?;
            } else {
                self.open(&tag)?;
                if tag.self_closing {
                    self.close(&tag.name)?;
                }
            }
        }
        if self.interface.is_some() || self.message.is_some() {
            return Err(ProtocolSchemaError::UnexpectedEnd);
        }
        let protocol = self
            .protocol
            .take()
            .ok_or(ProtocolSchemaError::MissingProtocol)?;
        if protocol.name.is_empty() || protocol.interfaces.is_empty() {
            return Err(ProtocolSchemaError::MissingProtocol);
        }
        Ok(protocol)
    }

    fn open(&mut self, tag: &Tag) -> Result<(), ProtocolSchemaError> {
        match tag.name.as_str() {
            "protocol" => {
                if self.protocol.is_some() {
                    return Err(ProtocolSchemaError::DuplicateProtocol);
                }
                self.protocol = Some(ProtocolSchema {
                    name: tag.required("name")?.to_owned(),
                    interfaces: Vec::new(),
                });
            }
            "interface" => {
                if self.protocol.is_none() || self.interface.is_some() {
                    return Err(ProtocolSchemaError::InvalidNesting);
                }
                self.interface = Some(InterfaceSchema {
                    name: tag.required("name")?.to_owned(),
                    version: parse_positive(tag.required("version")?)?,
                    requests: Vec::new(),
                    events: Vec::new(),
                });
            }
            "request" | "event" => {
                if self.interface.is_none() || self.message.is_some() {
                    return Err(ProtocolSchemaError::InvalidNesting);
                }
                let kind = if tag.name == "request" {
                    MessageKind::Request
                } else {
                    MessageKind::Event
                };
                self.message = Some(MessageSchema {
                    name: tag.required("name")?.to_owned(),
                    since: tag.optional("since").map_or(Ok(1), parse_positive)?,
                    destructor: tag.optional("type") == Some("destructor"),
                    kind,
                    arguments: Vec::new(),
                });
            }
            "arg" => {
                let message = self
                    .message
                    .as_mut()
                    .ok_or(ProtocolSchemaError::InvalidNesting)?;
                if message.arguments.len() >= MAX_ARGUMENTS_PER_MESSAGE {
                    return Err(ProtocolSchemaError::CollectionLimit);
                }
                message.arguments.push(ArgumentSchema {
                    name: tag.required("name")?.to_owned(),
                    argument_type: ArgumentType::parse(tag.required("type")?)?,
                    interface: tag.optional("interface").map(str::to_owned),
                    allow_null: tag.optional("allow-null") == Some("true"),
                });
            }
            _ => {}
        }
        Ok(())
    }

    fn close(&mut self, name: &str) -> Result<(), ProtocolSchemaError> {
        match name {
            "request" | "event" => {
                let message = self
                    .message
                    .take()
                    .ok_or(ProtocolSchemaError::InvalidNesting)?;
                let interface = self
                    .interface
                    .as_mut()
                    .ok_or(ProtocolSchemaError::InvalidNesting)?;
                let messages = match message.kind {
                    MessageKind::Request => &mut interface.requests,
                    MessageKind::Event => &mut interface.events,
                };
                if messages.len() >= MAX_MESSAGES_PER_INTERFACE {
                    return Err(ProtocolSchemaError::CollectionLimit);
                }
                messages.push(message);
            }
            "interface" => {
                if self.message.is_some() {
                    return Err(ProtocolSchemaError::InvalidNesting);
                }
                let interface = self
                    .interface
                    .take()
                    .ok_or(ProtocolSchemaError::InvalidNesting)?;
                let protocol = self
                    .protocol
                    .as_mut()
                    .ok_or(ProtocolSchemaError::InvalidNesting)?;
                if protocol.interfaces.len() >= MAX_INTERFACES {
                    return Err(ProtocolSchemaError::CollectionLimit);
                }
                protocol.interfaces.push(interface);
            }
            "protocol" => {
                if self.interface.is_some() || self.message.is_some() {
                    return Err(ProtocolSchemaError::InvalidNesting);
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn next_tag(&mut self) -> Result<Option<Tag>, ProtocolSchemaError> {
        loop {
            let Some(relative_start) = self.source[self.cursor..].find('<') else {
                return Ok(None);
            };
            let start = self.cursor + relative_start;
            if self.source[start..].starts_with("<!--") {
                let end = self.source[start + 4..]
                    .find("-->")
                    .ok_or(ProtocolSchemaError::MalformedXml)?;
                self.cursor = start + 4 + end + 3;
                continue;
            }
            if self.source[start..].starts_with("<?") {
                let end = self.source[start + 2..]
                    .find("?>")
                    .ok_or(ProtocolSchemaError::MalformedXml)?;
                self.cursor = start + 2 + end + 2;
                continue;
            }
            if self.source[start..].starts_with("<!") {
                let end = self.source[start + 2..]
                    .find('>')
                    .ok_or(ProtocolSchemaError::MalformedXml)?;
                self.cursor = start + 2 + end + 1;
                continue;
            }
            let end = self.source[start + 1..]
                .find('>')
                .ok_or(ProtocolSchemaError::MalformedXml)?
                + start
                + 1;
            self.cursor = end + 1;
            return Tag::parse(&self.source[start + 1..end]).map(Some);
        }
    }
}

#[derive(Debug)]
struct Tag {
    name: String,
    attributes: Vec<(String, String)>,
    closing: bool,
    self_closing: bool,
}

impl Tag {
    fn parse(raw: &str) -> Result<Self, ProtocolSchemaError> {
        let raw = raw.trim();
        let closing = raw.starts_with('/');
        let raw = raw.strip_prefix('/').unwrap_or(raw).trim();
        let self_closing = raw.ends_with('/');
        let raw = raw.strip_suffix('/').unwrap_or(raw).trim();
        let name_end = raw.find(char::is_whitespace).unwrap_or(raw.len());
        let name = &raw[..name_end];
        if name.is_empty() {
            return Err(ProtocolSchemaError::MalformedXml);
        }
        let mut attributes = Vec::new();
        if !closing {
            let mut rest = raw[name_end..].trim();
            while !rest.is_empty() {
                let equals = rest.find('=').ok_or(ProtocolSchemaError::MalformedXml)?;
                let key = rest[..equals].trim();
                if key.is_empty() || key.chars().any(char::is_whitespace) {
                    return Err(ProtocolSchemaError::MalformedXml);
                }
                rest = rest[equals + 1..].trim_start();
                let quote = rest
                    .chars()
                    .next()
                    .ok_or(ProtocolSchemaError::MalformedXml)?;
                if quote != '"' && quote != '\'' {
                    return Err(ProtocolSchemaError::MalformedXml);
                }
                let after_quote = &rest[quote.len_utf8()..];
                let value_end = after_quote
                    .find(quote)
                    .ok_or(ProtocolSchemaError::MalformedXml)?;
                attributes.push((key.to_owned(), decode_entities(&after_quote[..value_end])?));
                rest = after_quote[value_end + quote.len_utf8()..].trim_start();
            }
        }
        Ok(Self {
            name: name.to_owned(),
            attributes,
            closing,
            self_closing,
        })
    }

    fn optional(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    fn required(&self, name: &str) -> Result<&str, ProtocolSchemaError> {
        self.optional(name)
            .ok_or(ProtocolSchemaError::MissingAttribute)
    }
}

fn parse_positive(value: &str) -> Result<u32, ProtocolSchemaError> {
    let value = value
        .parse::<u32>()
        .map_err(|_| ProtocolSchemaError::InvalidNumber)?;
    if value == 0 {
        Err(ProtocolSchemaError::InvalidNumber)
    } else {
        Ok(value)
    }
}

fn decode_entities(value: &str) -> Result<String, ProtocolSchemaError> {
    let mut decoded = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(index) = rest.find('&') {
        decoded.push_str(&rest[..index]);
        rest = &rest[index..];
        let (entity, replacement) = if rest.starts_with("&amp;") {
            (5, '&')
        } else if rest.starts_with("&lt;") {
            (4, '<')
        } else if rest.starts_with("&gt;") {
            (4, '>')
        } else if rest.starts_with("&quot;") {
            (6, '"')
        } else if rest.starts_with("&apos;") {
            (6, '\'')
        } else {
            return Err(ProtocolSchemaError::MalformedXml);
        };
        decoded.push(replacement);
        rest = &rest[entity..];
    }
    decoded.push_str(rest);
    Ok(decoded)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProtocolSchemaError {
    SourceTooLarge,
    MalformedXml,
    MissingProtocol,
    DuplicateProtocol,
    MissingAttribute,
    InvalidNumber,
    UnknownArgumentType,
    InvalidNesting,
    UnexpectedEnd,
    CollectionLimit,
}

impl fmt::Display for ProtocolSchemaError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Wayland protocol XML is invalid: {self:?}")
    }
}

impl std::error::Error for ProtocolSchemaError {}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
        <?xml version="1.0" encoding="UTF-8"?>
        <protocol name="sample">
          <interface name="sample_surface" version="3">
            <request name="attach">
              <arg name="buffer" type="object" interface="wl_buffer" allow-null="true"/>
              <arg name="x" type="int"/>
            </request>
            <event name="done" since="2"><arg name="serial" type="uint"/></event>
          </interface>
        </protocol>
    "#;

    #[test]
    fn official_message_shape_is_parsed_and_signed() {
        let schema = ProtocolSchema::parse(SAMPLE).unwrap();
        let interface = schema.interface("sample_surface").unwrap();
        assert_eq!(interface.requests[0].native_signature(), "?oi");
        assert_eq!(interface.events[0].native_signature(), "2u");
    }

    #[test]
    fn malformed_or_unbounded_sources_are_rejected() {
        assert!(ProtocolSchema::parse("<protocol>").is_err());
        assert_eq!(
            ProtocolSchema::parse(&" ".repeat(MAX_SOURCE_BYTES + 1)),
            Err(ProtocolSchemaError::SourceTooLarge)
        );
    }
}

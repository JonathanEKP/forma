//! Rendering one definition for one door.
//!
//! An operation is defined once; what differs between the doors is only how
//! it is written down for each. There are exactly two differences, and this
//! module is the whole of both:
//!
//! - **Names.** A description that says "call get_design" must keep saying so
//!   to an MCP host, and must say `design.get` to a Pontis session, whose tools
//!   are named after capability ids (ADR-022). So text names another operation
//!   as `{design.get}`, and rendering swaps in that operation's name on the door.
//!   That is a substitution and nothing else — no conditionals, no defaults, no
//!   formatting. A name that does not resolve, or names an operation the door
//!   does not expose, is an error rather than a stray brace in front of a model;
//!   `{{` and `}}` are how text says a brace it means.
//! - **Schema dialect.** forma-mcp's schemas are what rmcp generates: JSON
//!   Schema 2020-12 with a `$schema` key and schemars' numeric `format`
//!   annotations (`uint64`, `float`, …). Pontis compiles input schemas with Ajv
//!   in strict draft-07 mode, which refuses both — measured, not assumed — so
//!   the Pontis door drops them. Neither changes what an input may be: the
//!   annotations are not assertions, and `minimum: 0` already carries what
//!   `uint64` would.
//!
//! The manual has one difference more, and it lives in [`crate::manual`]
//! rather than here, because no operation's description may use it.

use serde_json::{Map, Value};

use crate::operations::{Operation, REGISTRY};

/// The two ways an agent reaches Forma (plan §8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Door {
    /// forma-mcp, serving any MCP host directly.
    Mcp,
    /// The capability catalog the Pontis sidecar declares.
    Pontis,
}

/// What an operation is called on a door, if that door exposes it.
pub fn name_on(operation: &Operation, door: Door) -> Option<&'static str> {
    match door {
        Door::Mcp => operation.doors.mcp_tool(),
        Door::Pontis => operation.doors.on_pontis().then_some(operation.id),
    }
}

/// Swaps every `{operation.id}` in `text` for that operation's name on `door`.
///
/// `{{` and `}}` are a literal brace, as in Rust's `format!`. Descriptions never
/// need one; the manual does, because its examples are JSON.
pub fn render_text(text: &str, door: Door) -> Result<String, String> {
    let mut out = String::with_capacity(text.len());
    for piece in pieces(text)? {
        match piece {
            Piece::Text(text) => out.push_str(text),
            Piece::Name(id) => {
                let operation = REGISTRY
                    .iter()
                    .find(|operation| operation.id == id)
                    .ok_or_else(|| format!("{{{id}}} names no operation in the registry"))?;
                let name = name_on(operation, door).ok_or_else(|| {
                    format!("{{{id}}} is not reachable through the {door:?} door")
                })?;
                out.push_str(name);
            }
        }
    }
    Ok(out)
}

/// Every `{operation.id}` that `text` names, in order, whether or not it
/// resolves — read by the same parser `render_text` uses, so a check written
/// against this cannot disagree with rendering about what a name is.
pub fn names(text: &str) -> Result<Vec<&str>, String> {
    Ok(pieces(text)?
        .into_iter()
        .filter_map(|piece| match piece {
            Piece::Name(id) => Some(id),
            Piece::Text(_) => None,
        })
        .collect())
}

enum Piece<'a> {
    Text(&'a str),
    Name(&'a str),
}

fn pieces(text: &str) -> Result<Vec<Piece<'_>>, String> {
    let mut pieces = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find(['{', '}']) {
        pieces.push(Piece::Text(&rest[..at]));
        let brace = &rest[at..at + 1];
        let after = &rest[at + 1..];
        if after.starts_with(brace) {
            pieces.push(Piece::Text(brace));
            rest = &after[1..];
            continue;
        }
        if brace == "}" {
            return Err(format!("unopened '}}' in {text:?}"));
        }
        let close = after
            .find('}')
            .ok_or_else(|| format!("unclosed '{{' in {text:?}"))?;
        pieces.push(Piece::Name(&after[..close]));
        rest = &after[close + 1..];
    }
    pieces.push(Piece::Text(rest));
    Ok(pieces)
}

/// The operation's input schema as `door` publishes it.
pub fn render_schema(operation: &Operation, door: Door) -> Result<Map<String, Value>, String> {
    let mut schema = Value::Object((operation.input)());
    render_descriptions(&mut schema, door)?;
    if door == Door::Pontis {
        to_pontis_dialect(&mut schema);
    }
    match schema {
        Value::Object(object) => Ok(object),
        _ => unreachable!("rendering never changes the root's type"),
    }
}

/// Applies `render_text` to every `description` in a schema. Parameter
/// descriptions name other operations as often as tool descriptions do.
fn render_descriptions(schema: &mut Value, door: Door) -> Result<(), String> {
    match schema {
        Value::Object(object) => {
            for (key, value) in object.iter_mut() {
                match value {
                    Value::String(text) if key == "description" => {
                        *text = render_text(text, door)?;
                    }
                    _ => render_descriptions(value, door)?,
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                render_descriptions(item, door)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn to_pontis_dialect(schema: &mut Value) {
    match schema {
        Value::Object(object) => {
            object.remove("$schema");
            if object
                .get("format")
                .and_then(Value::as_str)
                .is_some_and(is_numeric_format)
            {
                object.remove("format");
            }
            for value in object.values_mut() {
                to_pontis_dialect(value);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(to_pontis_dialect),
        _ => {}
    }
}

/// schemars' names for Rust's numeric types. Any other format is left in
/// place, so one Ajv does not know fails loudly at registration instead of
/// vanishing here.
fn is_numeric_format(format: &str) -> bool {
    matches!(
        format,
        "int8"
            | "int16"
            | "int32"
            | "int64"
            | "uint8"
            | "uint16"
            | "uint32"
            | "uint64"
            | "uint"
            | "int"
            | "float"
            | "double"
    )
}

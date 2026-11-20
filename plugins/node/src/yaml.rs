//! YAML documents as a tree of nodes, each with the line where it starts (for `pnpm-workspace.yaml`).

use std::collections::HashMap;

use yaml_rust2::parser::{Event, MarkedEventReceiver, Parser};
use yaml_rust2::scanner::{Marker, TScalarStyle};

/// A node's value. A plain empty, `~`, `null` or `Null` scalar is [`Value::Null`]; every other
/// scalar keeps its text, whatever it would mean in YAML 1.1 (`on` stays the string `on`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Value {
    Null,
    Scalar(String),
    Sequence(Vec<Node>),
    Mapping(Vec<(Node, Node)>),
}

/// One node and the one-based line where it starts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Node {
    pub(crate) value: Value,
    pub(crate) line: usize,
}

impl Node {
    /// The value of the first entry whose key is the scalar `key`.
    pub(crate) fn get(&self, key: &str) -> Option<&Node> {
        let Value::Mapping(entries) = &self.value else {
            return None;
        };
        entries
            .iter()
            .find(|(name, _)| name.scalar() == Some(key))
            .map(|(_, value)| value)
    }

    pub(crate) fn scalar(&self) -> Option<&str> {
        match &self.value {
            Value::Scalar(text) => Some(text),
            _ => None,
        }
    }

    pub(crate) fn sequence(&self) -> Option<&[Node]> {
        match &self.value {
            Value::Sequence(items) => Some(items),
            _ => None,
        }
    }

    pub(crate) fn mapping(&self) -> Option<&[(Node, Node)]> {
        match &self.value {
            Value::Mapping(entries) => Some(entries),
            _ => None,
        }
    }
}

enum Open {
    Sequence {
        items: Vec<Node>,
        line: usize,
        anchor: usize,
    },
    Mapping {
        entries: Vec<(Node, Node)>,
        key: Option<Node>,
        line: usize,
        anchor: usize,
    },
}

#[derive(Default)]
struct Builder {
    open: Vec<Open>,
    documents: Vec<Node>,
    anchors: HashMap<usize, Node>,
}

impl Builder {
    fn add(&mut self, node: Node, anchor: usize) {
        if anchor != 0 {
            self.anchors.insert(anchor, node.clone());
        }
        match self.open.last_mut() {
            None => self.documents.push(node),
            Some(Open::Sequence { items, .. }) => items.push(node),
            Some(Open::Mapping { entries, key, .. }) => match key.take() {
                None => *key = Some(node),
                Some(name) => entries.push((name, node)),
            },
        }
    }
}

impl MarkedEventReceiver for Builder {
    fn on_event(&mut self, event: Event, mark: Marker) {
        let line = mark.line();
        match event {
            Event::Scalar(text, style, anchor, _) => {
                let null = style == TScalarStyle::Plain
                    && matches!(text.as_str(), "" | "~" | "null" | "Null");
                let value = if null {
                    Value::Null
                } else {
                    Value::Scalar(text)
                };
                self.add(Node { value, line }, anchor);
            }
            Event::SequenceStart(anchor, _) => self.open.push(Open::Sequence {
                items: Vec::new(),
                line,
                anchor,
            }),
            Event::MappingStart(anchor, _) => self.open.push(Open::Mapping {
                entries: Vec::new(),
                key: None,
                line,
                anchor,
            }),
            Event::SequenceEnd => {
                if let Some(Open::Sequence {
                    items,
                    line,
                    anchor,
                }) = self.open.pop()
                {
                    let value = Value::Sequence(items);
                    self.add(Node { value, line }, anchor);
                }
            }
            Event::MappingEnd => {
                if let Some(Open::Mapping {
                    entries,
                    line,
                    anchor,
                    ..
                }) = self.open.pop()
                {
                    let value = Value::Mapping(entries);
                    self.add(Node { value, line }, anchor);
                }
            }
            Event::Alias(anchor) => {
                let node = self.anchors.get(&anchor).cloned().unwrap_or(Node {
                    value: Value::Null,
                    line,
                });
                self.add(node, 0);
            }
            _ => {}
        }
    }
}

/// The first document of `text`; an empty text is a null node on line 1.
///
/// # Errors
/// The parser's message when `text` is not valid YAML.
pub(crate) fn parse(text: &str) -> Result<Node, String> {
    let mut builder = Builder::default();
    Parser::new_from_str(text)
        .load(&mut builder, false)
        .map_err(|error| error.to_string())?;
    Ok(builder.documents.into_iter().next().unwrap_or(Node {
        value: Value::Null,
        line: 1,
    }))
}

#[cfg(test)]
mod tests {
    use super::{Value, parse};

    #[test]
    fn nodes_carry_their_lines_and_aliases_their_anchor() {
        let root = parse("a: &x one\nb:\n  - *x\n  - ~\non: push\n").expect("valid");
        assert_eq!(root.get("a").map(|node| node.line), Some(1));
        let items = root
            .get("b")
            .and_then(|node| node.sequence())
            .expect("a list");
        assert_eq!(items[0].scalar(), Some("one"));
        assert_eq!(items[1].value, Value::Null);
        assert_eq!(items[1].line, 4);
        assert_eq!(root.get("on").and_then(|node| node.scalar()), Some("push"));
        assert_eq!(parse("").expect("empty").value, Value::Null);
        assert!(parse("a: [").is_err());
    }
}

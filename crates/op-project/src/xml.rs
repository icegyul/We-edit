//! A small bounded XML tree for interchange files (docs/prproj-spec.md 3).
//!
//! Documents are treated as hostile: DTDs are rejected (so no external or custom entities),
//! only the predefined and numeric character references are resolved, and element count,
//! nesting depth, attribute and text sizes are limited.

use std::fmt::Write as _;

use quick_xml::XmlVersion;
use quick_xml::events::Event;
use thiserror::Error;

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub max_nodes: usize,
    pub max_depth: usize,
    pub max_attr_len: usize,
    pub max_text_len: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_nodes: 5_000_000,
            max_depth: 256,
            max_attr_len: 1 << 20,
            max_text_len: 64 << 20,
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum XmlError {
    #[error("XML syntax error: {0}")]
    Syntax(String),
    #[error("documents with a DTD are not accepted")]
    Dtd,
    #[error("unknown entity &{0};")]
    Entity(String),
    #[error("the document exceeds the {0} limit")]
    Limit(&'static str),
    #[error("the document has no root element")]
    Empty,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<usize>,
    pub text: String,
    pub parent: Option<usize>,
}

/// Parsed document. Node 0 is the root element.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Doc {
    pub nodes: Vec<Node>,
}

impl Doc {
    pub fn root(&self) -> usize {
        0
    }

    pub fn node(&self, i: usize) -> &Node {
        &self.nodes[i]
    }

    pub fn name(&self, i: usize) -> &str {
        &self.nodes[i].name
    }

    pub fn attr(&self, i: usize, key: &str) -> Option<&str> {
        self.nodes[i]
            .attrs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    pub fn text(&self, i: usize) -> &str {
        self.nodes[i].text.trim()
    }

    pub fn children(&self, i: usize) -> impl Iterator<Item = usize> + '_ {
        self.nodes[i].children.iter().copied()
    }

    /// First child with this name.
    pub fn child(&self, i: usize, name: &str) -> Option<usize> {
        self.children(i).find(|c| self.nodes[*c].name == name)
    }

    pub fn children_named<'a>(
        &'a self,
        i: usize,
        name: &'a str,
    ) -> impl Iterator<Item = usize> + 'a {
        self.children(i)
            .filter(move |c| self.nodes[*c].name == name)
    }

    /// Follows a `/`-separated path of child names.
    pub fn path(&self, i: usize, path: &str) -> Option<usize> {
        let mut cur = i;
        for part in path.split('/').filter(|p| !p.is_empty()) {
            cur = self.child(cur, part)?;
        }
        Some(cur)
    }

    /// Text of a child path, trimmed.
    pub fn text_at(&self, i: usize, path: &str) -> Option<&str> {
        self.path(i, path).map(|n| self.text(n))
    }

    /// Element path from the root, for diagnostics.
    pub fn location(&self, i: usize) -> String {
        let mut parts = Vec::new();
        let mut cur = Some(i);
        while let Some(c) = cur {
            parts.push(self.nodes[c].name.clone());
            cur = self.nodes[c].parent;
        }
        parts.reverse();
        parts.join("/")
    }

    /// Serializes a subtree back to XML text (for keeping unmapped data verbatim).
    pub fn subtree_xml(&self, i: usize, max_len: usize) -> Option<String> {
        let mut out = String::new();
        self.write_node(i, &mut out, max_len)?;
        Some(out)
    }

    fn write_node(&self, i: usize, out: &mut String, max_len: usize) -> Option<()> {
        let n = &self.nodes[i];
        let _ = write!(out, "<{}", n.name);
        for (k, v) in &n.attrs {
            let _ = write!(out, " {k}=\"{}\"", escape(v));
        }
        if n.children.is_empty() && n.text.is_empty() {
            out.push_str("/>");
        } else {
            out.push('>');
            out.push_str(&escape(&n.text));
            for c in &n.children {
                self.write_node(*c, out, max_len)?;
            }
            let _ = write!(out, "</{}>", n.name);
        }
        (out.len() <= max_len).then_some(())
    }

    pub fn depth(&self, i: usize) -> usize {
        let mut d = 0;
        let mut cur = self.nodes[i].parent;
        while let Some(c) = cur {
            d += 1;
            cur = self.nodes[c].parent;
        }
        d
    }
}

fn resolve_entity(name: &str) -> Result<char, XmlError> {
    Ok(match name {
        "lt" => '<',
        "gt" => '>',
        "amp" => '&',
        "apos" => '\'',
        "quot" => '"',
        _ => {
            let num = name
                .strip_prefix('#')
                .ok_or_else(|| XmlError::Entity(name.to_string()))?;
            let v = if let Some(hex) = num.strip_prefix('x') {
                u32::from_str_radix(hex, 16)
            } else {
                num.parse::<u32>()
            }
            .map_err(|_| XmlError::Entity(name.to_string()))?;
            char::from_u32(v)
                .filter(|c| *c != '\0')
                .ok_or_else(|| XmlError::Entity(name.to_string()))?
        }
    })
}

/// Parses a UTF-8 document.
pub fn parse(text: &str, limits: Limits) -> Result<Doc, XmlError> {
    let mut reader = quick_xml::Reader::from_str(text);
    let cfg = reader.config_mut();
    cfg.check_end_names = true;
    cfg.expand_empty_elements = false;
    let mut nodes: Vec<Node> = Vec::new();
    let mut stack: Vec<usize> = Vec::new();
    let mut total_text = 0usize;
    let syntax = |e: quick_xml::Error| XmlError::Syntax(e.to_string());
    let mut root_done = false;
    loop {
        let ev = reader.read_event().map_err(syntax)?;
        match ev {
            Event::DocType(d) => {
                // a bare `<!DOCTYPE name>` declares nothing and is accepted; any internal subset
                // or external identifier is refused
                let body: &str = &d;
                let body = body.trim();
                if body.contains('[')
                    || body.contains("SYSTEM")
                    || body.contains("PUBLIC")
                    || body.contains(char::is_whitespace)
                {
                    return Err(XmlError::Dtd);
                }
            }
            Event::Start(_) | Event::Empty(_) => {
                let (e, empty) = match ev {
                    Event::Start(e) => (e, false),
                    Event::Empty(e) => (e, true),
                    _ => unreachable!(),
                };
                if stack.is_empty() && root_done {
                    return Err(XmlError::Syntax("more than one root element".into()));
                }
                if nodes.len() >= limits.max_nodes {
                    return Err(XmlError::Limit("element count"));
                }
                if stack.len() >= limits.max_depth {
                    return Err(XmlError::Limit("nesting depth"));
                }
                let name = e.name().as_ref().to_string();
                let mut attrs = Vec::new();
                for a in e.attributes() {
                    let a = a.map_err(|e| XmlError::Syntax(e.to_string()))?;
                    let key = a.key.as_ref().to_string();
                    let value = a
                        .normalized_value(XmlVersion::Implicit1_0)
                        .map_err(|e| XmlError::Syntax(e.to_string()))?
                        .into_owned();
                    if value.len() > limits.max_attr_len {
                        return Err(XmlError::Limit("attribute length"));
                    }
                    attrs.push((key, value));
                }
                let id = nodes.len();
                let parent = stack.last().copied();
                nodes.push(Node {
                    name,
                    attrs,
                    children: Vec::new(),
                    text: String::new(),
                    parent,
                });
                if let Some(p) = parent {
                    nodes[p].children.push(id);
                }
                if empty {
                    if stack.is_empty() {
                        root_done = true;
                    }
                } else {
                    stack.push(id);
                }
            }
            Event::End(_) => {
                stack.pop();
                if stack.is_empty() {
                    root_done = true;
                }
            }
            Event::Text(t) => {
                if let Some(&cur) = stack.last() {
                    let s = t.xml10_content();
                    total_text += s.len();
                    if nodes[cur].text.len() + s.len() > limits.max_text_len
                        || total_text > limits.max_text_len * 4
                    {
                        return Err(XmlError::Limit("text length"));
                    }
                    nodes[cur].text.push_str(&s);
                }
            }
            Event::CData(t) => {
                if let Some(&cur) = stack.last() {
                    let s: &str = &t;
                    if nodes[cur].text.len() + s.len() > limits.max_text_len {
                        return Err(XmlError::Limit("text length"));
                    }
                    nodes[cur].text.push_str(s);
                }
            }
            Event::GeneralRef(r) => {
                let c = resolve_entity(&r)?;
                if let Some(&cur) = stack.last() {
                    nodes[cur].text.push(c);
                }
            }
            Event::Comment(_) | Event::Decl(_) | Event::PI(_) => {}
            Event::Eof => break,
        }
    }
    if nodes.is_empty() {
        return Err(XmlError::Empty);
    }
    if !stack.is_empty() {
        return Err(XmlError::Syntax("unclosed element".into()));
    }
    Ok(Doc { nodes })
}

/// Escapes text for element content and attribute values.
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c if (c as u32) < 0x20 && !matches!(c, '\n' | '\r' | '\t') => {}
            c => out.push(c),
        }
    }
    out
}

/// Minimal indenting XML writer.
#[derive(Default)]
pub struct Writer {
    out: String,
    stack: Vec<String>,
}

impl Writer {
    pub fn new() -> Writer {
        let mut w = Writer::default();
        w.out
            .push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        w
    }

    fn indent(&mut self) {
        for _ in 0..self.stack.len() {
            self.out.push_str("  ");
        }
    }

    pub fn open(&mut self, name: &str, attrs: &[(&str, &str)]) {
        self.indent();
        let _ = write!(self.out, "<{name}");
        for (k, v) in attrs {
            let _ = write!(self.out, " {k}=\"{}\"", escape(v));
        }
        self.out.push_str(">\n");
        self.stack.push(name.to_string());
    }

    pub fn close(&mut self) {
        if let Some(name) = self.stack.pop() {
            self.indent();
            let _ = writeln!(self.out, "</{name}>");
        }
    }

    pub fn leaf(&mut self, name: &str, text: &str) {
        self.indent();
        let _ = writeln!(self.out, "<{name}>{}</{name}>", escape(text));
    }

    pub fn empty(&mut self, name: &str, attrs: &[(&str, &str)]) {
        self.indent();
        let _ = write!(self.out, "<{name}");
        for (k, v) in attrs {
            let _ = write!(self.out, " {k}=\"{}\"", escape(v));
        }
        self.out.push_str("/>\n");
    }

    pub fn raw(&mut self, text: &str) {
        self.out.push_str(text);
    }

    pub fn finish(mut self) -> String {
        while !self.stack.is_empty() {
            self.close();
        }
        self.out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tree_text_and_entities() {
        let d = parse(
            "<?xml version=\"1.0\"?><a x=\"1&amp;2\"><b>t&lt;u&#x41;</b><c/></a>",
            Limits::default(),
        )
        .unwrap();
        assert_eq!(d.attr(0, "x"), Some("1&2"));
        assert_eq!(d.text_at(0, "b"), Some("t<uA"));
        assert!(d.child(0, "c").is_some());
        assert_eq!(d.location(d.path(0, "b").unwrap()), "a/b");
    }

    #[test]
    fn hostile_documents_are_refused() {
        assert_eq!(
            parse(
                "<!DOCTYPE a [<!ENTITY e \"x\">]><a>&e;</a>",
                Limits::default()
            ),
            Err(XmlError::Dtd)
        );
        assert_eq!(
            parse(
                "<!DOCTYPE a SYSTEM \"http://x/a.dtd\"><a/>",
                Limits::default()
            ),
            Err(XmlError::Dtd)
        );
        assert!(parse("<!DOCTYPE xmeml><xmeml/>", Limits::default()).is_ok());
        assert!(matches!(
            parse("<a>&e;</a>", Limits::default()),
            Err(XmlError::Entity(_))
        ));
        let deep = "<a>".repeat(300) + &"</a>".repeat(300);
        assert_eq!(
            parse(&deep, Limits::default()),
            Err(XmlError::Limit("nesting depth"))
        );
        assert!(parse("<a><b></a>", Limits::default()).is_err());
        assert!(parse("<a/><b/>", Limits::default()).is_err());
    }

    #[test]
    fn writer_escapes() {
        let mut w = Writer::new();
        w.open("a", &[("k", "<\"&")]);
        w.leaf("b", "x & y");
        let s = w.finish();
        let d = parse(&s, Limits::default()).unwrap();
        assert_eq!(d.attr(0, "k"), Some("<\"&"));
        assert_eq!(d.text_at(0, "b"), Some("x & y"));
    }
}

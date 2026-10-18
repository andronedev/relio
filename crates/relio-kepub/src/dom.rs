//! Arena DOM built by html5ever with kepubify's parser leniency, and a port of
//! its polyglot XHTML renderer (golang.org/x/net/html fork).

use std::borrow::Cow;
use std::cell::{Ref, RefCell};

use html5ever::buffer_queue::BufferQueue;
use html5ever::interface::{ElementFlags, NodeOrText, QuirksMode, TreeSink};
use html5ever::tendril::StrTendril;
use html5ever::tokenizer::{
    Tag, TagKind, Token, TokenSink, TokenSinkResult, Tokenizer, TokenizerOpts,
};
use html5ever::tree_builder::{TreeBuilder, TreeBuilderOpts};
use html5ever::{Attribute, LocalName, QualName, TokenizerResult, ns};

pub(crate) type NodeId = usize;

pub(crate) const DOCUMENT: NodeId = 0;

#[derive(Debug, Clone)]
pub(crate) enum Data {
    Document,
    Doctype {
        name: String,
        public: String,
        system: String,
    },
    Text(String),
    Comment(String),
    Element {
        name: QualName,
        attrs: Vec<Attr>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Attr {
    /// Namespace prefix of adjusted foreign attributes (`xlink`, `xml`, `xmlns`).
    pub prefix: String,
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone)]
pub(crate) struct Node {
    pub data: Data,
    pub parent: Option<NodeId>,
    pub first_child: Option<NodeId>,
    pub last_child: Option<NodeId>,
    pub prev: Option<NodeId>,
    pub next: Option<NodeId>,
}

#[derive(Debug, Clone)]
pub(crate) struct Dom {
    nodes: Vec<Node>,
    names: Vec<QualName>,
}

impl Dom {
    fn new() -> Self {
        let mut dom = Self {
            nodes: Vec::new(),
            names: Vec::new(),
        };
        dom.push(Data::Document);
        dom
    }

    fn push(&mut self, data: Data) -> NodeId {
        let name = match &data {
            Data::Element { name, .. } => name.clone(),
            _ => QualName::new(None, ns!(), LocalName::from("")),
        };
        self.names.push(name);
        self.nodes.push(Node {
            data,
            parent: None,
            first_child: None,
            last_child: None,
            prev: None,
            next: None,
        });
        self.nodes.len() - 1
    }

    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id]
    }

    pub fn data_mut(&mut self, id: NodeId) -> &mut Data {
        &mut self.nodes[id].data
    }

    pub fn new_element(&mut self, local: &str, attrs: &[(&str, &str)]) -> NodeId {
        let attrs = attrs
            .iter()
            .map(|(key, value)| Attr {
                prefix: String::new(),
                key: (*key).to_owned(),
                value: (*value).to_owned(),
            })
            .collect();
        self.push(Data::Element {
            name: QualName::new(None, ns!(html), LocalName::from(local)),
            attrs,
        })
    }

    pub fn new_text(&mut self, text: &str) -> NodeId {
        self.push(Data::Text(text.to_owned()))
    }

    pub fn duplicate(&mut self, id: NodeId) -> NodeId {
        let data = self.nodes[id].data.clone();
        self.push(data)
    }

    /// Local element name, or `None` for other node types.
    pub fn name(&self, id: NodeId) -> Option<&str> {
        match &self.nodes[id].data {
            Data::Element { name, .. } => Some(&name.local),
            _ => None,
        }
    }

    pub fn attr(&self, id: NodeId, key: &str) -> Option<&str> {
        match &self.nodes[id].data {
            Data::Element { attrs, .. } => attrs
                .iter()
                .find(|a| a.key == key)
                .map(|a| a.value.as_str()),
            _ => None,
        }
    }

    pub fn children(&self, id: NodeId) -> Vec<NodeId> {
        let mut children = Vec::new();
        let mut child = self.nodes[id].first_child;
        while let Some(c) = child {
            children.push(c);
            child = self.nodes[c].next;
        }
        children
    }

    /// First element in document order with this local name.
    pub fn find(&self, from: NodeId, local: &str) -> Option<NodeId> {
        self.children(from).into_iter().find_map(|c| {
            if self.name(c) == Some(local) {
                Some(c)
            } else {
                self.find(c, local)
            }
        })
    }

    pub fn detach(&mut self, id: NodeId) {
        let Node {
            parent, prev, next, ..
        } = self.nodes[id];
        if let Some(parent) = parent {
            if self.nodes[parent].first_child == Some(id) {
                self.nodes[parent].first_child = next;
            }
            if self.nodes[parent].last_child == Some(id) {
                self.nodes[parent].last_child = prev;
            }
        }
        if let Some(prev) = prev {
            self.nodes[prev].next = next;
        }
        if let Some(next) = next {
            self.nodes[next].prev = prev;
        }
        let node = &mut self.nodes[id];
        node.parent = None;
        node.prev = None;
        node.next = None;
    }

    pub fn append(&mut self, parent: NodeId, child: NodeId) {
        self.detach(child);
        let last = self.nodes[parent].last_child;
        self.nodes[child].parent = Some(parent);
        self.nodes[child].prev = last;
        match last {
            Some(last) => self.nodes[last].next = Some(child),
            None => self.nodes[parent].first_child = Some(child),
        }
        self.nodes[parent].last_child = Some(child);
    }

    pub fn insert_before(&mut self, sibling: NodeId, child: NodeId) {
        self.detach(child);
        let parent = self.nodes[sibling].parent.expect("sibling has a parent");
        let prev = self.nodes[sibling].prev;
        self.nodes[child].parent = Some(parent);
        self.nodes[child].prev = prev;
        self.nodes[child].next = Some(sibling);
        self.nodes[sibling].prev = Some(child);
        match prev {
            Some(prev) => self.nodes[prev].next = Some(child),
            None => self.nodes[parent].first_child = Some(child),
        }
    }

    fn append_text(&mut self, parent: NodeId, text: &str) {
        if let Some(last) = self.nodes[parent].last_child
            && let Data::Text(existing) = &mut self.nodes[last].data
        {
            existing.push_str(text);
            return;
        }
        let node = self.new_text(text);
        self.append(parent, node);
    }

    fn insert_text_before(&mut self, sibling: NodeId, text: &str) {
        if let Some(prev) = self.nodes[sibling].prev
            && let Data::Text(existing) = &mut self.nodes[prev].data
        {
            existing.push_str(text);
            return;
        }
        let node = self.new_text(text);
        self.insert_before(sibling, node);
    }
}

pub(crate) fn parse(html: &str) -> Dom {
    let html = protect_duplicate_attributes(html.strip_prefix('\u{feff}').unwrap_or(html));
    let opts = TreeBuilderOpts {
        scripting_enabled: true,
        ..TreeBuilderOpts::default()
    };
    let tokenizer = Tokenizer::new(
        Lenient(TreeBuilder::new(Sink::default(), opts)),
        TokenizerOpts::default(),
    );
    let input = BufferQueue::default();
    input.push_back(StrTendril::from(&*html));
    while !matches!(tokenizer.feed(&input), TokenizerResult::Done) {}
    tokenizer.end();
    tokenizer.sink.0.sink.dom.into_inner()
}

/// kepubify's `ParseOptionLenientSelfClosing`: `<a/>`, `<p/>`, `<div/>`,
/// `<span/>`, `<title/>` and `<script/>` are closed immediately instead of
/// swallowing what follows.
struct Lenient<S>(S);

const LENIENT: &[&str] = &["a", "p", "div", "span", "title", "script"];

impl<S: TokenSink> TokenSink for Lenient<S> {
    type Handle = S::Handle;

    fn process_token(&self, token: Token, line: u64) -> TokenSinkResult<S::Handle> {
        match token {
            Token::TagToken(tag)
                if tag.kind == TagKind::StartTag
                    && tag.self_closing
                    && LENIENT.contains(&&*tag.name) =>
            {
                let end = Tag {
                    kind: TagKind::EndTag,
                    self_closing: false,
                    attrs: Vec::new(),
                    ..tag.clone()
                };
                let start = Tag {
                    self_closing: false,
                    ..tag
                };
                // Raw-text switches and script pauses requested for the start tag are dropped:
                // the element is already closed.
                let _ = self.0.process_token(Token::TagToken(start), line);
                let _ = self.0.process_token(Token::TagToken(end), line);
                TokenSinkResult::Continue
            }
            token => self.0.process_token(token, line),
        }
    }

    fn end(&self) {
        self.0.end();
    }

    fn adjusted_current_node_present_but_not_in_html_namespace(&self) -> bool {
        self.0
            .adjusted_current_node_present_but_not_in_html_namespace()
    }
}

struct Sink {
    dom: RefCell<Dom>,
}

impl Default for Sink {
    fn default() -> Self {
        Self {
            dom: RefCell::new(Dom::new()),
        }
    }
}

fn convert_attrs(attrs: Vec<Attribute>) -> Vec<Attr> {
    attrs
        .into_iter()
        .map(|a| Attr {
            prefix: a.name.prefix.map(|p| p.to_string()).unwrap_or_default(),
            key: restore_duplicate_name(&a.name.local).to_owned(),
            value: a.value.to_string(),
        })
        .collect()
}

/// Marks renamed duplicate attributes: `MARK index MARK name`.
const DUPLICATE_MARK: char = '\u{e000}';
const RAW_TEXT: &[&str] = &[
    "script",
    "style",
    "textarea",
    "title",
    "xmp",
    "iframe",
    "noembed",
    "noframes",
    "noscript",
    "plaintext",
];

fn restore_duplicate_name(name: &str) -> &str {
    match name.strip_prefix(DUPLICATE_MARK) {
        Some(rest) => rest
            .split_once(DUPLICATE_MARK)
            .map_or(name, |(_, original)| original),
        None => name,
    }
}

/// Go's tokenizer keeps repeated attributes where html5ever drops them, so
/// repeats are renamed before parsing and restored when building the DOM.
fn protect_duplicate_attributes(html: &str) -> Cow<'_, str> {
    let mut renames: Vec<std::ops::Range<usize>> = Vec::new();
    let bytes = html.as_bytes();
    let mut i = 0;
    while let Some(offset) = html[i..].find('<') {
        let at = i + offset;
        let rest = &html[at..];
        let skip_to = |end: &str| rest.find(end).map_or(html.len(), |e| at + e + end.len());
        if rest.starts_with("<!--") {
            i = skip_to("-->");
            continue;
        }
        if rest.starts_with("<!") || rest.starts_with("<?") || rest.starts_with("</") {
            i = skip_to(">");
            continue;
        }
        if !rest[1..].starts_with(|c: char| c.is_ascii_alphabetic()) {
            i = at + 1;
            continue;
        }
        let name_end = rest
            .find(|c: char| c.is_ascii_whitespace() || c == '/' || c == '>')
            .unwrap_or(rest.len());
        let tag = rest[1..name_end].to_ascii_lowercase();
        let mut j = at + name_end;
        let mut seen: Vec<String> = Vec::new();
        while j < bytes.len() && bytes[j] != b'>' {
            if bytes[j].is_ascii_whitespace() || bytes[j] == b'/' {
                j += 1;
                continue;
            }
            let start = j;
            j += 1;
            while j < bytes.len()
                && !bytes[j].is_ascii_whitespace()
                && !matches!(bytes[j], b'/' | b'>' | b'=')
            {
                j += 1;
            }
            let name = html[start..j].to_ascii_lowercase();
            if seen.contains(&name) {
                renames.push(start..j);
            } else {
                seen.push(name);
            }
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'=' {
                j += 1;
                while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
                match bytes.get(j) {
                    Some(&quote @ (b'"' | b'\'')) => {
                        j = html[j + 1..]
                            .find(quote as char)
                            .map_or(html.len(), |e| j + 1 + e + 1);
                    }
                    _ => {
                        while j < bytes.len() && !bytes[j].is_ascii_whitespace() && bytes[j] != b'>'
                        {
                            j += 1;
                        }
                    }
                }
            }
        }
        let self_closing = j > 0 && bytes[j - 1] == b'/';
        i = (j + 1).min(html.len());
        let lenient_closed = self_closing && LENIENT.contains(&tag.as_str());
        if RAW_TEXT.contains(&tag.as_str()) && !lenient_closed {
            let close = format!("</{tag}");
            i = bytes[i..]
                .windows(close.len())
                .position(|w| w.eq_ignore_ascii_case(close.as_bytes()))
                .map_or(html.len(), |p| i + p);
        }
    }
    if renames.is_empty() {
        return Cow::Borrowed(html);
    }
    let mut out = String::with_capacity(html.len() + renames.len() * 8);
    let mut copied = 0;
    for (n, range) in renames.into_iter().enumerate() {
        out.push_str(&html[copied..range.start]);
        out.push_str(&format!("{DUPLICATE_MARK}{n}{DUPLICATE_MARK}"));
        out.push_str(&html[range.clone()]);
        copied = range.end;
    }
    out.push_str(&html[copied..]);
    Cow::Owned(out)
}

impl TreeSink for Sink {
    type Handle = NodeId;
    type Output = Self;
    type ElemName<'a> = Ref<'a, QualName>;

    fn finish(self) -> Self {
        self
    }

    fn parse_error(&self, _msg: Cow<'static, str>) {}

    fn get_document(&self) -> NodeId {
        DOCUMENT
    }

    fn elem_name<'a>(&'a self, target: &'a NodeId) -> Ref<'a, QualName> {
        Ref::map(self.dom.borrow(), |dom| &dom.names[*target])
    }

    fn create_element(
        &self,
        name: QualName,
        attrs: Vec<Attribute>,
        _flags: ElementFlags,
    ) -> NodeId {
        self.dom.borrow_mut().push(Data::Element {
            name,
            attrs: convert_attrs(attrs),
        })
    }

    fn create_comment(&self, text: StrTendril) -> NodeId {
        self.dom.borrow_mut().push(Data::Comment(text.to_string()))
    }

    fn create_pi(&self, target: StrTendril, data: StrTendril) -> NodeId {
        self.dom
            .borrow_mut()
            .push(Data::Comment(format!("?{target} {data}")))
    }

    fn append(&self, parent: &NodeId, child: NodeOrText<NodeId>) {
        let mut dom = self.dom.borrow_mut();
        match child {
            NodeOrText::AppendNode(node) => dom.append(*parent, node),
            NodeOrText::AppendText(text) => dom.append_text(*parent, &text),
        }
    }

    fn append_based_on_parent_node(
        &self,
        element: &NodeId,
        prev_element: &NodeId,
        child: NodeOrText<NodeId>,
    ) {
        if self.dom.borrow().nodes[*element].parent.is_some() {
            self.append_before_sibling(element, child);
        } else {
            self.append(prev_element, child);
        }
    }

    fn append_doctype_to_document(
        &self,
        name: StrTendril,
        public_id: StrTendril,
        system_id: StrTendril,
    ) {
        let mut dom = self.dom.borrow_mut();
        let doctype = dom.push(Data::Doctype {
            name: name.to_string(),
            public: public_id.to_string(),
            system: system_id.to_string(),
        });
        dom.append(DOCUMENT, doctype);
    }

    fn get_template_contents(&self, target: &NodeId) -> NodeId {
        *target
    }

    fn same_node(&self, x: &NodeId, y: &NodeId) -> bool {
        x == y
    }

    fn set_quirks_mode(&self, _mode: QuirksMode) {}

    fn append_before_sibling(&self, sibling: &NodeId, new_node: NodeOrText<NodeId>) {
        let mut dom = self.dom.borrow_mut();
        match new_node {
            NodeOrText::AppendNode(node) => dom.insert_before(*sibling, node),
            NodeOrText::AppendText(text) => dom.insert_text_before(*sibling, &text),
        }
    }

    fn add_attrs_if_missing(&self, target: &NodeId, attrs: Vec<Attribute>) {
        let mut dom = self.dom.borrow_mut();
        if let Data::Element {
            attrs: existing, ..
        } = dom.data_mut(*target)
        {
            for attr in convert_attrs(attrs) {
                if !existing
                    .iter()
                    .any(|a| a.prefix == attr.prefix && a.key == attr.key)
                {
                    existing.push(attr);
                }
            }
        }
    }

    fn remove_from_parent(&self, target: &NodeId) {
        self.dom.borrow_mut().detach(*target);
    }

    fn reparent_children(&self, node: &NodeId, new_parent: &NodeId) {
        let mut dom = self.dom.borrow_mut();
        for child in dom.children(*node) {
            dom.append(*new_parent, child);
        }
    }
}

const VOID: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "keygen", "link", "meta", "param",
    "source", "track", "wbr",
];
const RAW_CHILDREN: &[&str] = &[
    "iframe",
    "noembed",
    "noframes",
    "noscript",
    "plaintext",
    "script",
    "style",
    "xmp",
];

/// Renders with `RenderOptionAllowXMLDeclarations` and `RenderOptionPolyglot`.
pub(crate) fn render(dom: &Dom) -> String {
    let mut out = String::new();
    render_node(dom, DOCUMENT, &mut out);
    out
}

/// Returns false once a `plaintext` element was rendered: nothing may follow.
fn render_node(dom: &Dom, id: NodeId, out: &mut String) -> bool {
    match &dom.node(id).data {
        Data::Text(text) => {
            let mut parts = text.split('\u{a0}');
            escape(out, parts.next().unwrap_or_default());
            for part in parts {
                out.push_str("&#160;");
                escape(out, part);
            }
            true
        }
        Data::Document => dom
            .children(id)
            .into_iter()
            .all(|c| render_node(dom, c, out)),
        Data::Comment(data) => {
            if data.starts_with("?xml ") && data.ends_with('?') {
                out.push('<');
                out.push_str(data);
                out.push('>');
            } else {
                out.push_str("<!--");
                out.push_str(data);
                out.push_str("-->");
            }
            true
        }
        Data::Doctype {
            name,
            public,
            system,
        } => {
            out.push_str("<!DOCTYPE ");
            out.push_str(name);
            if !public.is_empty() {
                out.push_str(" PUBLIC ");
                quoted(out, public);
                if !system.is_empty() {
                    out.push(' ');
                    quoted(out, system);
                }
            } else if !system.is_empty() {
                out.push_str(" SYSTEM ");
                quoted(out, system);
            }
            out.push('>');
            true
        }
        Data::Element { name, attrs } => render_element(dom, id, &name.local, attrs, out),
    }
}

fn render_element(dom: &Dom, id: NodeId, name: &str, attrs: &[Attr], out: &mut String) -> bool {
    out.push('<');
    out.push_str(name);

    let mut attrs = Cow::Borrowed(attrs);
    let ensure = |attrs: &mut Cow<[Attr]>, prefix: &str, key: &str, value: &str| {
        let present = attrs.iter().any(|a| {
            (a.prefix.eq_ignore_ascii_case(prefix) && a.key.eq_ignore_ascii_case(key))
                || a.prefix.eq_ignore_ascii_case(key)
        });
        if !present {
            attrs.to_mut().push(Attr {
                prefix: prefix.to_owned(),
                key: key.to_owned(),
                value: value.to_owned(),
            });
        }
    };
    match name {
        "html" => ensure(&mut attrs, "", "xmlns", "http://www.w3.org/1999/xhtml"),
        "svg" => {
            ensure(&mut attrs, "", "xmlns", "http://www.w3.org/2000/svg");
            ensure(&mut attrs, "xmlns", "xlink", "http://www.w3.org/1999/xlink");
        }
        "math" => ensure(
            &mut attrs,
            "",
            "xmlns",
            "http://www.w3.org/1998/Math/MathML",
        ),
        "script" => ensure(&mut attrs, "", "type", "text/javascript"),
        "style" => ensure(&mut attrs, "", "type", "text/css"),
        _ => {}
    }
    for attr in attrs.iter() {
        out.push(' ');
        if !attr.prefix.is_empty() {
            out.push_str(&attr.prefix);
            out.push(':');
        }
        out.push_str(&attr.key);
        out.push_str("=\"");
        escape(out, &attr.value);
        out.push('"');
    }

    if VOID.contains(&name) {
        out.push_str("/>");
        return true;
    }
    out.push('>');

    let children = dom.children(id);
    if let Some(&first) = children.first()
        && matches!(&dom.node(first).data, Data::Text(t) if t.starts_with('\n'))
        && matches!(name, "pre" | "listing" | "textarea")
    {
        out.push('\n');
    }
    let raw = RAW_CHILDREN.contains(&name);
    for child in children {
        match &dom.node(child).data {
            Data::Text(text) if raw => out.push_str(text),
            _ => {
                if !render_node(dom, child, out) {
                    return false;
                }
            }
        }
    }
    if name == "plaintext" {
        return false;
    }
    out.push_str("</");
    out.push_str(name);
    out.push('>');
    true
}

fn escape(out: &mut String, text: &str) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '\'' => out.push_str("&#39;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&#34;"),
            '\r' => out.push_str("&#13;"),
            c => out.push(c),
        }
    }
}

fn quoted(out: &mut String, value: &str) {
    let quote = if value.contains('"') { '\'' } else { '"' };
    out.push(quote);
    out.push_str(value);
    out.push(quote);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(html: &str) -> String {
        render(&parse(html))
    }

    #[test]
    fn renders_polyglot_xhtml() {
        assert_eq!(
            roundtrip(
                "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<!DOCTYPE html>\n<html><head><title/><style>p{}</style></head><body><p>a&nbsp;b's <br> \"c\"</p></body></html>"
            ),
            "<?xml version=\"1.0\" encoding=\"utf-8\"?><!DOCTYPE html><html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title></title><style type=\"text/css\">p{}</style></head><body><p>a&#160;b&#39;s <br/> &#34;c&#34;</p></body></html>"
        );
    }

    #[test]
    fn closes_lenient_self_closing_tags() {
        assert_eq!(
            roundtrip("<body><a id=\"x\"/>text<div/><p/><span/><b/>bold</body>"),
            "<html xmlns=\"http://www.w3.org/1999/xhtml\"><head></head><body><a id=\"x\"></a>text<div></div><p></p><span></span><b>bold</b></body></html>"
        );
    }

    #[test]
    fn renders_foreign_content() {
        assert_eq!(
            roundtrip("<body><svg viewBox=\"0 0 1 1\"><image xlink:href=\"a.png\"/></svg></body>"),
            "<html xmlns=\"http://www.w3.org/1999/xhtml\"><head></head><body><svg viewBox=\"0 0 1 1\" xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\"><image xlink:href=\"a.png\"></image></svg></body></html>"
        );
    }
}

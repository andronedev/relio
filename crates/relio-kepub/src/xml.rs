//! Minimal port of github.com/beevik/etree v1.1.0 on top of Go's strict
//! encoding/xml raw tokenizer: just what kepubify needs to rewrite the OPF.

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Item {
    Element(Element),
    CharData { data: String, whitespace: bool },
    Comment(String),
    Directive(String),
    ProcInst { target: String, inst: String },
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct Element {
    pub space: String,
    pub tag: String,
    pub attrs: Vec<Attr>,
    pub children: Vec<Item>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Attr {
    pub space: String,
    pub key: String,
    pub value: String,
}

#[derive(Debug)]
pub(crate) struct SyntaxError;

impl Element {
    pub fn new(space: &str, tag: &str) -> Self {
        Self {
            space: space.to_owned(),
            tag: tag.to_owned(),
            ..Self::default()
        }
    }

    pub fn attr(&self, key: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|a| a.key == key)
            .map(|a| a.value.as_str())
    }

    /// etree's `CreateAttr`: replaces the value of an existing attribute.
    pub fn set_attr(&mut self, space: &str, key: &str, value: &str) {
        match self
            .attrs
            .iter_mut()
            .find(|a| a.space == space && a.key == key)
        {
            Some(attr) => attr.value = value.to_owned(),
            None => self.attrs.push(Attr {
                space: space.to_owned(),
                key: key.to_owned(),
                value: value.to_owned(),
            }),
        }
    }

    pub fn child_elements(&self) -> impl Iterator<Item = &Element> {
        self.children.iter().filter_map(|c| match c {
            Item::Element(e) => Some(e),
            _ => None,
        })
    }

    pub fn child_mut(&mut self, tag: &str) -> Option<&mut Element> {
        self.children.iter_mut().find_map(|c| match c {
            Item::Element(e) if e.tag == tag => Some(e),
            _ => None,
        })
    }

    /// Elements in breadth-first order, like etree's `//` selector.
    pub fn descendants(&self) -> Vec<&Element> {
        let mut queue = std::collections::VecDeque::from([self]);
        let mut out = Vec::new();
        while let Some(e) = queue.pop_front() {
            out.push(e);
            queue.extend(e.child_elements());
        }
        out
    }

    /// Mutable access by a breadth-first index from [`Element::descendants`].
    pub fn descendant_mut(&mut self, index: usize) -> Option<&mut Element> {
        let path = self.bfs_path(index)?;
        let mut cur = self;
        for i in path {
            let Item::Element(e) = &mut cur.children[i] else {
                unreachable!()
            };
            cur = e;
        }
        Some(cur)
    }

    fn bfs_path(&self, index: usize) -> Option<Vec<usize>> {
        let mut queue = std::collections::VecDeque::from([(self, Vec::new())]);
        let mut n = 0;
        while let Some((e, path)) = queue.pop_front() {
            if n == index {
                return Some(path);
            }
            n += 1;
            for (i, child) in e.children.iter().enumerate() {
                if let Item::Element(c) = child {
                    let mut p = path.clone();
                    p.push(i);
                    queue.push_back((c, p));
                }
            }
        }
        None
    }

    /// Removes descendant elements matching the predicate.
    pub fn remove_where(&mut self, matches: &dyn Fn(&Element) -> bool) {
        self.children
            .retain(|c| !matches!(c, Item::Element(e) if matches(e)));
        for child in &mut self.children {
            if let Item::Element(e) = child {
                e.remove_where(matches);
            }
        }
    }

    fn full_tag(&self) -> String {
        if self.space.is_empty() {
            self.tag.clone()
        } else {
            format!("{}:{}", self.space, self.tag)
        }
    }

    /// etree's `Indent`, applied to a document root (depth 0).
    pub fn indent(&mut self, spaces: usize) {
        self.indent_at(0, spaces);
    }

    fn indent_at(&mut self, depth: isize, spaces: usize) {
        self.children.retain(|c| {
            !matches!(
                c,
                Item::CharData {
                    whitespace: true,
                    ..
                }
            )
        });
        if self.children.is_empty() {
            return;
        }
        let newline = |depth: isize| {
            let n = depth * spaces as isize;
            if n < 0 {
                "\n".to_owned()
            } else {
                format!("\n{}", " ".repeat(n as usize))
            }
        };
        let old = std::mem::take(&mut self.children);
        let mut is_char_data = false;
        let mut first = true;
        for mut child in old {
            is_char_data = matches!(child, Item::CharData { .. });
            if !is_char_data {
                if !first || depth > 0 {
                    self.children.push(Item::CharData {
                        data: newline(depth),
                        whitespace: true,
                    });
                }
                first = false;
            }
            if let Item::Element(e) = &mut child {
                e.indent_at(depth + 1, spaces);
            }
            self.children.push(child);
        }
        if !is_char_data && (!first || depth > 0) {
            self.children.push(Item::CharData {
                data: newline(depth - 1),
                whitespace: true,
            });
        }
    }

    /// Serializes the children of a document root.
    pub fn write_document(&self) -> String {
        let mut out = String::new();
        for child in &self.children {
            write_item(child, &mut out);
        }
        out
    }
}

fn write_item(item: &Item, out: &mut String) {
    match item {
        Item::Element(e) => {
            out.push('<');
            out.push_str(&e.full_tag());
            for a in &e.attrs {
                out.push(' ');
                if !a.space.is_empty() {
                    out.push_str(&a.space);
                    out.push(':');
                }
                out.push_str(&a.key);
                out.push_str("=\"");
                escape(out, &a.value);
                out.push('"');
            }
            if e.children.is_empty() {
                out.push_str("/>");
            } else {
                out.push('>');
                for c in &e.children {
                    write_item(c, out);
                }
                out.push_str("</");
                out.push_str(&e.full_tag());
                out.push('>');
            }
        }
        Item::CharData { data, .. } => escape(out, data),
        Item::Comment(data) => {
            out.push_str("<!--");
            out.push_str(data);
            out.push_str("-->");
        }
        Item::Directive(data) => {
            out.push_str("<!");
            out.push_str(data);
            out.push('>');
        }
        Item::ProcInst { target, inst } => {
            out.push_str("<?");
            out.push_str(target);
            if !inst.is_empty() {
                out.push(' ');
                out.push_str(inst);
            }
            out.push_str("?>");
        }
    }
}

fn escape(out: &mut String, s: &str) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\'' => out.push_str("&apos;"),
            '"' => out.push_str("&quot;"),
            c if in_char_range(c) => out.push(c),
            _ => out.push('\u{fffd}'),
        }
    }
}

fn in_char_range(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}')
}

fn is_whitespace(s: &str) -> bool {
    s.bytes().all(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r'))
}

/// Parses a document into a root element holding the top-level items.
pub(crate) fn parse(src: &str) -> Result<Element, SyntaxError> {
    let mut stack = vec![Element::default()];
    let mut parser = Parser { src, pos: 0 };
    while let Some(token) = parser.next()? {
        match token {
            Raw::Start(element, self_closing) => {
                if self_closing {
                    stack
                        .last_mut()
                        .ok_or(SyntaxError)?
                        .children
                        .push(Item::Element(element));
                } else {
                    stack.push(element);
                }
            }
            Raw::End => {
                if stack.len() < 2 {
                    return Err(SyntaxError);
                }
                let element = stack.pop().expect("checked above");
                stack
                    .last_mut()
                    .expect("checked above")
                    .children
                    .push(Item::Element(element));
            }
            Raw::Item(item) => stack.last_mut().ok_or(SyntaxError)?.children.push(item),
        }
    }
    while stack.len() > 1 {
        let element = stack.pop().expect("loop condition");
        stack
            .last_mut()
            .expect("loop condition")
            .children
            .push(Item::Element(element));
    }
    Ok(stack.pop().expect("root"))
}

enum Raw {
    Start(Element, bool),
    End,
    Item(Item),
}

struct Parser<'a> {
    src: &'a str,
    pos: usize,
}

impl Parser<'_> {
    fn rest(&self) -> &str {
        &self.src[self.pos..]
    }

    fn take_until(&mut self, end: &str) -> Result<String, SyntaxError> {
        let len = self.rest().find(end).ok_or(SyntaxError)?;
        let taken = self.rest()[..len].to_owned();
        self.pos += len + end.len();
        Ok(taken)
    }

    fn skip_space(&mut self) {
        let rest = self.rest();
        self.pos += rest.len() - rest.trim_start_matches([' ', '\t', '\r', '\n']).len();
    }

    fn name(&mut self) -> Result<String, SyntaxError> {
        let rest = self.rest();
        let len = rest
            .find(|c: char| c.is_ascii_whitespace() || matches!(c, '/' | '>' | '=' | '?' | '<'))
            .unwrap_or(rest.len());
        if len == 0 {
            return Err(SyntaxError);
        }
        let name = rest[..len].to_owned();
        self.pos += len;
        Ok(name)
    }

    fn next(&mut self) -> Result<Option<Raw>, SyntaxError> {
        if self.pos >= self.src.len() {
            return Ok(None);
        }
        if !self.rest().starts_with('<') {
            let len = self.rest().find('<').unwrap_or(self.rest().len());
            let data = decode(&self.rest()[..len])?;
            self.pos += len;
            let whitespace = is_whitespace(&data);
            return Ok(Some(Raw::Item(Item::CharData { data, whitespace })));
        }
        let rest = self.rest();
        if rest.starts_with("<!--") {
            self.pos += 4;
            let data = self.take_until("-->")?;
            if data.contains("--") {
                return Err(SyntaxError);
            }
            return Ok(Some(Raw::Item(Item::Comment(data))));
        }
        if rest.starts_with("<![CDATA[") {
            self.pos += 9;
            let data = normalize_newlines(&self.take_until("]]>")?);
            let whitespace = is_whitespace(&data);
            return Ok(Some(Raw::Item(Item::CharData { data, whitespace })));
        }
        if rest.starts_with("<!") {
            self.pos += 2;
            return Ok(Some(Raw::Item(Item::Directive(self.directive()?))));
        }
        if rest.starts_with("<?") {
            self.pos += 2;
            let target = self.name()?;
            self.skip_space();
            let inst = self.take_until("?>")?;
            return Ok(Some(Raw::Item(Item::ProcInst { target, inst })));
        }
        if rest.starts_with("</") {
            self.pos += 2;
            self.name()?;
            self.skip_space();
            if !self.rest().starts_with('>') {
                return Err(SyntaxError);
            }
            self.pos += 1;
            return Ok(Some(Raw::End));
        }
        self.pos += 1;
        let (space, tag) = split_name(&self.name()?);
        let mut element = Element::new(&space, &tag);
        loop {
            self.skip_space();
            let rest = self.rest();
            if rest.starts_with("/>") {
                self.pos += 2;
                return Ok(Some(Raw::Start(element, true)));
            }
            if rest.starts_with('>') {
                self.pos += 1;
                return Ok(Some(Raw::Start(element, false)));
            }
            let (space, key) = split_name(&self.name()?);
            self.skip_space();
            if !self.rest().starts_with('=') {
                return Err(SyntaxError);
            }
            self.pos += 1;
            self.skip_space();
            let quote = self
                .rest()
                .chars()
                .next()
                .filter(|c| matches!(c, '"' | '\''))
                .ok_or(SyntaxError)?;
            self.pos += 1;
            let raw = self.take_until(&quote.to_string())?;
            if raw.contains('<') {
                return Err(SyntaxError);
            }
            element.set_attr(&space, &key, &decode(&raw)?);
        }
    }

    fn directive(&mut self) -> Result<String, SyntaxError> {
        let mut depth = 0;
        let mut quote = None;
        for (i, c) in self.rest().char_indices() {
            match (quote, c) {
                (Some(q), c) if c == q => quote = None,
                (Some(_), _) => {}
                (None, '"' | '\'') => quote = Some(c),
                (None, '<') => depth += 1,
                (None, '>') if depth == 0 => {
                    let data = self.rest()[..i].to_owned();
                    self.pos += i + 1;
                    return Ok(data);
                }
                (None, '>') => depth -= 1,
                _ => {}
            }
        }
        Err(SyntaxError)
    }
}

fn split_name(name: &str) -> (String, String) {
    match name.split_once(':') {
        Some((space, local)) if !space.is_empty() && !local.is_empty() && !local.contains(':') => {
            (space.to_owned(), local.to_owned())
        }
        _ => (String::new(), name.to_owned()),
    }
}

fn normalize_newlines(s: &str) -> String {
    s.replace("\r\n", "\n").replace('\r', "\n")
}

fn decode(raw: &str) -> Result<String, SyntaxError> {
    let raw = normalize_newlines(raw);
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw.as_str();
    while let Some(pos) = rest.find('&') {
        out.push_str(&rest[..pos]);
        rest = &rest[pos + 1..];
        let end = rest.find(';').ok_or(SyntaxError)?;
        let name = &rest[..end];
        let c = match name {
            "lt" => '<',
            "gt" => '>',
            "amp" => '&',
            "apos" => '\'',
            "quot" => '"',
            _ => {
                let num = name.strip_prefix('#').ok_or(SyntaxError)?;
                let code = match num.strip_prefix('x') {
                    Some(hex) => u32::from_str_radix(hex, 16),
                    None => num.parse(),
                }
                .map_err(|_| SyntaxError)?;
                char::from_u32(code).ok_or(SyntaxError)?
            }
        };
        out.push(c);
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indents_like_etree() {
        let src = "<?xml version=\"1.0\"?>\n<package a='1 &amp; 2'><metadata>\n<dc:title>T &apos;x&apos;</dc:title><meta name=\"c\" /></metadata><spine/></package>\n";
        let mut doc = parse(src).unwrap();
        doc.indent(4);
        assert_eq!(
            doc.write_document(),
            "<?xml version=\"1.0\"?>\n<package a=\"1 &amp; 2\">\n    <metadata>\n        <dc:title>T &apos;x&apos;</dc:title>\n        <meta name=\"c\"/>\n    </metadata>\n    <spine/>\n</package>\n"
        );
    }

    #[test]
    fn rejects_malformed_xml() {
        assert!(parse("<a b=c/>").is_err());
        assert!(parse("<a>&nbsp;</a>").is_err());
    }
}

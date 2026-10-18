//! Smart punctuation, ported from github.com/kr/smartypants (LaTeX dashes) as
//! kepubify applies it to each text node.
//!
//! Like the original, the text is entity-escaped first and the rules run over
//! the escaped bytes, so `"` is only ever seen as `&quot;`; the result is
//! unescaped again.

pub(crate) fn smarten(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len() + 16);
    for c in text.chars() {
        match c {
            '"' => escaped.push_str("&quot;"),
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            c => escaped.push(c),
        }
    }
    let processed =
        String::from_utf8(apply(escaped.as_bytes())).expect("rules only replace ASCII with UTF-8");
    unescape(&processed)
}

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find('&') {
        out.push_str(&rest[..pos]);
        rest = &rest[pos..];
        let (decoded, len) = [
            ("&amp;", '&'),
            ("&lt;", '<'),
            ("&gt;", '>'),
            ("&quot;", '"'),
        ]
        .iter()
        .find(|(entity, _)| rest.starts_with(entity))
        .map_or(('&', 1), |(entity, c)| (*c, entity.len()));
        out.push(decoded);
        rest = &rest[len..];
    }
    out.push_str(rest);
    out
}

#[derive(Default)]
struct Quotes {
    single: bool,
    double: bool,
}

fn apply(p: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(p.len());
    let mut quotes = Quotes::default();
    let mut i = 0;
    while i < p.len() {
        let prev = if i > 0 { p[i - 1] } else { 0 };
        let text = &p[i..];
        let skip = match p[i] {
            b'&' => amp(&mut out, &mut quotes, prev, text),
            b'\'' => single_quote(&mut out, &mut quotes, prev, text),
            b'(' => parens(&mut out, text),
            b'-' => dash(&mut out, text),
            b'.' => period(&mut out, text),
            b'1' | b'3' => number(&mut out, prev, text),
            b'`' => backtick(&mut out, &mut quotes, prev, text),
            c => {
                out.push(c);
                0
            }
        };
        i += skip + 1;
    }
    out
}

fn is_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\r' | 0x0c | 0x0b)
}

fn is_punct(c: u8) -> bool {
    c.is_ascii_punctuation()
}

fn word_boundary(c: u8) -> bool {
    c == 0 || is_space(c) || is_punct(c)
}

fn at(text: &[u8], i: usize) -> u8 {
    text.get(i).copied().unwrap_or(0)
}

fn quote(out: &mut Vec<u8>, prev: u8, next: u8, double: bool, open: &mut bool) {
    *open = match () {
        _ if prev == 0 && next == 0 => !*open,
        _ if is_space(prev) && next == 0 => true,
        _ if next == 0 => false,
        _ if prev == 0 && is_space(next) => false,
        _ if is_space(prev) && is_space(next) => !*open,
        _ if is_space(next) => false,
        _ if prev == 0 && is_punct(next) => false,
        _ if is_space(prev) && is_punct(next) => true,
        _ if is_punct(prev) && is_punct(next) => !*open,
        _ if is_punct(next) => false,
        _ if prev == 0 || is_space(prev) || is_punct(prev) => true,
        _ => false,
    };
    let mark = match (double, *open) {
        (true, true) => "“",
        (true, false) => "”",
        (false, true) => "‘",
        (false, false) => "’",
    };
    out.extend_from_slice(mark.as_bytes());
}

fn single_quote(out: &mut Vec<u8>, quotes: &mut Quotes, prev: u8, text: &[u8]) -> usize {
    if text.len() >= 2 {
        let t1 = text[1].to_ascii_lowercase();
        if t1 == b'\'' {
            quote(out, prev, at(text, 2), true, &mut quotes.double);
            return 1;
        }
        if matches!(t1, b's' | b't' | b'm' | b'd') && (text.len() < 3 || word_boundary(text[2])) {
            out.extend_from_slice("’".as_bytes());
            return 0;
        }
        if text.len() >= 3 {
            let t2 = text[2].to_ascii_lowercase();
            let contraction = matches!((t1, t2), (b'r', b'e') | (b'l', b'l') | (b'v', b'e'));
            if contraction && (text.len() < 4 || word_boundary(text[3])) {
                out.extend_from_slice("’".as_bytes());
                return 0;
            }
        }
    }
    quote(out, prev, at(text, 1), false, &mut quotes.single);
    0
}

fn parens(out: &mut Vec<u8>, text: &[u8]) -> usize {
    if text.len() >= 3 {
        let t1 = text[1].to_ascii_lowercase();
        let t2 = text[2].to_ascii_lowercase();
        if t1 == b'c' && t2 == b')' {
            out.extend_from_slice("©".as_bytes());
            return 2;
        }
        if t1 == b'r' && t2 == b')' {
            out.extend_from_slice("®".as_bytes());
            return 2;
        }
        if text.len() >= 4 && t1 == b't' && t2 == b'm' && text[3] == b')' {
            out.extend_from_slice("™".as_bytes());
            return 3;
        }
    }
    out.push(b'(');
    0
}

fn dash(out: &mut Vec<u8>, text: &[u8]) -> usize {
    if text.starts_with(b"---") {
        out.extend_from_slice("—".as_bytes());
        return 2;
    }
    if text.starts_with(b"--") {
        out.extend_from_slice("–".as_bytes());
        return 1;
    }
    out.push(b'-');
    0
}

fn amp(out: &mut Vec<u8>, quotes: &mut Quotes, prev: u8, text: &[u8]) -> usize {
    if text.starts_with(b"&quot;") {
        quote(out, prev, at(text, 6), true, &mut quotes.double);
        return 5;
    }
    if text.starts_with(b"&#0;") {
        return 3;
    }
    out.push(b'&');
    0
}

fn period(out: &mut Vec<u8>, text: &[u8]) -> usize {
    if text.starts_with(b"...") {
        out.extend_from_slice("…".as_bytes());
        return 2;
    }
    if text.starts_with(b". . .") {
        out.extend_from_slice("…".as_bytes());
        return 4;
    }
    out.push(b'.');
    0
}

fn backtick(out: &mut Vec<u8>, quotes: &mut Quotes, prev: u8, text: &[u8]) -> usize {
    if text.len() >= 2 && text[1] == b'`' {
        quote(out, prev, at(text, 2), true, &mut quotes.double);
        return 1;
    }
    0
}

fn number(out: &mut Vec<u8>, prev: u8, text: &[u8]) -> usize {
    if word_boundary(prev) && text.len() >= 3 {
        let lower = |i: usize| at(text, i).to_ascii_lowercase();
        let ends = |i: usize| text.len() <= i || word_boundary(text[i]);
        let fraction = match &text[..3] {
            b"1/2" if ends(3) => Some("½"),
            b"1/4" if ends(3) || (text.len() >= 5 && lower(3) == b't' && lower(4) == b'h') => {
                Some("¼")
            }
            b"3/4"
                if ends(3)
                    || (text.len() >= 6
                        && lower(3) == b't'
                        && lower(4) == b'h'
                        && lower(5) == b's') =>
            {
                Some("¾")
            }
            _ => None,
        };
        if let Some(fraction) = fraction {
            out.extend_from_slice(fraction.as_bytes());
            return 2;
        }
    }
    out.push(text[0]);
    0
}

#[cfg(test)]
mod tests {
    use super::smarten;

    #[test]
    fn matches_smartypants() {
        assert_eq!(smarten(r#""Hello," she said."#), "“Hello,” she said.");
        assert_eq!(
            smarten("It's 'quoted' -- really --- yes..."),
            "It’s ‘quoted’ – really — yes…"
        );
        assert_eq!(smarten("(c) (R) (tm) 1/2 3/4ths 11/2"), "© ® ™ ½ ¾ths 11/2");
        assert_eq!(smarten("``Tex'' style"), "“Tex” style");
        assert_eq!(
            smarten("Tom & Jerry \"x\" a < b > c"),
            "Tom & Jerry “x” a < b > c"
        );
    }
}

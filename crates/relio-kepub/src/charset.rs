//! Content document decoding, mirroring kepubify's `--charset` handling.

use encoding_rs::{Encoding, UTF_8, UTF_16BE, UTF_16LE, WINDOWS_1252};

use std::cell::RefCell;

use html5ever::buffer_queue::BufferQueue;
use html5ever::tendril::StrTendril;
use html5ever::tokenizer::{TagKind, Token, TokenSink, TokenSinkResult, Tokenizer, TokenizerOpts};

/// Invalid UTF-8 bytes are carried through the transform as private-use
/// characters and written back unchanged, like Go strings do.
const INVALID_BASE: u32 = 0x10FF00;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Charset {
    /// Treat documents as UTF-8 (kepubify's default); invalid bytes are kept.
    #[default]
    Utf8,
    /// Detect from the byte order mark or `<meta>` declaration, else guess.
    Auto,
    /// Force a WHATWG encoding label such as `windows-1252`.
    Label(String),
}

pub(crate) struct Decoded {
    pub text: String,
    pub lossless_bytes: bool,
}

pub(crate) fn decode(bytes: &[u8], charset: &Charset) -> Result<Decoded, String> {
    let encoding = match charset {
        Charset::Utf8 => return Ok(decode_utf8(bytes)),
        Charset::Auto => detect(bytes),
        Charset::Label(label) => {
            Encoding::for_label(label.as_bytes()).ok_or_else(|| label.clone())?
        }
    };
    let (text, _) = encoding.decode_with_bom_removal(bytes);
    Ok(Decoded {
        text: text.into_owned(),
        lossless_bytes: false,
    })
}

/// Turns the transformed text back into bytes, restoring preserved invalid bytes.
pub(crate) fn encode(text: &str, lossless_bytes: bool) -> Vec<u8> {
    if !lossless_bytes {
        return text.as_bytes().to_vec();
    }
    let mut out = Vec::with_capacity(text.len());
    let mut buf = [0u8; 4];
    for c in text.chars() {
        match (c as u32).checked_sub(INVALID_BASE) {
            Some(byte) if byte <= 0xFF => out.push(byte as u8),
            _ => out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes()),
        }
    }
    out
}

pub(crate) fn decode_utf8(bytes: &[u8]) -> Decoded {
    if let Ok(text) = std::str::from_utf8(bytes) {
        return Decoded {
            text: text.to_owned(),
            lossless_bytes: false,
        };
    }
    let mut text = String::with_capacity(bytes.len());
    for chunk in bytes.utf8_chunks() {
        text.push_str(chunk.valid());
        for &byte in chunk.invalid() {
            text.push(
                char::from_u32(INVALID_BASE + u32::from(byte)).expect("valid private-use scalar"),
            );
        }
    }
    Decoded {
        text,
        lossless_bytes: true,
    }
}

/// golang.org/x/net/html/charset.DetermineEncoding without a content type.
fn detect(bytes: &[u8]) -> &'static Encoding {
    let head = &bytes[..bytes.len().min(1024)];
    if let Some((encoding, _)) = Encoding::for_bom(head) {
        return encoding;
    }
    if let Some(encoding) = prescan(head) {
        return encoding;
    }
    let complete = trim_partial_rune(head);
    if complete.iter().any(|&b| b >= 0x80) && std::str::from_utf8(complete).is_ok() {
        UTF_8
    } else {
        WINDOWS_1252
    }
}

fn trim_partial_rune(bytes: &[u8]) -> &[u8] {
    match std::str::from_utf8(bytes) {
        Err(e) if e.error_len().is_none() => &bytes[..e.valid_up_to()],
        _ => bytes,
    }
}

/// golang.org/x/net/html/charset's `<meta>` prescan.
fn prescan(head: &[u8]) -> Option<&'static Encoding> {
    let text = String::from_utf8_lossy(head);
    let metas = MetaCollector::default();
    let tokenizer = Tokenizer::new(metas, TokenizerOpts::default());
    let input = BufferQueue::default();
    input.push_back(StrTendril::from(&*text));
    let _ = tokenizer.feed(&input);
    tokenizer.end();
    let metas = tokenizer.sink.metas.into_inner();

    metas.into_iter().find_map(|attrs| {
        let mut seen = Vec::new();
        let (mut pragma, mut need_pragma, mut encoding) = (false, None, None);
        for (key, value) in attrs {
            if seen.contains(&key) {
                continue;
            }
            seen.push(key.clone());
            let value = value.to_ascii_lowercase();
            match key.as_str() {
                "http-equiv" => pragma |= value == "content-type",
                "content" if encoding.is_none() => {
                    if let Some(found) =
                        content_type_charset(&value).and_then(|l| Encoding::for_label(l.as_bytes()))
                    {
                        encoding = Some(found);
                        need_pragma = Some(true);
                    }
                }
                "charset" => {
                    encoding = Encoding::for_label(value.trim().as_bytes());
                    need_pragma = Some(false);
                }
                _ => {}
            }
        }
        match need_pragma {
            None => None,
            Some(true) if !pragma => None,
            _ => encoding.map(|e| {
                if e == UTF_16LE || e == UTF_16BE {
                    UTF_8
                } else {
                    e
                }
            }),
        }
    })
}

#[derive(Default)]
struct MetaCollector {
    metas: RefCell<Vec<Vec<(String, String)>>>,
}

impl TokenSink for MetaCollector {
    type Handle = ();

    fn process_token(&self, token: Token, _line: u64) -> TokenSinkResult<()> {
        if let Token::TagToken(tag) = token
            && tag.kind == TagKind::StartTag
            && &*tag.name == "meta"
        {
            let attrs = tag
                .attrs
                .iter()
                .map(|a| (a.name.local.to_string(), a.value.to_string()))
                .collect();
            self.metas.borrow_mut().push(attrs);
        }
        TokenSinkResult::Continue
    }
}

pub(crate) fn content_type_charset(content: &str) -> Option<String> {
    content.split(';').skip(1).find_map(|param| {
        let (key, value) = param.split_once('=')?;
        key.trim()
            .eq_ignore_ascii_case("charset")
            .then(|| value.trim().trim_matches(['"', '\'']).to_owned())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_invalid_utf8_bytes() {
        let bytes = b"<p>Caf\xe9 \xff</p>";
        let decoded = decode(bytes, &Charset::Utf8).unwrap();
        assert!(decoded.lossless_bytes);
        assert_eq!(encode(&decoded.text, true), bytes);
    }

    #[test]
    fn detects_declared_and_fallback_encodings() {
        let latin = b"<html><head><meta charset=\"iso-8859-1\"></head><body>Caf\xe9</body></html>";
        assert!(decode(latin, &Charset::Auto).unwrap().text.contains("Café"));
        let undeclared = b"<p>Caf\xe9</p>";
        assert!(
            decode(undeclared, &Charset::Auto)
                .unwrap()
                .text
                .contains("Café")
        );
        let utf16 = [0xFF, 0xFE, b'h', 0, b'i', 0];
        assert_eq!(decode(&utf16, &Charset::Auto).unwrap().text, "hi");
        assert!(decode(b"x", &Charset::Label("nope".into())).is_err());
    }
}

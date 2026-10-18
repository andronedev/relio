//! Sentence segmentation for `koboSpan`s, ported from kepubify's state machine.

#[derive(Clone, Copy, PartialEq)]
enum Input {
    Punct,
    Extra,
    Space,
    Any,
}

#[derive(Clone, Copy, PartialEq)]
enum State {
    Default,
    AfterPunct,
    AfterExtra,
    AfterSpace,
}

const CJK_ENDINGS: &[char] = &['。', '！', '？', '｡'];
const CJK_CLOSING: &[char] = &['」', '』', '）', '】', '〕', '》', '〉', '”', '’'];

/// Splits text into sentences the way kepubify does. With `cjk`, full-width
/// terminators also end a sentence without needing trailing whitespace.
pub(crate) fn split_sentences(text: &str, cjk: bool) -> Vec<&str> {
    let mut sentences = Vec::new();
    let mut start = 0;
    let mut state = State::Default;
    let mut cjk_end = false;

    for (index, c) in text.char_indices() {
        let input = match c {
            '.' | '!' | '?' => Input::Punct,
            '\'' | '"' | '”' | '’' | '“' | '…' => Input::Extra,
            '\t' | '\n' | '\x0c' | '\r' | ' ' => Input::Space,
            c if cjk && CJK_ENDINGS.contains(&c) => Input::Punct,
            c if cjk && cjk_end && CJK_CLOSING.contains(&c) => Input::Extra,
            _ => Input::Any,
        };

        let after_cjk = cjk_end && matches!(state, State::AfterPunct | State::AfterExtra);
        let (split, next) = match (state, input) {
            (State::AfterSpace, Input::Space) => (false, State::AfterSpace),
            (State::AfterSpace, Input::Punct) => (true, State::AfterPunct),
            (State::AfterSpace, _) => (true, State::Default),
            (_, Input::Any) if after_cjk => (true, State::Default),
            (State::AfterPunct, Input::Extra) if after_cjk => (false, State::AfterPunct),
            (_, Input::Punct) => (false, State::AfterPunct),
            (State::AfterPunct, Input::Extra) => (false, State::AfterExtra),
            (State::AfterPunct | State::AfterExtra, Input::Space) => (false, State::AfterSpace),
            _ => (false, State::Default),
        };

        if split {
            sentences.push(&text[start..index]);
            start = index;
        }
        if input == Input::Punct {
            cjk_end = CJK_ENDINGS.contains(&c);
        }
        state = next;
    }

    if start < text.len() || sentences.is_empty() {
        sentences.push(&text[start..]);
    }
    sentences
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split(text: &str) -> Vec<&str> {
        split_sentences(text, false)
    }

    #[test]
    fn matches_kepubify_cases() {
        assert_eq!(split(""), [""]);
        assert_eq!(split("   "), ["   "]);
        assert_eq!(
            split("One. Two!  \"Three?\" Four"),
            ["One. ", "Two!  ", "\"Three?\" ", "Four"]
        );
        assert_eq!(split("3.14 is pi... Right"), ["3.14 is pi... ", "Right"]);
        assert_eq!(split("Wait.\u{a0}No"), ["Wait.\u{a0}No"]);
        assert_eq!(split("Hm.'' Next"), ["Hm.'' Next"]);
        assert_eq!(split("Ends here. "), ["Ends here. "]);
        assert_eq!(split(" Lead. Trail"), [" Lead. ", "Trail"]);
    }

    #[test]
    fn splits_cjk_only_when_enabled() {
        assert_eq!(split("日本語です。次の文！"), ["日本語です。次の文！"]);
        assert_eq!(
            split_sentences("日本語です。次の文！「引用。」最後", true),
            ["日本語です。", "次の文！", "「引用。」", "最後"]
        );
    }
}

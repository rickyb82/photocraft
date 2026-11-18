//! The flags in the page's address (`?cpu&lang=it`), read as whole parameters.
//!
//! A page that frames PhotoCraft puts its own text in the same query: a user's name, a file's
//! name. So a flag is on only when the query has it as a key of its own, and `?author=cpu` or
//! `?name=webgl.png` switch nothing. Keys and values are compared as written: nothing here needs
//! percent-decoding, and an encoded value simply matches no flag.

/// The longest language tag taken from `?lang=`. Tags are short (`it`, `pt-BR`, `zh-Hant-TW`); the
/// value is the address's free text, and matching a tag against the catalogs splits it into parts.
const MAX_LANG_CHARS: usize = 35;

/// The parameters of `query` (with or without its leading `?`), each as its key and its value;
/// a bare key (`?cpu`) has an empty value.
fn pairs(query: &str) -> impl Iterator<Item = (&str, &str)> {
    query.strip_prefix('?').unwrap_or(query).split('&').filter(|pair| !pair.is_empty()).map(|pair| pair.split_once('=').unwrap_or((pair, "")))
}

/// Whether the query has the flag `key`, bare (`?cpu`) or with any value (`?cpu=1`).
pub fn has(query: &str, key: &str) -> bool {
    pairs(query).any(|(k, _)| k == key)
}

/// The value of the first `key=value` in the query; `None` without that key or with an empty value.
pub fn value<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    pairs(query).find(|(k, _)| *k == key).map(|(_, value)| value).filter(|value| !value.is_empty())
}

/// `?lang=<code>`: the language a page asks the interface in, when the value looks like a
/// language tag (letters, digits, `-` and `_`, at most [`MAX_LANG_CHARS`] of them). Whether
/// PhotoCraft has that language is the caller's question.
pub fn lang(query: &str) -> Option<&str> {
    value(query, "lang").filter(|tag| tag.len() <= MAX_LANG_CHARS && tag.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_')))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a framing page sends: free text beside the flags. Before, `?cpu` and `?webgl` were
    /// looked for anywhere in the query, so a user or a file with such a name switched them on.
    #[test]
    fn a_flag_is_a_key_of_its_own_never_part_of_another_value() {
        let framed = "?lang=it&host=parent&author=cpu%20webgl&v=3&name=webgl-cpu.png";
        assert!(!has(framed, "cpu") && !has(framed, "webgl"));
        assert!(has(framed, "host") && has(framed, "v"));
        for on in ["?cpu", "cpu", "?webgl&cpu", "?cpu=1&webgl", "?free-mb=300&cpu="] {
            assert!(has(on, "cpu"), "{on}");
        }
        for off in ["", "?", "?gpucpu", "?cpus", "?cpu2=1", "?x=cpu", "?x=1&=cpu", "?webgl"] {
            assert!(!has(off, "cpu"), "{off}");
        }
    }

    #[test]
    fn a_value_is_the_first_one_given_for_its_key() {
        assert_eq!(value("?lang=it&host=parent", "host"), Some("parent"));
        assert_eq!(value("?lang=it&lang=fr", "lang"), Some("it"));
        assert_eq!(value("?xlang=it&language=fr", "lang"), None);
        assert_eq!(value("?lang&host=parent", "lang"), None, "a bare key has no value");
        assert_eq!(value("?lang=", "lang"), None);
        assert_eq!(value("?name=a=b", "name"), Some("a=b"), "only the first `=` ends the key");
    }

    #[test]
    fn only_something_shaped_like_a_language_tag_is_a_language() {
        assert_eq!(lang("?lang=it&host=parent"), Some("it"));
        assert_eq!(lang("?host=parent&lang=pt-BR"), Some("pt-BR"));
        assert_eq!(lang("?lang=zh_Hant_TW"), Some("zh_Hant_TW"));
        assert_eq!(lang("?host=parent"), None);
        assert_eq!(lang("?name=lang=it"), None, "another parameter's value");
        for hostile in ["?lang=it%20IT", "?lang=../it", "?lang=it;en", "?lang=<it>", "?lang=it.UTF-8"] {
            assert_eq!(lang(hostile), None, "{hostile}");
        }
        // A tag of many parts would be split and matched part by part: refused by its length.
        assert_eq!(lang(&format!("?lang={}", "a-".repeat(MAX_LANG_CHARS))), None);
        assert_eq!(lang(&format!("?lang={}", "a".repeat(MAX_LANG_CHARS))).map(str::len), Some(MAX_LANG_CHARS));
    }
}

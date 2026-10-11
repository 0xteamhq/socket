//! Writing a value into a query so that it stays a value.
//!
//! SOQL and SOSL are text, and a value placed in one is text inside text. A
//! name such as `O'Brien` ends a SOQL string early, and whatever follows it
//! is read as part of the query. These functions write a value so that every
//! character of it is taken as itself.

/// `value` as it is written between the single quotes of a SOQL string, so
/// that all of it is read as the value and none of it as the query.
///
/// The backslash and both quotes are written with a backslash before them,
/// and a line break, a tab and every other control character as SOQL's own
/// escape for it. Everything else, accented letters and other scripts
/// included, is left as it is.
///
/// ```
/// use socketkit_salesforce::escape_soql;
/// let query = format!("SELECT Id FROM Contact WHERE LastName = '{}'", escape_soql("O'Brien"));
/// assert_eq!(query, r"SELECT Id FROM Contact WHERE LastName = 'O\'Brien'");
/// ```
///
/// This is for a value compared with `=`, `!=`, `IN` and the like. Inside
/// `LIKE`, use [`escape_soql_like`]. It does not make a number, a date or a
/// field name safe: those are not written in quotes, and have to be checked
/// for what they are.
pub fn escape_soql(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 8);
    for character in value.chars() {
        match character {
            '\\' => escaped.push_str(r"\\"),
            '\'' => escaped.push_str(r"\'"),
            '"' => escaped.push_str(r#"\""#),
            '\n' => escaped.push_str(r"\n"),
            '\r' => escaped.push_str(r"\r"),
            '\t' => escaped.push_str(r"\t"),
            '\u{000C}' => escaped.push_str(r"\f"),
            // A control character has no place in a query as it stands.
            // SOQL reads `\uXXXX` as the character with that code.
            other if other.is_control() => escaped.push_str(&format!(r"\u{:04x}", u32::from(other))),
            other => escaped.push(other),
        }
    }
    escaped
}

/// `value` as a whole SOQL string, quotes included: `'O\'Brien'`.
///
/// ```
/// use socketkit_salesforce::quote_soql;
/// assert_eq!(format!("Name = {}", quote_soql("Acme")), "Name = 'Acme'");
/// ```
pub fn quote_soql(value: &str) -> String {
    format!("'{}'", escape_soql(value))
}

/// `value` as it is written inside the string of a SOQL `LIKE`, so that it
/// matches only itself.
///
/// `LIKE` gives `%` and `_` a meaning: any run of characters, and any one
/// character. Here they are written to mean themselves, as well as
/// everything [`escape_soql`] does. The wildcards the query wants are added
/// around the result:
///
/// ```
/// use socketkit_salesforce::escape_soql_like;
/// let query = format!("SELECT Id FROM Account WHERE Name LIKE '{}%'", escape_soql_like("100%_sure"));
/// assert_eq!(query, r"SELECT Id FROM Account WHERE Name LIKE '100\%\_sure%'");
/// ```
pub fn escape_soql_like(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 8);
    for character in escape_soql(value).chars() {
        if matches!(character, '%' | '_') {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

/// The characters SOSL gives a meaning to in the text it searches for.
const SOSL_RESERVED: [char; 19] = [
    '?', '&', '|', '!', '{', '}', '[', ']', '(', ')', '^', '~', '*', ':', '\\', '"', '\'', '+', '-',
];

/// `text` as it is written in the search text of SOSL, between the braces of
/// `FIND {…}`, so that every character is searched for as itself.
///
/// SOSL reads `*` and `?` as wildcards, `"` as the start of a phrase, `-` and
/// `!` as "not", and braces as the end of the text. Each such character is
/// written with a backslash before it.
///
/// ```
/// use socketkit_salesforce::escape_sosl;
/// let search = format!("FIND {{{}}} IN NAME FIELDS RETURNING Account(Id, Name)", escape_sosl("Smith-Jones (UK)"));
/// assert_eq!(search, r"FIND {Smith\-Jones \(UK\)} IN NAME FIELDS RETURNING Account(Id, Name)");
/// ```
///
/// The words `AND`, `OR` and `AND NOT` between other words are still read as
/// operators: SOSL has no way to write them as plain words short of
/// searching for a phrase.
pub fn escape_sosl(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len() + 8);
    for character in text.chars() {
        if SOSL_RESERVED.contains(&character) {
            escaped.push('\\');
        }
        // A line break or a tab separates words, as a space does, and a
        // control character is nothing a search can find.
        escaped.push(if character.is_control() { ' ' } else { character });
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One character of a SOQL string, as Salesforce reads it.
    #[derive(Debug, Clone, Copy, PartialEq)]
    enum Read {
        /// A character that stands for itself.
        Plain(char),
        /// A `%` or `_` written without a backslash: itself after `=`, and a
        /// wildcard after `LIKE`.
        Bare(char),
    }

    /// Reads a SOQL string back, as Salesforce's documentation says it is
    /// read: the characters it holds, and how much of the input the string
    /// took. `None` when `text` does not start with a complete string.
    fn read_soql_string(text: &str) -> Option<(Vec<Read>, usize)> {
        let mut characters = text.char_indices();
        if characters.next()?.1 != '\'' {
            return None;
        }
        let mut value = Vec::new();
        while let Some((at, character)) = characters.next() {
            match character {
                '\'' => return Some((value, at + 1)),
                '\\' => value.push(Read::Plain(match characters.next()?.1 {
                    'n' | 'N' => '\n',
                    'r' | 'R' => '\r',
                    't' | 'T' => '\t',
                    'f' | 'F' => '\u{000C}',
                    'u' => {
                        let code: String = (0..4).filter_map(|_| characters.next().map(|(_, c)| c)).collect();
                        char::from_u32(u32::from_str_radix(&code, 16).ok()?)?
                    }
                    literal @ ('\\' | '\'' | '"' | '_' | '%') => literal,
                    // A backslash before anything else is an error in SOQL.
                    _ => return None,
                })),
                bare @ ('%' | '_') => value.push(Read::Bare(bare)),
                other => value.push(Read::Plain(other)),
            }
        }
        None
    }

    /// What a string that was read says when it is compared with `=`.
    fn compared(read: &[Read]) -> String {
        read.iter().map(|(Read::Plain(c) | Read::Bare(c))| *c).collect()
    }

    const AWKWARD: [&str; 22] = [
        "",
        "Acme",
        "O'Brien",
        "'",
        "''",
        r"\",
        r"\\",
        r"\'",
        r"'\",
        r"trailing\",
        "' OR Name != '",
        "x' OR '1'='1",
        "'; DELETE FROM Account --",
        r#"say "hello""#,
        "line one\nline two\r\nline three",
        "tab\there",
        "bell\u{0007} backspace\u{0008} feed\u{000C} null\u{0000} delete\u{007F}",
        "Zoë Müller, 東京, Ελλάδα, עברית",
        "emoji 🎉 and a combining e\u{0301}",
        "100% _sure_",
        r"\n is not a line break",
        r"\u0027 is not a quote",
    ];

    #[test]
    fn an_escaped_value_is_read_back_as_exactly_what_went_in() {
        for value in AWKWARD {
            let written = quote_soql(value);
            let (read, taken) =
                read_soql_string(&written).unwrap_or_else(|| panic!("{value:?} was written as {written}"));
            assert_eq!(compared(&read), value, "written as {written}");
            assert_eq!(
                taken,
                written.len(),
                "{value:?}: the string ends where it should, at {written}"
            );
        }
    }

    #[test]
    fn nothing_in_a_value_can_end_the_string_and_go_on_as_query() {
        // Whatever follows the value in the query stays outside the string.
        for value in AWKWARD {
            let query = format!(
                "SELECT Id FROM Account WHERE Name = {} AND IsDeleted = false",
                quote_soql(value)
            );
            let from = query.find('\'').unwrap();
            let (read, taken) = read_soql_string(&query[from..]).unwrap();
            assert_eq!(compared(&read), value);
            assert_eq!(&query[from + taken..], " AND IsDeleted = false", "{value:?}");
        }
    }

    #[test]
    fn each_special_character_is_written_as_soql_documents() {
        assert_eq!(escape_soql("O'Brien"), r"O\'Brien");
        assert_eq!(escape_soql(r"C:\temp"), r"C:\\temp");
        assert_eq!(escape_soql(r#"6" nails"#), r#"6\" nails"#);
        assert_eq!(escape_soql("a\nb\rc\td"), r"a\nb\rc\td");
        assert_eq!(escape_soql("\u{000C}"), r"\f");
        assert_eq!(
            escape_soql("\u{0000}\u{0007}\u{0008}\u{001B}\u{007F}"),
            r"\u0000\u0007\u0008\u001b\u007f"
        );
        // A backslash is escaped before anything else is, and once.
        assert_eq!(escape_soql(r"\'"), r"\\\'");
        assert_eq!(escape_soql(r"\n"), r"\\n");
        // What needs no escape is not touched.
        assert_eq!(
            escape_soql("Zoë & Søn GmbH (東京) 100% _ok_ 🎉"),
            "Zoë & Søn GmbH (東京) 100% _ok_ 🎉"
        );
        assert_eq!(quote_soql(""), "''");
    }

    #[test]
    fn a_like_value_matches_only_itself() {
        assert_eq!(escape_soql_like("100%"), r"100\%");
        assert_eq!(escape_soql_like("first_name"), r"first\_name");
        assert_eq!(escape_soql_like("%_%"), r"\%\_\%");
        assert_eq!(escape_soql_like("O'Brien_50%"), r"O\'Brien\_50\%");
        // A backslash in the value is doubled first, so the one added before
        // a wildcard is the only one that escapes it.
        assert_eq!(escape_soql_like(r"\%"), r"\\\%");
        assert_eq!(escape_soql_like("Zoë\n"), r"Zoë\n");
        for value in AWKWARD {
            let written = format!("'{}'", escape_soql_like(value));
            let (read, taken) = read_soql_string(&written).unwrap_or_else(|| panic!("{value:?}: {written}"));
            assert_eq!(taken, written.len(), "{value:?}");
            // What LIKE is given holds no wildcard of its own, and says the value.
            assert!(
                read.iter().all(|character| matches!(character, Read::Plain(_))),
                "{value:?} leaves a wildcard in {written}"
            );
            assert_eq!(compared(&read), value, "{value:?}");
        }
    }

    #[test]
    fn a_backslash_beside_a_wildcard_is_doubled_and_the_wildcard_escaped_once() {
        // Each expected string is written out by hand, in raw strings so
        // that what is on the line is what is compared, and is not worked
        // out by anything in this file.
        //
        // One backslash, then a percent sign: the backslash is doubled, and
        // the percent sign gets one of its own. Three backslashes in all.
        assert_eq!(escape_soql_like(r"\%"), r"\\\%");
        // A percent sign, then one backslash.
        assert_eq!(escape_soql_like(r"%\"), r"\%\\");
        // The same with an underscore, before and after.
        assert_eq!(escape_soql_like(r"\_"), r"\\\_");
        assert_eq!(escape_soql_like(r"_\"), r"\_\\");
        // Two backslashes, then a percent sign: four, and one more.
        assert_eq!(escape_soql_like(r"\\%"), r"\\\\\%");
        // A backslash between text and both wildcards.
        assert_eq!(escape_soql_like(r"50\%_off"), r"50\\\%\_off");
        // The count is what matters. An even number of backslashes before a
        // wildcard would escape each other and leave it a wildcard; there
        // are always two for each one in the value, and one more.
        for (value, backslashes) in [(r"%", 1), (r"\%", 3), (r"\\%", 5), (r"\\\%", 7)] {
            let written = escape_soql_like(value);
            assert_eq!(
                written.matches('\\').count(),
                backslashes,
                "{value} is written {written}"
            );
            assert!(written.ends_with('%'));
        }
        // Outside LIKE the wildcards are plain, and only the backslash is doubled.
        assert_eq!(escape_soql(r"\%_"), r"\\%_");
    }

    #[test]
    fn every_character_sosl_reserves_is_escaped() {
        assert_eq!(
            escape_sosl(r#"?&|!{}[]()^~*:\"'+-"#),
            r#"\?\&\|\!\{\}\[\]\(\)\^\~\*\:\\\"\'\+\-"#
        );
        assert_eq!(escape_sosl("Smith-Jones (UK)"), r"Smith\-Jones \(UK\)");
        assert_eq!(escape_sosl("Acme"), "Acme");
        assert_eq!(escape_sosl("Zoë Müller 東京 🎉"), "Zoë Müller 東京 🎉");
        assert_eq!(
            escape_sosl("a\nb\tc\u{0000}d"),
            "a b c d",
            "a line break separates words"
        );
        assert_eq!(escape_sosl(""), "");
    }

    #[test]
    fn search_text_cannot_close_the_braces_or_add_a_clause() {
        for text in [
            "Acme} RETURNING User(Id, Email) FIND {x",
            "} IN ALL FIELDS RETURNING Account(Id WHERE Name = 'x')",
            r"\} RETURNING User(Id)",
            r"\\} RETURNING User(Id)",
            "{}",
        ] {
            let written = escape_sosl(text);
            // Every brace is behind an odd number of backslashes: escaped.
            let characters: Vec<char> = written.chars().collect();
            for (at, character) in characters.iter().enumerate() {
                if matches!(character, '{' | '}') {
                    let before = characters[..at].iter().rev().take_while(|c| **c == '\\').count();
                    assert_eq!(before % 2, 1, "{text:?} leaves a brace open in {written}");
                }
            }
        }
    }
}

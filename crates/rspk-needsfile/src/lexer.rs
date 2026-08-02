//! Needsfile lexer (tokenizer).

use crate::error::ParseError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Token
{
    /// Newline — significant because entries are line-delimited.
    Newline,
    /// An identifier or unquoted keyword (e.g. `if`, `os`, `present`,
    /// `apt`, `ripgrep`).
    Ident(String),
    /// A double-quoted string literal (e.g. `"dev"`).
    String(String),
    /// `=` or `==`.
    Equals,
    /// `!=`.
    NotEquals,
    /// `&&`.
    And,
    /// `||`.
    Or,
    /// `!`.
    Bang,
    /// `(`.
    LParen,
    /// `)`.
    RParen,
    /// `{`.
    LBrace,
    /// `}`.
    RBrace,
    /// `:` (manager/package separator).
    Colon,
    /// End of file.
    Eof,
}

#[derive(Debug, Clone)]
pub(crate) struct Spanned
{
    pub token:  Token,
    pub offset: usize,
    pub length: usize,
}

/// Tokenizes the source into a flat list of spanned tokens.
///
/// Comments (`# ...` until end of line) and whitespace are skipped.
/// Newlines are emitted as tokens so the parser can delimit entries.
pub(crate) fn tokenize(source: &str) -> Result<Vec<Spanned>, ParseError>
{
    let mut tokens = Vec::new();
    let bytes = source.as_bytes();
    let mut i = 0;
    while i < bytes.len()
    {
        let b = bytes[i];
        // Whitespace (except newline).
        if b == b' ' || b == b'\t' || b == b'\r'
        {
            i += 1;
            continue;
        }
        // Newline.
        if b == b'\n'
        {
            tokens.push(Spanned {
                token:  Token::Newline,
                offset: i,
                length: 1,
            });
            i += 1;
            continue;
        }
        // Comment: skip to end of line.
        if b == b'#'
        {
            while i < bytes.len() && bytes[i] != b'\n'
            {
                i += 1;
            }
            continue;
        }
        // Single-character punctuation.
        let single = match b
        {
            b'{' => Some(Token::LBrace),
            b'}' => Some(Token::RBrace),
            b'(' => Some(Token::LParen),
            b')' => Some(Token::RParen),
            b':' => Some(Token::Colon),
            _ => None,
        };
        if let Some(t) = single
        {
            tokens.push(Spanned {
                token:  t,
                offset: i,
                length: 1,
            });
            i += 1;
            continue;
        }
        // Two-character operators.
        if i + 1 < bytes.len()
        {
            let two = &source[i..i + 2];
            let t = match two
            {
                "==" => Some(Token::Equals),
                "!=" => Some(Token::NotEquals),
                "&&" => Some(Token::And),
                "||" => Some(Token::Or),
                _ => None,
            };
            if let Some(tok) = t
            {
                tokens.push(Spanned {
                    token:  tok,
                    offset: i,
                    length: 2,
                });
                i += 2;
                continue;
            }
        }
        // Single-character `=` and `!`.
        if b == b'='
        {
            tokens.push(Spanned {
                token:  Token::Equals,
                offset: i,
                length: 1,
            });
            i += 1;
            continue;
        }
        if b == b'!'
        {
            tokens.push(Spanned {
                token:  Token::Bang,
                offset: i,
                length: 1,
            });
            i += 1;
            continue;
        }
        // Quoted string.
        if b == b'"'
        {
            let start = i;
            i += 1;
            let mut value = String::new();
            while i < bytes.len() && bytes[i] != b'"'
            {
                if bytes[i] == b'\\' && i + 1 < bytes.len()
                {
                    i += 1;
                    match bytes[i]
                    {
                        b'"' => value.push('"'),
                        b'\\' => value.push('\\'),
                        b'n' => value.push('\n'),
                        b't' => value.push('\t'),
                        other => value.push(other as char),
                    }
                }
                else
                {
                    value.push(bytes[i] as char);
                }
                i += 1;
            }
            if i >= bytes.len()
            {
                return Err(ParseError {
                    offset:  start,
                    length:  1,
                    message: "unterminated string literal".to_string(),
                });
            }
            i += 1; // closing `"`
            tokens.push(Spanned {
                token:  Token::String(value),
                offset: start,
                length: i - start,
            });
            continue;
        }
        // Identifier: [A-Za-z_@./-][A-Za-z0-9_@./+-]*
        // Allows `@angular/core`, `pkg@scope`, `a.b-c_d` etc.
        if is_ident_start(b)
        {
            let start = i;
            while i < bytes.len() && is_ident_cont(bytes[i])
            {
                i += 1;
            }
            let text = &source[start..i];
            tokens.push(Spanned {
                token:  Token::Ident(text.to_string()),
                offset: start,
                length: i - start,
            });
            continue;
        }
        // Unknown character.
        return Err(ParseError {
            offset:  i,
            length:  1,
            message: format!("unexpected character: {:?}", b as char),
        });
    }
    tokens.push(Spanned {
        token:  Token::Eof,
        offset: source.len(),
        length: 0,
    });
    Ok(tokens)
}

fn is_ident_start(b: u8) -> bool
{
    b.is_ascii_alphanumeric() || b == b'_' || b == b'@'
}

fn is_ident_cont(b: u8) -> bool
{
    b.is_ascii_alphanumeric()
        || b == b'_'
        || b == b'-'
        || b == b'.'
        || b == b'/'
        || b == b'@'
        || b == b'+'
}

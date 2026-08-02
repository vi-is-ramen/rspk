//! Needsfile parser.

use crate::error::ParseError;
use crate::lexer::{Spanned, Token};
use crate::types::{Condition, ConditionalBlock, NeedsEntry, NeedsItem};

pub(crate) struct Parser
{
    tokens: Vec<Spanned>,
    pos:    usize,
}

impl Parser
{
    pub(crate) fn new(tokens: Vec<Spanned>) -> Self
    {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> &Spanned
    {
        &self.tokens[self.pos]
    }

    fn advance(&mut self) -> Spanned
    {
        let t = self.tokens[self.pos].clone();
        if self.pos + 1 < self.tokens.len()
        {
            self.pos += 1;
        }
        t
    }

    fn skip_newlines(&mut self)
    {
        while self.peek().token == Token::Newline
        {
            self.advance();
        }
    }

    fn expect(&mut self, expected: &Token) -> Result<Spanned, ParseError>
    {
        let p = self.peek().clone();
        if std::mem::discriminant(&p.token) == std::mem::discriminant(expected)
        {
            Ok(self.advance())
        }
        else
        {
            Err(ParseError {
                offset:  p.offset,
                length:  p.length.max(1),
                message: format!(
                    "expected {:?}, found {:?}",
                    expected, p.token
                ),
            })
        }
    }

    /// Parses the whole file into a list of [`NeedsItem`]s.
    pub(crate) fn parse_file(&mut self) -> Result<Vec<NeedsItem>, ParseError>
    {
        let mut items = Vec::new();
        loop
        {
            self.skip_newlines();
            if self.peek().token == Token::Eof
            {
                break;
            }
            items.push(self.parse_item()?);
        }
        Ok(items)
    }

    /// Parses a single item: either `if ... { ... }` or a package
    /// entry.
    fn parse_item(&mut self) -> Result<NeedsItem, ParseError>
    {
        let p = self.peek().clone();
        if let Token::Ident(ref s) = p.token
            && s == "if"
        {
            self.advance(); // consume `if`
            self.skip_newlines();
            let condition = self.parse_expr()?;
            self.skip_newlines();
            self.expect(&Token::LBrace)?;
            let mut block_items = Vec::new();
            loop
            {
                self.skip_newlines();
                if self.peek().token == Token::RBrace
                {
                    break;
                }
                if self.peek().token == Token::Eof
                {
                    return Err(ParseError {
                        offset:  p.offset,
                        length:  2,
                        message: "unclosed conditional block: expected '}'"
                            .to_string(),
                    });
                }
                block_items.push(self.parse_item()?);
            }
            self.advance(); // consume `}`
            Ok(NeedsItem::Conditional(ConditionalBlock {
                condition,
                items: block_items,
            }))
        }
        else
        {
            let entry = self.parse_entry()?;
            // Consume trailing newline (if any) so the outer loop
            // sees the next item cleanly.
            if self.peek().token == Token::Newline
            {
                self.advance();
            }
            Ok(NeedsItem::Entry(entry))
        }
    }

    /// Parses a package entry on the current line.
    ///
    /// Format: `[manager:]package[=version]`.
    fn parse_entry(&mut self) -> Result<NeedsEntry, ParseError>
    {
        let first = self.expect_ident()?;
        let first_text = match first.token
        {
            Token::Ident(s) => s,
            _ => unreachable!(),
        };
        // Optional `manager:`.
        let (manager, package_start) = if self.peek().token == Token::Colon
        {
            self.advance();
            let pkg_tok = self.expect_ident()?;
            let pkg = match pkg_tok.token
            {
                Token::Ident(s) => s,
                _ => unreachable!(),
            };
            (Some(first_text), pkg)
        }
        else
        {
            (None, first_text)
        };
        if package_start.is_empty()
        {
            return Err(ParseError {
                offset:  first.offset,
                length:  first.length.max(1),
                message: "empty package name".to_string(),
            });
        }
        // Optional `=version`.
        let version = if self.peek().token == Token::Equals
        {
            self.advance();
            let v_tok = self.peek().clone();
            let v = match &v_tok.token
            {
                Token::Ident(s) => s.clone(),
                Token::String(s) => s.clone(),
                _ =>
                {
                    return Err(ParseError {
                        offset:  v_tok.offset,
                        length:  v_tok.length.max(1),
                        message: "expected version after '='".to_string(),
                    });
                },
            };
            self.advance();
            if v.is_empty()
            {
                return Err(ParseError {
                    offset:  v_tok.offset,
                    length:  v_tok.length.max(1),
                    message: "empty version after '='".to_string(),
                });
            }
            Some(v)
        }
        else
        {
            None
        };
        // Entry must end at newline or EOF.
        match self.peek().token
        {
            Token::Newline | Token::Eof =>
            {},
            _ =>
            {
                let p = self.peek().clone();
                return Err(ParseError {
                    offset:  p.offset,
                    length:  p.length.max(1),
                    message: "unexpected token after package entry".to_string(),
                });
            },
        }
        Ok(NeedsEntry {
            package: package_start,
            manager,
            version,
        })
    }

    fn expect_ident(&mut self) -> Result<Spanned, ParseError>
    {
        let p = self.peek().clone();
        match p.token
        {
            Token::Ident(_) => Ok(self.advance()),
            _ => Err(ParseError {
                offset:  p.offset,
                length:  p.length.max(1),
                message: format!("expected identifier, found {:?}", p.token),
            }),
        }
    }

    // ─── Expression grammar ─────────────────────────────────────
    //
    //   expr     -> or_expr
    //   or_expr  -> and_expr ( '||' and_expr )*
    //   and_expr -> unary ( '&&' unary )*
    //   unary    -> '!' unary | primary
    //   primary  -> '(' expr ')' | comparison
    //   comparison -> IDENT '=' value
    //              | 'present' STRING
    //              | 'feature' STRING

    fn parse_expr(&mut self) -> Result<Condition, ParseError>
    {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<Condition, ParseError>
    {
        let mut left = self.parse_and()?;
        while self.peek().token == Token::Or
        {
            self.advance();
            self.skip_newlines();
            let right = self.parse_and()?;
            left = Condition::Or(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Condition, ParseError>
    {
        let mut left = self.parse_unary()?;
        while self.peek().token == Token::And
        {
            self.advance();
            self.skip_newlines();
            let right = self.parse_unary()?;
            left = Condition::And(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Condition, ParseError>
    {
        if self.peek().token == Token::Bang
        {
            self.advance();
            let inner = self.parse_unary()?;
            Ok(Condition::Not(Box::new(inner)))
        }
        else
        {
            self.parse_primary()
        }
    }

    fn parse_primary(&mut self) -> Result<Condition, ParseError>
    {
        if self.peek().token == Token::LParen
        {
            self.advance();
            self.skip_newlines();
            let inner = self.parse_expr()?;
            self.skip_newlines();
            self.expect(&Token::RParen)?;
            return Ok(inner);
        }
        let p = self.peek().clone();
        let ident = match p.token
        {
            Token::Ident(ref s) => s.clone(),
            _ =>
            {
                return Err(ParseError {
                    offset:  p.offset,
                    length:  p.length.max(1),
                    message: "expected condition (os, present, feature, mode)"
                        .to_string(),
                });
            },
        };
        match ident.as_str()
        {
            "present" =>
            {
                self.advance();
                let v = self.expect_string()?;
                Ok(Condition::ManagerPresent(v))
            },
            "feature" =>
            {
                self.advance();
                let v = self.expect_string()?;
                Ok(Condition::FeaturePresent(v))
            },
            "os" | "mode" =>
            {
                self.advance();
                let eq_tok = self.peek().clone();
                match eq_tok.token
                {
                    Token::Equals =>
                    {
                        self.advance();
                    },
                    Token::NotEquals =>
                    {
                        // `os != linux` desugars to `!(os = linux)`
                        self.advance();
                        let value = self.expect_value()?;
                        let inner = if ident == "os"
                        {
                            Condition::OsEq(value)
                        }
                        else
                        {
                            Condition::ModeEq(value)
                        };
                        return Ok(Condition::Not(Box::new(inner)));
                    },
                    _ =>
                    {
                        return Err(ParseError {
                            offset:  eq_tok.offset,
                            length:  eq_tok.length.max(1),
                            message: "expected '=' or '!=' after identifier"
                                .to_string(),
                        });
                    },
                }
                let value = self.expect_value()?;
                if ident == "os"
                {
                    Ok(Condition::OsEq(value))
                }
                else
                {
                    Ok(Condition::ModeEq(value))
                }
            },
            other => Err(ParseError {
                offset:  p.offset,
                length:  p.length,
                message: format!(
                    "unknown condition keyword '{}' (expected os, mode, \
                     present, feature)",
                    other
                ),
            }),
        }
    }

    /// Expects a double-quoted string.
    fn expect_string(&mut self) -> Result<String, ParseError>
    {
        let p = self.peek().clone();
        match p.token
        {
            Token::String(s) =>
            {
                self.advance();
                Ok(s)
            },
            _ => Err(ParseError {
                offset:  p.offset,
                length:  p.length.max(1),
                message: "expected quoted string".to_string(),
            }),
        }
    }

    /// Expects either a quoted string or a bare identifier as a value
    /// (used on the right-hand side of `os = linux`).
    fn expect_value(&mut self) -> Result<String, ParseError>
    {
        let p = self.peek().clone();
        match p.token
        {
            Token::String(s) =>
            {
                self.advance();
                Ok(s)
            },
            Token::Ident(s) =>
            {
                self.advance();
                Ok(s)
            },
            _ => Err(ParseError {
                offset:  p.offset,
                length:  p.length.max(1),
                message: "expected value (identifier or quoted string)"
                    .to_string(),
            }),
        }
    }
}

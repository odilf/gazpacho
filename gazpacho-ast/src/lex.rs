//! Hand-rolled lexer.
//!
//! Newlines are significant at bracket depth 0 (they separate items and
//! let-bindings) and suppressed inside `()`/`[]`/`{}` so expressions can
//! span lines when bracketed. Consecutive newlines collapse to one token.

use num_rational::Rational64;

use crate::ast::{Module, Span, Str};

#[derive(Clone, Debug, PartialEq)]
pub enum Token {
    Ident(Str),
    Int(i64),
    Float(f64),
    Str(Str),
    Time(Rational64),

    KwDef,
    KwLet,
    KwImport,
    KwAs,
    KwTrue,
    KwFalse,

    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Comma,
    Colon,
    Semi,
    Eq,
    Arrow,
    Pipe,
    DotDot,
    Dot,
    Plus,
    Minus,
    Star,
    Slash,
    Lt,
    Gt,
    Le,
    Ge,
    EqEq,
    Ne,

    Newline,
    Eof,
}

#[derive(Clone, Debug)]
pub struct SpannedToken {
    pub value: Token,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct LexError {
    pub kind: LexErrorKind,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum LexErrorKind {
    UnterminatedString,
    Other(Option<char>),
    InvalidNumberIteral,
    //"unexpected character `|` (pipelines use `|>`)"
    UnexpectedPipe,
    // "unexpected character `!`"
    UnexpectedBang,
    UnexpectedCharacter(char),
}

// Shorthand to construct errors.
// type LexErr = LexErrorKind;
use LexErrorKind as LexErr;

/// Lexes a source file and returns the tokens and the errors encountered. If it
/// encounters and error it tries to keep going to partially parse the file.
pub fn lex(src: &str) -> (Vec<SpannedToken>, Vec<LexError>, Module) {
    let mut lexer = Lexer::new(src);
    lexer.run();

    (lexer.tokens, lexer.errors, lexer.module)
}

struct Lexer<'a> {
    module: Module,
    src: &'a str,
    pos: usize,
    /// Bracket nesting depth; newlines are suppressed when > 0.
    depth: u32,
    tokens: Vec<SpannedToken>,
    errors: Vec<LexError>,
}

#[expect(
    clippy::string_slice,
    reason = "self.pos only ever advances by char::len_utf8() or over bytes
    already verified ascii, so it's always at a char boundary"
)]
impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Self {
        Lexer {
            src,
            module: Module::empty(),
            pos: 0,
            depth: 0,
            tokens: Vec::new(),
            errors: Vec::new(),
        }
    }

    fn run(&mut self) {
        while let Some(c) = self.peek_char() {
            let start = self.pos;
            match c {
                ' ' | '\t' | '\r' => {
                    self.pos += 1;
                }
                '\n' => {
                    self.pos += 1;
                    let after_newline = matches!(
                        self.tokens.last().map(|t| &t.value),
                        Some(Token::Newline) | None
                    );
                    if self.depth == 0 && !after_newline {
                        self.insert(Token::Newline, start);
                    }
                }
                '/' if self.peek_at(1) == Some('/') => {
                    while let Some(c) = self.peek_char() {
                        if c == '\n' {
                            break;
                        }
                        self.pos += c.len_utf8();
                    }
                }
                '"' => self.string(start),
                c if c.is_ascii_digit() => self.number(start),
                c if c.is_ascii_alphabetic() || c == '_' => self.ident(start),
                c => self.punct(c, start),
            }
        }
    }

    fn peek_char(&self) -> Option<char> {
        self.src[self.pos..].chars().next()
    }

    fn peek_at(&self, n_bytes_ahead: usize) -> Option<char> {
        self.src.get(self.pos + n_bytes_ahead..)?.chars().next()
    }

    fn insert(&mut self, token: Token, start: usize) {
        self.tokens.push(SpannedToken {
            value: token,
            span: Span::new(start, self.pos),
        });
    }

    fn error(&mut self, error: LexErrorKind, start: usize) {
        self.errors.push(LexError {
            kind: error,
            span: Span::new(start, self.pos),
        });
    }

    fn string(&mut self, start: usize) {
        self.pos += 1; // opening quote
        let mut value = String::new();
        loop {
            match self.peek_char() {
                None | Some('\n') => {
                    self.error(LexErr::UnterminatedString, start);
                    break;
                }
                Some('"') => {
                    self.pos += 1;
                    break;
                }
                Some('\\') => {
                    self.pos += 1;
                    match self.peek_char() {
                        Some('n') => value.push('\n'),
                        Some('t') => value.push('\t'),
                        Some('\\') => value.push('\\'),
                        Some('"') => value.push('"'),
                        other => {
                            self.error(LexErr::Other(other), self.pos - 1);
                        }
                    }
                    if let Some(c) = self.peek_char() {
                        self.pos += c.len_utf8();
                    }
                }
                Some(c) => {
                    value.push(c);
                    self.pos += c.len_utf8();
                }
            }
        }

        // TODO(alloc): I guess we can avoid the allocation with a specific (awkward)
        // `get_str_or_intern_with_owned_value` function. This becomes even more
        // awkward if you consider that this should arguably be a Cow in the
        // first place. Unimportant for now regardless.
        let str = self.module.get_str_or_intern(&value);
        self.insert(Token::Str(str), start);
    }

    fn digits(&mut self) -> &str {
        let start = self.pos;
        while matches!(self.peek_char(), Some(c) if c.is_ascii_digit()) {
            self.pos += 1;
        }
        &self.src[start..self.pos]
    }

    fn number(&mut self, start: usize) {
        let whole = self.digits().to_owned();
        // Consume a fractional part only if `.` is followed by a digit, so
        // `2..3` lexes as Int DotDot Int and `x.field` keeps its Dot.
        let frac = if self.peek_char() == Some('.')
            && matches!(self.peek_at(1), Some(c) if c.is_ascii_digit())
        {
            self.pos += 1;
            Some(self.digits().to_owned())
        } else {
            None
        };

        // Unit suffix: `2s`, `250ms`, `2.5s`.
        let suffix_start = self.pos;
        while matches!(self.peek_char(), Some(c) if c.is_ascii_alphabetic()) {
            self.pos += 1;
        }
        let suffix = &self.src[suffix_start..self.pos];

        let token = match (frac, suffix) {
            (None, "") => whole.parse::<i64>().map(Token::Int).ok(),
            (Some(frac), "") => format!("{whole}.{frac}")
                .parse::<f64>()
                .map(Token::Float)
                .ok(),
            (frac, "s" | "ms") => {
                let frac = frac.unwrap_or_default();
                let scale = 10i64.checked_pow(frac.len() as u32);
                let per_unit = if suffix == "ms" { 1000 } else { 1 };
                (|| {
                    let scale = scale?;
                    let whole: i64 = whole.parse().ok()?;
                    let frac: i64 = if frac.is_empty() {
                        0
                    } else {
                        frac.parse().ok()?
                    };
                    let numer = whole.checked_mul(scale)?.checked_add(frac)?;
                    let denom = scale.checked_mul(per_unit)?;
                    Some(Token::Time(Rational64::new(numer, denom)))
                })()
            }
            _ => None,
        };

        match token {
            Some(token) => self.insert(token, start),
            None => self.error(LexErr::InvalidNumberIteral, start),
        }
    }

    fn ident(&mut self, start: usize) {
        while matches!(self.peek_char(), Some(c) if c.is_ascii_alphanumeric() || c == '_') {
            self.pos += 1;
        }
        let kind = match &self.src[start..self.pos] {
            "def" => Token::KwDef,
            "let" => Token::KwLet,
            "import" => Token::KwImport,
            "as" => Token::KwAs,
            "true" => Token::KwTrue,
            "false" => Token::KwFalse,
            name => Token::Ident(self.module.get_str_or_intern(name)),
        };
        self.insert(kind, start);
    }

    /// Choose `yes` if next char is `next`, otherwise `no`.
    fn choose(&mut self, next: char, yes: Token, no: Token) -> Token {
        if self.peek_char() == Some(next) {
            self.pos += 1;
            yes
        } else {
            no
        }
    }

    fn punct(&mut self, c: char, start: usize) {
        use Token as TK;
        self.pos += c.len_utf8();
        let kind = match c {
            '(' => {
                self.depth += 1;
                TK::LParen
            }
            '[' => {
                self.depth += 1;
                TK::LBracket
            }
            '{' => {
                self.depth += 1;
                TK::LBrace
            }
            ')' => {
                self.depth = self.depth.saturating_sub(1);
                TK::RParen
            }
            ']' => {
                self.depth = self.depth.saturating_sub(1);
                TK::RBracket
            }
            '}' => {
                self.depth = self.depth.saturating_sub(1);
                TK::RBrace
            }
            ',' => TK::Comma,
            ':' => TK::Colon,
            ';' => TK::Semi,
            '+' => TK::Plus,
            '*' => TK::Star,
            '/' => TK::Slash,
            '=' => self.choose('=', TK::EqEq, TK::Eq),
            '-' => self.choose('>', TK::Arrow, TK::Minus),
            '|' => {
                if self.peek_char() == Some('>') {
                    self.pos += 1;
                    TK::Pipe
                } else {
                    self.error(LexErr::UnexpectedPipe, start);
                    return;
                }
            }
            '.' => self.choose('.', TK::DotDot, TK::Dot),
            '<' => self.choose('=', TK::Le, TK::Lt),
            '>' => self.choose('=', TK::Ge, TK::Gt),
            '!' => {
                if self.peek_char() == Some('=') {
                    self.pos += 1;
                    TK::Ne
                } else {
                    self.error(LexErr::UnexpectedBang, start);
                    return;
                }
            }
            c => {
                self.error(LexErr::UnexpectedCharacter(c), start);
                return;
            }
        };
        self.insert(kind, start);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[track_caller]
    fn tokens(src: &str) -> Vec<Token> {
        let (tokens, errors, _module) = lex(src);
        assert!(errors.is_empty(), "unexpected lex errors: {errors:?}");
        tokens.into_iter().map(|t| t.value).collect()
    }

    #[test]
    fn lex_time_units() {
        let cases = [
            ("1s", Rational64::from_integer(1)),
            ("1500ms", Rational64::new(3, 2)),
            ("0.5s", Rational64::new(1, 2)),
            ("10ms", Rational64::new(1, 100)),
            ("0s", Rational64::from_integer(0)),
        ];
        for (src, expected) in cases {
            assert_eq!(tokens(src), vec![Token::Time(expected)], "lexing {src:?}");
        }
    }

    #[test]
    fn lex_numbers_and_ranges() {
        // `2..3` must not lex `2.` as a float.
        assert_eq!(
            tokens("2..3"),
            vec![Token::Int(2), Token::DotDot, Token::Int(3)]
        );
        assert_eq!(tokens("2.75"), vec![Token::Float(2.75)]);
    }

    #[test]
    fn lex_string_escapes() {
        let (tokens, errors, module) = lex(r#""a\"b\\c\nd""#);
        assert!(errors.is_empty(), "unexpected lex errors: {errors:?}");

        let Token::Str(str) = tokens[0].value else {
            panic!("not a str token")
        };

        assert_eq!(module.str(str), "a\"b\\c\nd");
    }

    #[test]
    fn lex_newlines_suppressed_inside_brackets() {
        let toks = tokens("[1,\n2]\n(a\n+ b)");
        assert!(
            !toks.contains(&Token::Newline) || {
                // Only the newline *between* the bracketed groups survives.
                toks.iter().filter(|k| **k == Token::Newline).count() == 1
            },
            "unexpected newlines in {toks:?}"
        );
    }

    #[test]
    fn lex_consecutive_newlines_collapse() {
        let toks = tokens("a\n\n\nb");
        let newlines = toks.iter().filter(|k| **k == Token::Newline).count();
        assert_eq!(newlines, 1);
    }

    #[test]
    fn lex_comments_are_skipped() {
        assert_eq!(
            tokens("1 // a comment, with 2s and \"strings\"\n2"),
            vec![Token::Int(1), Token::Newline, Token::Int(2),]
        );
    }

    #[test]
    fn lex_errors_bad_suffix_and_char() {
        let (_, errors, _) = lex("2x");
        assert_eq!(errors.len(), 1, "{errors:?}");
        let (_, errors, _) = lex("@");
        assert_eq!(errors.len(), 1, "{errors:?}");
        let (_, errors, _) = lex("\"unterminated");
        assert_eq!(errors.len(), 1, "{errors:?}");
    }
}

//! Recursive-descent parser with error recovery.
//!
//! Surface sugar keeps its structure in the tree:
//! - unary/binary/variadic operators -> [`Expr::Operator`] nodes
//! - `a..b` -> a binary `..` operator
//! - `x |> f(a)` -> `f(x, a)` (prepends `x` to the call's arguments)
//! - `.field` -> [`Expr::FieldAccessor`]
//!
//! Broken regions become `Expr::Error` nodes so we can partially parse the ast.

use crate::ast::{
    Arg, BinaryOp, Def, Expr, ExprId, Import, Literal, Module, Name, Operator, Param, Span,
    TypeExpr, UnaryOp, VariadicOp,
};
use crate::lex::{LexErrorKind, SpannedToken, Token, lex};

#[derive(Clone, Debug)]
pub struct ParseError {
    pub kind: ParseErrorKind,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum ParseErrorKind {
    Lex(LexErrorKind),
    ExpectedToken { token: Token, found: Option<Token> },
    ExpectedIdentifier { found: Option<Token> },
    ExpectedStringAfterImport,
    ExpectedExpr { found: Option<Token> },
    ExpectedEof { found: Token },
}

use ParseErrorKind as ParseErr;

pub fn parse(src: &str) -> (Module, Vec<ParseError>) {
    // TODO: We could try to stream, but that seems unecessarilly complex for now.
    let (tokens, lex_errors, module) = lex(src);
    let mut parser = Parser {
        tokens,
        pos: 0,
        module,
        errors: lex_errors
            .into_iter()
            .map(|e| ParseError {
                kind: ParseErrorKind::Lex(e.kind),
                span: e.span,
            })
            .collect(),
    };
    parser.module();
    (parser.module, parser.errors)
}

/// Current parsing state.
#[derive(Debug, Clone)]
struct Parser {
    tokens: Vec<SpannedToken>,
    /// pos=tokens.len() indicates end of file.
    pos: usize,
    module: Module,
    errors: Vec<ParseError>,
}

// Plumbing
impl Parser {
    fn peek(&self) -> Option<&Token> {
        Some(&self.tokens.get(self.pos)?.value)
    }
    fn peek_2nd(&self) -> Option<&Token> {
        Some(&self.tokens.get(self.pos + 1)?.value)
    }

    fn bump(&mut self) {
        self.pos = (self.pos + 1).min(self.tokens.len());
    }

    fn eat(&mut self, token: Token) -> bool {
        if self.peek() == Some(&token) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, token: Token) {
        if self.peek() != Some(&token) {
            self.error(
                ParseErr::ExpectedToken {
                    token,
                    found: self.peek().cloned(),
                },
                self.start_span(),
            );
        }

        self.bump();
    }

    fn expect_ident(&mut self) -> Name {
        let Some(Token::Ident(name)) = self.peek() else {
            self.error(
                ParseErr::ExpectedIdentifier {
                    found: self.peek().cloned(),
                },
                self.start_span(),
            );

            // NIT: Ugly (but pragmatic) way to handle this
            return Name(self.module.get_str_or_intern("<error>"));
        };

        let name = Name(name.to_owned());
        self.bump();
        name
    }

    /// Whether we have finished reading all tokens.
    fn eof(&mut self) -> bool {
        self.pos == self.tokens.len()
    }

    #[track_caller]
    fn start_span(&self) -> u32 {
        match self.tokens.get(self.pos) {
            Some(t) => t.span.start,
            None => {
                self.tokens
                    .last()
                    // TODO: Prop test
                    .expect("spans are not created for empty files")
                    .span
                    .end
                    + 1
            }
        }
    }

    #[track_caller]
    fn end_span(&self, span_start: u32) -> Span {
        let end = if self.pos == 0 {
            self.tokens
                .first()
                .expect("spans are not created for empty files")
                .span
                .start
        } else {
            self.tokens
                .get(self.pos - 1)
                // TODO: Prop test?
                .expect("pos <= tokens.len()")
                .span
                .end
        };

        Span {
            start: span_start,
            end,
        }
    }

    #[track_caller]
    fn error(&mut self, error: ParseErrorKind, span_start: u32) -> Span {
        let span = self.end_span(span_start);
        self.errors.push(ParseError { kind: error, span });
        span
    }

    /// Skips statement separators (newlines and semicolons).
    fn eat_seps(&mut self) {
        while self.eat(Token::Newline) || self.eat(Token::Semi) {}
    }

    /// Move forward until we consume a separator (or hit EOF).
    fn recover_to_sep(&mut self) {
        while !self.eof() {
            if self.eat(Token::Newline) || self.eat(Token::Semi) {
                break;
            }
            // Skip over any other (unexpected) token; otherwise we'd spin here.
            self.bump();
        }
    }

    fn alloc(&mut self, expr: Expr, span: Span) -> ExprId {
        self.module.alloc(expr, span)
    }
}

// Parsing proper
impl Parser {
    fn module(&mut self) {
        loop {
            self.eat_seps();
            if !self.import_or_def() {
                break;
            }
        }

        // No tail expression
        if self.peek().is_none() {
            return;
        }

        self.module.value = Some(self.body());
        self.eat_seps();
        if let Some(next) = self.peek() {
            self.error(
                ParseErr::ExpectedEof {
                    found: next.clone(),
                },
                self.start_span(),
            );
        }
    }

    /// Returns whether we parsed either one of the two.
    fn import_or_def(&mut self) -> bool {
        if self.eat(Token::KwImport) {
            let path = match self.peek() {
                Some(&Token::Str(str)) => {
                    self.bump();
                    str
                }
                _ => {
                    self.error(ParseErr::ExpectedStringAfterImport, self.start_span());
                    self.module.get_str_or_intern("<error>")
                }
            };

            self.expect(Token::KwAs);
            let alias = self.expect_ident();
            self.module.imports.push(Import { path, alias });
            return true;
        } else if self.eat(Token::KwDef) {
            let name = self.expect_ident();

            // Arguments
            let (params, ret) = if self.eat(Token::LParen) {
                let mut params = Vec::new();
                while !matches!(self.peek(), Some(Token::RParen)) {
                    params.push(self.param());
                    if !self.eat(Token::Comma) {
                        break;
                    }
                }
                self.expect(Token::RParen);
                let ret = if self.eat(Token::Arrow) {
                    Some(self.type_expr())
                } else {
                    None
                };
                (params, ret)
            } else {
                // Allow no parenthesis.
                (Vec::new(), None)
            };

            self.expect(Token::Eq);
            let body = self.body();
            self.module.defs.push(Def {
                name,
                params,
                ret,
                body,
            });
            return true;
        }

        false
    }

    fn param(&mut self) -> Param {
        let name = self.expect_ident();
        let ty = self.eat(Token::Colon).then(|| self.type_expr());

        let default = self.eat(Token::Eq).then(|| self.expr());
        Param { name, ty, default }
    }

    fn type_expr(&mut self) -> TypeExpr {
        let name = self.expect_ident();
        let mut args = Vec::new();
        if self.eat(Token::Lt) {
            loop {
                args.push(self.type_expr());
                if !self.eat(Token::Comma) {
                    break;
                }
            }
            self.expect(Token::Gt);
        }
        TypeExpr::Named { name, args }
    }

    /// A body: a chain of `let name = expr` lines followed by the result
    /// expression.
    fn body(&mut self) -> ExprId {
        self.eat_seps();
        let span_start = self.start_span();

        let mut bindings = Vec::new();
        while self.eat(Token::KwLet) {
            let name = self.expect_ident();
            self.expect(Token::Eq);
            let value = self.expr();
            bindings.push((name, value));
            self.eat_seps();
        }

        let body = self.expr();
        if bindings.is_empty() {
            body
        } else {
            let span = self.end_span(span_start);
            self.alloc(Expr::Let { bindings, body }, span)
        }
    }

    // --- expressions ------------------------------------------------------

    fn expr(&mut self) -> ExprId {
        // Expressions might always be piped
        self.pipe()
    }

    /// `x |> f(a)` desugars to `f(x, a)` by prepending `x` to the argument list
    /// of the right-hand side.
    ///
    /// Chains like `x |> f(a) |> g(b) = f(x, a) |> g(b) = g(f(x, a), b)`.
    fn pipe(&mut self) -> ExprId {
        let span_start = self.start_span();
        let mut lhs = self.binary(0);
        while self.eat(Token::Pipe) {
            let rhs = self.binary(0);

            // TODO: Currently its impossible to express that `a |> f(b)` should
            // mean `f(b)(a)`

            // Desugar `a |> f(b)` to `f(a, b)`. Only real calls absorb the
            // piped value; operators are their own nodes and keep their arity.
            if let Expr::Call { args, .. } = self.module.expr_mut(rhs) {
                args.insert(0, Arg::anon(lhs));
                // rhs has been mutated to `f(a, b)`
                *self.module.span_mut(rhs) = self.end_span(span_start);
                lhs = rhs;
            }
            // Regular `a |> f` to `f(a)`
            else {
                lhs = self.alloc(
                    Expr::Call {
                        callee: rhs,
                        args: vec![Arg::anon(lhs)],
                    },
                    self.end_span(span_start),
                );
            }
        }
        lhs
    }

    /// Left-associative binary operators by precedence level.
    fn binary(&mut self, level: usize) -> ExprId {
        // Which operator each token maps to at this precedence level. `+`/`*`
        // are associative and flatten into one variadic node; the rest stay
        // binary.
        #[derive(Clone, Copy)]
        enum Op {
            Bin(BinaryOp),
            Var(VariadicOp),
        }
        let ops: &[_] = match level {
            0 => &[
                (Token::Lt, Op::Bin(BinaryOp::Lt)),
                (Token::Gt, Op::Bin(BinaryOp::Gt)),
                (Token::Le, Op::Bin(BinaryOp::Le)),
                (Token::Ge, Op::Bin(BinaryOp::Ge)),
                (Token::EqEq, Op::Bin(BinaryOp::Eq)),
                (Token::Ne, Op::Bin(BinaryOp::Ne)),
            ],
            1 => &[
                (Token::Plus, Op::Var(VariadicOp::Sum)),
                (Token::Minus, Op::Bin(BinaryOp::Subtract)),
            ],
            2 => &[
                (Token::Star, Op::Var(VariadicOp::Multiply)),
                (Token::Slash, Op::Bin(BinaryOp::Divide)),
            ],
            _ => return self.maybe_range(),
        };
        let span_start = self.start_span();
        let mut lhs = self.binary(level + 1);

        while let Some(&(_, op)) = ops.iter().find(|(kind, _)| Some(kind) == self.peek()) {
            self.bump();
            let rhs = self.binary(level + 1);
            let span = self.end_span(span_start);
            lhs = match op {
                Op::Bin(op) => self.alloc(Expr::Operator(Operator::Binary { op, lhs, rhs }), span),
                Op::Var(op) => self.push_variadic(op, lhs, rhs, span),
            };
        }

        lhs
    }

    /// Appends `rhs` to `lhs` when it is already a variadic node for the same
    /// operator (so `a + b + c` is one 3-operand node), otherwise starts a new
    /// variadic node.
    fn push_variadic(&mut self, op: VariadicOp, lhs: ExprId, rhs: ExprId, span: Span) -> ExprId {
        if let Expr::Operator(Operator::Variadic {
            op: existing,
            operands,
        }) = self.module.expr_mut(lhs)
            && *existing == op
        {
            operands.push(rhs);
            *self.module.span_mut(lhs) = span;
            return lhs;
        }

        self.alloc(
            Expr::Operator(Operator::Variadic {
                op,
                operands: vec![lhs, rhs],
            }),
            span,
        )
    }

    /// `a..b` range operator. Also returns the bare left operand when there is
    /// no `..`, since this is the entry point below the operator levels.
    fn maybe_range(&mut self) -> ExprId {
        let span_start = self.start_span();
        let lhs = self.unary();
        if self.eat(Token::DotDot) {
            let rhs = self.unary();
            self.alloc(
                Expr::Operator(Operator::Binary {
                    op: BinaryOp::Range,
                    lhs,
                    rhs,
                }),
                self.end_span(span_start),
            )
        } else {
            lhs
        }
    }

    fn unary(&mut self) -> ExprId {
        let span_start = self.start_span();
        if self.eat(Token::Minus) {
            let operand = self.unary();
            self.alloc(
                Expr::Operator(Operator::Unary {
                    op: UnaryOp::Neg,
                    operand,
                }),
                self.end_span(span_start),
            )
        } else {
            self.postfix()
        }
    }

    // fn op_call(
    //     &mut self,
    //     name: &str,
    //     op_span: Span,
    //     operands: Vec<ExprId>,
    // ) -> ExprId {
    //     let span = operands
    //         .map(|id| self.module.span(id))
    //         .fold(op_span, Span::join);
    //     let callee = self.alloc(Expr::Var(Name(name.to_owned())), op_span);
    //     let args = operands.map(|value| Arg { name: None, value }).collect();
    //     self.alloc(Expr::Call { callee, args }, span)
    // }

    // Calls and field accesses.
    fn postfix(&mut self) -> ExprId {
        // Capture the start before the callee/base so its span is included.
        let span_start = self.start_span();
        let mut expr = self.primary();
        loop {
            if self.eat(Token::LParen) {
                let args = self.args();
                self.expect(Token::RParen);
                expr = self.alloc(Expr::Call { callee: expr, args }, self.end_span(span_start));
                continue;
            }

            if self.eat(Token::Dot) {
                let field = self.expect_ident();
                expr = self.alloc(Expr::Field { base: expr, field }, self.end_span(span_start));
                continue;
            }

            break;
        }
        expr
    }

    /// Parses a comma-separated argument list up to (but not consuming) the
    /// closing `)`, which the caller expects. Handles empty lists and trailing
    /// commas.
    fn args(&mut self) -> Vec<Arg> {
        let mut args = Vec::new();
        while !matches!(self.peek(), Some(Token::RParen) | None) {
            let name = match (self.peek(), self.peek_2nd()) {
                (Some(Token::Ident(name)), Some(Token::Eq)) => {
                    let name = Name(*name);
                    self.bump();
                    self.bump();
                    Some(name)
                }
                _ => None,
            };

            let value = self.expr();
            args.push(Arg { name, value });
            if !self.eat(Token::Comma) {
                break;
            }
        }

        args
    }

    fn primary(&mut self) -> ExprId {
        // Increase preemptively position.
        let span_start = self.start_span();
        let next = self.peek().cloned();
        self.bump();
        let token_span = self.end_span(span_start);

        // TODO: This cloned can be avoided.
        match next {
            Some(Token::Int(v)) => self.alloc(Expr::Lit(Literal::Int(v)), token_span),
            Some(Token::Float(v)) => self.alloc(Expr::Lit(Literal::Float(v.into())), token_span),
            Some(Token::Str(v)) => self.alloc(Expr::Lit(Literal::Str(v)), token_span),
            Some(Token::Time(v)) => self.alloc(Expr::Lit(Literal::Time(v)), token_span),
            Some(Token::KwTrue) => self.alloc(Expr::Lit(Literal::Bool(true)), token_span),
            Some(Token::KwFalse) => self.alloc(Expr::Lit(Literal::Bool(false)), token_span),
            // `x -> body` lambda, or a plain variable.
            Some(Token::Ident(name)) if self.eat(Token::Arrow) => {
                let body = self.expr();
                self.alloc(
                    Expr::Lambda {
                        params: vec![Param {
                            name: Name(name),
                            ty: None,
                            default: None,
                        }],
                        body,
                    },
                    self.end_span(span_start),
                )
            }
            // Variable reference needs to go after arrow expression.
            Some(Token::Ident(name)) => self.alloc(Expr::Var(Name(name)), token_span),
            // `.field` accessor shorthand.
            Some(Token::Dot) => {
                let field = self.expect_ident();
                self.alloc(Expr::FieldAccessor { field }, self.end_span(span_start))
            }
            Some(Token::LParen) => {
                let inner = self.expr();
                self.expect(Token::RParen);
                inner
            }
            Some(Token::LBracket) => {
                let mut items = Vec::new();
                while !matches!(self.peek(), Some(Token::RBracket) | None) {
                    items.push(self.expr());
                    if !self.eat(Token::Comma) {
                        break;
                    }
                }
                self.expect(Token::RBracket);
                self.alloc(Expr::List(items), self.end_span(span_start))
            }
            Some(Token::LBrace) => {
                let mut fields = Vec::new();
                while !matches!(self.peek(), Some(Token::RBrace) | None) {
                    let name = self.expect_ident();
                    self.expect(Token::Colon);
                    fields.push((name, self.expr()));
                    if !self.eat(Token::Comma) {
                        break;
                    }
                }
                self.expect(Token::RBrace);
                self.alloc(Expr::Record(fields), self.end_span(span_start))
            }
            other => {
                // Restore preemptive increase (this would only have an effect
                // if the first token after the error is a separator).
                // TODO(test)
                self.pos -= 1;

                // The error span and the error expression here are different.
                // I'm not sure if this will be a problem in the future. My
                // reasoning now is that in error reporting I want to show that
                // the error is specifically at the "start", although it could
                // be nice to see everything that is ignored? Regardless, I'm
                // putting everything in the error expression to avoid having
                // source that is "unmarked" essentially. I think it's good in
                // the AST to know where each part of the source code goes to.
                self.error(ParseErrorKind::ExpectedExpr { found: other }, span_start);
                self.recover_to_sep();
                self.alloc(Expr::Error, self.end_span(span_start))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use num_rational::Rational64;

    #[track_caller]
    fn parse_ok(src: &str) -> Module {
        let (module, errors) = parse(src);
        assert!(errors.is_empty(), "unexpected parse errors: {errors:?}");
        module
    }

    #[track_caller]
    fn body_of<'m>(module: &'m Module, name: &str) -> &'m Expr {
        module.expr(module.def(name).expect("binding not found").body)
    }

    #[test]
    fn time_literals() {
        let module = parse_ok("def t = 250ms\ndef u = 2.5s\n");
        assert_eq!(
            body_of(&module, "t"),
            &Expr::Lit(Literal::Time(Rational64::new(1, 4)))
        );
        assert_eq!(
            body_of(&module, "u"),
            &Expr::Lit(Literal::Time(Rational64::new(5, 2)))
        );
    }

    #[test]
    fn pipeline_desugars_to_prepended_arg() {
        let module = parse_ok("a |> f(b)\n");
        // let module = parse_ok("let c = a |> f(b)\nc");
        let result = module.value.expect("module should have a result");
        let Expr::Call { callee, args } = module.expr(result) else {
            panic!("expected call");
        };
        assert_eq!(module.var_name(*callee), Some("f"));
        assert_eq!(args.len(), 2);
        assert_eq!(module.var_name(args[0].value), Some("a"));
        assert_eq!(module.var_name(args[1].value), Some("b"));
    }

    #[test]
    fn range_desugars_to_operator() {
        let module = parse_ok("def i = 2s..3s\n");
        let Expr::Operator(Operator::Binary { op, .. }) = body_of(&module, "i") else {
            panic!("expected binary operator");
        };
        assert_eq!(*op, BinaryOp::Range);
    }

    #[test]
    fn named_args() {
        let module = parse_ok("def r = rect(color = c, size = s)\n");
        let Expr::Call { args, .. } = body_of(&module, "r") else {
            panic!("expected call");
        };
        assert_eq!(module.name_str(args[0].name.unwrap()), "color");
        assert_eq!(module.name_str(args[1].name.unwrap()), "size");
    }

    #[test]
    fn def_with_let_chain() {
        let src = "def slow(clip: Video, amount: Float = 2.0) -> Video =\n  let factor = 1.0 / amount\n  speed(clip, factor)\n";
        let module = parse_ok(src);
        let def = module.def("slow").unwrap();
        assert_eq!(def.params.len(), 2);
        assert!(def.params[1].default.is_some());
        let Expr::Let { bindings, .. } = module.expr(def.body) else {
            panic!("expected let chain");
        };
        assert_eq!(module.name_str(bindings[0].0), "factor");
    }

    #[test]
    fn field_accessor_shorthand() {
        let module = parse_ok("def c = map(xs, .at)\n");
        let Expr::Call { args, .. } = body_of(&module, "c") else {
            panic!("expected call");
        };

        let Expr::FieldAccessor { field } = module.expr(args[1].value) else {
            panic!("not field accessor");
        };

        assert_eq!(module.name_str(*field), "at");
    }

    #[test]
    fn errors_recover_into_error_nodes() {
        let (module, errors) = parse("def x() = +\ndef y() = 1\n");
        assert!(!errors.is_empty());
        // The second binding still parses despite the first being broken.
        assert!(module.def("y").is_some());
    }

    #[test]
    fn generic_type_annotations() {
        let module = parse_ok("def first(xs: List<Clip<Frame>>) -> Clip<Frame> = head(xs)\n");
        let def = module.def("first").unwrap();
        let Some(TypeExpr::Named { name, args }) = &def.params[0].ty else {
            panic!("expected type");
        };
        assert_eq!(module.name_str(*name), "List");
        assert_eq!(args.len(), 1);
    }
}

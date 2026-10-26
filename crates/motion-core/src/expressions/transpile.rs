use crate::{Error, Result};
use swc_common::{sync::Lrc, FileName, SourceMap, Spanned, DUMMY_SP};
use swc_ecma_ast::*;
use swc_ecma_parser::{lexer::Lexer, EsSyntax, Parser, StringInput, Syntax};
use swc_ecma_visit::{VisitMut, VisitMutWith};

/// A source validation result. Offsets refer to the original UTF-8 draft,
/// before vector adaptation; runtime errors do not have this source mapping.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExpressionSourceDiagnostic {
    pub message: String,
    pub byte_offset: Option<usize>,
}
impl ExpressionSourceDiagnostic {
    fn new(message: impl Into<String>, byte_offset: Option<usize>) -> Self {
        Self { message: message.into(), byte_offset }
    }
}

pub fn expression_source_diagnostic(source: &str) -> Option<ExpressionSourceDiagnostic> {
    checked_transpile(source).err().map(|mut diagnostic| {
        diagnostic.message = diagnostic.message.chars().take(1024).collect();
        diagnostic
    })
}

/// Parse real JavaScript before adapting the vector operators and completion value
/// supported by Motion Studio expressions.
pub(super) fn transpile(source: &str) -> Result<String> {
    checked_transpile(source).map_err(|diagnostic| Error::Invalid(diagnostic.message))
}
fn checked_transpile(source: &str) -> std::result::Result<String, ExpressionSourceDiagnostic> {
    if source.trim().is_empty() || source.len() > 8192 {
        return Err(ExpressionSourceDiagnostic::new("expression source must be 1..8192 bytes", None));
    }
    let cm: Lrc<SourceMap> = Default::default();
    let file = cm.new_source_file(
        FileName::Custom("expression.js".into()).into(),
        source.to_owned(),
    );
    let lexer = Lexer::new(
        Syntax::Es(EsSyntax::default()),
        EsVersion::Es2018,
        StringInput::from(&*file),
        None,
    );
    let mut parser = Parser::new_from(lexer);
    let diagnostic = |e: &swc_ecma_parser::error::Error| {
        let offset = e.span().lo.0.checked_sub(file.start_pos.0).map(|n| n as usize)
            .filter(|&n| n <= source.len() && source.is_char_boundary(n));
        ExpressionSourceDiagnostic::new(format!(
            "JavaScript syntax: {:?} at byte {}",
            e.kind(),
            e.span().lo.0
        ), offset)
    };
    let mut script = parser.parse_script().map_err(|e| diagnostic(&e))?;
    if let Some(e) = parser.take_errors().first() {
        return Err(diagnostic(e));
    }
    let mut adapter = Adapter::default();
    script.visit_mut_with(&mut adapter);
    if adapter.invalid {
        return Err(ExpressionSourceDiagnostic::new(
            "expression exceeds AST limits, uses reserved __ms identifiers or asynchronous JavaScript", None));
    }
    if let Some(tail) = script.body.last_mut() {
        completion(tail);
    }
    Ok(swc_ecma_codegen::to_code(&script))
}

#[cfg(test)]
mod diagnostic_tests {
    use super::*;
    #[test]
    fn source_diagnostics_map_real_parser_spans_without_executing_the_draft() {
        let source = "const 名称 = 1;\nvalue + )";
        let diagnostic = expression_source_diagnostic(source).unwrap();
        assert_eq!(diagnostic.byte_offset, source.find(')'));
        assert!(diagnostic.message.contains("JavaScript syntax:"));
        assert!(transpile(source).unwrap_err().to_string().contains(&diagnostic.message));
        for source in ["while (true) {}", "value + [10, 0, 0]", "wiggle(2, 20)"] {
            assert!(expression_source_diagnostic(source).is_none());
        }
        for source in ["", "async function f() {}", "__msReserved", &"v".repeat(8193)] {
            let diagnostic = expression_source_diagnostic(source).unwrap();
            assert!(diagnostic.byte_offset.is_none());
            assert!(diagnostic.message.len() <= 1024);
            assert!(transpile(source).is_err());
        }
    }
}

fn completion(stmt: &mut Stmt) {
    match stmt {
        Stmt::Expr(e) => {
            *stmt = Stmt::Return(ReturnStmt {
                span: e.span,
                arg: Some(e.expr.clone()),
            })
        }
        Stmt::Block(b) => {
            if let Some(s) = b.stmts.last_mut() {
                completion(s);
            }
        }
        Stmt::If(i) => {
            completion(&mut i.cons);
            if let Some(s) = &mut i.alt {
                completion(s);
            }
        }
        // Other completion forms deliberately return undefined and receive a type error.
        _ => {}
    }
}

#[derive(Default)]
struct Adapter {
    depth: usize,
    count: usize,
    invalid: bool,
}
fn call<const N: usize>(name: &str, args: [Box<Expr>; N]) -> Expr {
    Expr::Call(CallExpr {
        span: DUMMY_SP,
        ctxt: Default::default(),
        callee: Callee::Expr(Box::new(Expr::Ident(Ident::new(
            name.into(),
            DUMMY_SP,
            Default::default(),
        )))),
        args: args
            .into_iter()
            .map(|expr| ExprOrSpread { spread: None, expr })
            .collect(),
        type_args: None,
    })
}
impl VisitMut for Adapter {
    fn visit_mut_function(&mut self, f: &mut Function) {
        if f.is_async {
            self.invalid = true;
        } else {
            f.visit_mut_children_with(self);
        }
    }
    fn visit_mut_ident(&mut self, id: &mut Ident) {
        self.invalid |= id.sym.starts_with("__ms");
    }
    fn visit_mut_stmt(&mut self, stmt: &mut Stmt) {
        self.depth += 1;
        self.count += 1;
        if self.depth > 128 || self.count > 4096 {
            self.invalid = true;
        } else {
            stmt.visit_mut_children_with(self);
        }
        self.depth -= 1;
    }
    fn visit_mut_expr(&mut self, expr: &mut Expr) {
        self.depth += 1;
        self.count += 1;
        if self.depth > 128 || self.count > 4096 {
            self.invalid = true;
            self.depth -= 1;
            return;
        }
        expr.visit_mut_children_with(self);
        match expr {
            Expr::Await(_) => self.invalid = true,
            Expr::Arrow(a) if a.is_async => self.invalid = true,
            Expr::Call(c) if matches!(&c.callee, Callee::Import(_)) => self.invalid = true,
            Expr::Bin(b) => {
                let name = match b.op {
                    BinaryOp::Add => "__msAdd",
                    BinaryOp::Sub => "__msSub",
                    BinaryOp::Mul => "__msMul",
                    BinaryOp::Div => "__msDiv",
                    BinaryOp::Mod => "__msMod",
                    _ => "",
                };
                if !name.is_empty() {
                    *expr = call(name, [b.left.clone(), b.right.clone()]);
                }
            }
            Expr::Unary(u) if u.op == UnaryOp::Minus => {
                *expr = call("__msNeg", [u.arg.clone()]);
            }
            Expr::Assign(a) => {
                let name = match a.op {
                    AssignOp::AddAssign => "__msAdd",
                    AssignOp::SubAssign => "__msSub",
                    AssignOp::MulAssign => "__msMul",
                    AssignOp::DivAssign => "__msDiv",
                    AssignOp::ModAssign => "__msMod",
                    _ => "",
                };
                // Identifier assignments evaluate their left side once. Member assignments
                // retain JS semantics, avoiding duplicate getters or index side effects.
                if let AssignTarget::Simple(SimpleAssignTarget::Ident(i)) = &a.left {
                    if !name.is_empty() {
                        *a.right =
                            call(name, [Box::new(Expr::Ident(i.id.clone())), a.right.clone()]);
                        a.op = AssignOp::Assign;
                    }
                }
            }
            _ => {}
        }
        self.depth -= 1;
    }
}

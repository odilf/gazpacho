//! Compilation of the gazpacho AST into a render graph.
//!
//! The idea here is to take an AST and turn it into a representation that is
//! easily renderable. This comes down to mostly resolving names, for now.

use std::collections::HashMap;

use gazpacho_ast::{Expr, ExprId, Module, Name};

use eyre::{self, OptionExt};

mod graph;

use gazpacho_datatypes::Str;
use gazpacho_operations::{NodeId, NodeInput, Op};
pub use graph::RenderGraph;

/// Compile the [`Module`] into a [`RenderGraph`] and also give the [`NodeId`] of the output.
pub fn compile(module: &Module) -> eyre::Result<(RenderGraph, NodeId)> {
    let mut graph = RenderGraph::new();

    let Some(value) = &module.value else {
        eyre::bail!("Module doesn't have tail expression to evaluate.")
    };

    match eval(*value, module, Env::empty(), &mut graph)? {
        NodeInput::Node(node) => Ok((graph, node)),
        _ => eyre::bail!("Tail expression evaluates to a constant."),
    }
}

#[derive(Debug, Clone)]
struct Env {
    parent: Option<Box<Env>>,
    // TODO: Use `nohash_haser`
    values: HashMap<Str, NodeInput>,
}

impl Env {
    fn empty() -> Env {
        Env {
            parent: None,
            values: HashMap::new(),
        }
    }

    fn get(&self, name: Name) -> Option<NodeInput> {
        self.values
            .get(&name.0)
            .copied()
            .or_else(|| self.parent.as_ref().and_then(|parent| parent.get(name)))
    }
}

/// Evaluate the expression by adding it to the render graph, and return the
/// input that nodes should use.
#[expect(unused_variables)]
fn eval(
    expr: ExprId,
    module: &Module,
    env: Env,
    graph: &mut RenderGraph,
) -> eyre::Result<NodeInput> {
    let value = match module.expr(expr) {
        Expr::Lit(lit) => NodeInput::Constant((*lit).into()),
        Expr::Var(name) => {
            if let Some(value) = env.get(*name) {
                value
            } else {
                let def = module
                    .defs
                    .iter()
                    .find(|def| def.name == *name)
                    .ok_or_else(|| {
                        eyre::eyre!("Couldn't find name `{}`", module.name_str(*name))
                    })?;
                todo!("return raw defs?")
                // eval(def.body, module, env, graph)?
            }
        }
        Expr::Call { callee, args } => match module.expr(*callee) {
            Expr::Var(name) => {
                let op = Op::try_load(
                    name.0,
                    module.strings(),
                    args.iter().map(|arg| {
                        (
                            arg.name.map(|s| s.0),
                            eval(arg.value, module, env.clone(), graph),
                        )
                    }),
                )?
                .ok_or_eyre("Calling a non-builtin variable")?;

                let node = graph.insert(op);
                NodeInput::Node(node)
            }
            // TODO: What kind of expressions are even parseable here?
            other => eyre::bail!("Tried to call an expression of type {}", other.type_name()),
        },
        Expr::Operator(operator) => todo!(),
        Expr::Let { bindings, body } => todo!(),
        Expr::Lambda { params, body } => todo!(),
        Expr::List(expr_ids) => eyre::bail!("Lists not implemented"),
        Expr::Record(items) => eyre::bail!("Records not implemented"),
        Expr::Field { base, field } => eyre::bail!("Records not implemented"),
        Expr::FieldAccessor { field } => eyre::bail!("Records not implemented"),
        Expr::Wgsl { source, inputs } => eyre::bail!("WGSL not implemented"),
        Expr::Script {
            lang,
            source,
            inputs,
        } => eyre::bail!("Custom scripts not implemented"),
        Expr::Error => eyre::bail!("Module has an error."),
    };

    Ok(value)
}

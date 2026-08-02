//! Condition evaluation and AST flattening.

use crate::types::{Condition, EvalContext, NeedsEntry, NeedsItem};

impl Condition
{
    /// Evaluates the condition against the given context.
    pub fn eval(&self, ctx: &EvalContext) -> bool
    {
        match self
        {
            Condition::OsEq(name) => ctx
                .os
                .as_ref()
                .map(|p| p.to_string().to_lowercase() == name.to_lowercase())
                .unwrap_or(false),
            Condition::ManagerPresent(name) => ctx
                .available_managers
                .iter()
                .any(|m| m.eq_ignore_ascii_case(name)),
            Condition::FeaturePresent(name) => ctx.features.contains(name),
            Condition::ModeEq(value) =>
            {
                ctx.mode.as_ref().map(|m| m == value).unwrap_or(false)
            },
            Condition::Not(c) => !c.eval(ctx),
            Condition::And(a, b) => a.eval(ctx) && b.eval(ctx),
            Condition::Or(a, b) => a.eval(ctx) || b.eval(ctx),
        }
    }
}

/// Recursively flattens a list of [`NeedsItem`]s into a plain list of
/// [`NeedsEntry`]s, evaluating conditions against `ctx`.
pub fn flatten(items: &[NeedsItem], ctx: &EvalContext) -> Vec<NeedsEntry>
{
    let mut out = Vec::new();
    for item in items
    {
        match item
        {
            NeedsItem::Entry(e) => out.push(e.clone()),
            NeedsItem::Conditional(block) =>
            {
                if block.condition.eval(ctx)
                {
                    out.extend(flatten(&block.items, ctx));
                }
            },
        }
    }
    out
}

//! Compiled routing rules.
//!
//! A rule pairs an ID with a matcher and an action. Evaluation order lives in
//! [`crate::router`]; this module owns only the rule data type so matching
//! semantics and selection orchestration stay separate.

use crate::matcher::MatchExpr;
use crate::model::{RouteActionSpec, RuleId};

#[derive(Debug, Clone)]
pub struct CompiledRule {
    pub id: RuleId,
    pub matcher: MatchExpr,
    pub action: RouteActionSpec,
}

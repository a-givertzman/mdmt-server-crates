//!
//! 
//! 
//! 
//! 
mod calculations;
mod calculation_graph;

pub use calculations::*;
pub use calculation_graph::*;

///
/// ### Calculation-Graph | Calculation Dependency
/// 
/// Calculation Dependency contains IecKey's of the members accessed on the context for the calculation sequence
#[derive(Debug)]
pub struct CalculationTags {
    /// IecId's, which calculation reads from context.
    pub inputs: Vec<String>,
    /// IecId's, which calculation returns, writes into context.
    /// members stored into the context
    pub outputs: Vec<String>,
}
///
/// ### Calculation-Graph | Calculation Dependencies
/// 
/// Returns Calculation Dependencies for the single calculation sequence
pub trait EvalTags {
    // fn static_tags() -> CalculationTags;
    fn tags(&self) -> CalculationTags;
}

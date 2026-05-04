//!
//! 
//! 
//! 
//! 
mod calculations;

pub use calculations::*;

///
/// ### Calculation-Graph | Calculation Dependency
/// 
/// Calculation Dependency contains IecKey's of the members accessed on the context for the calculation sequence
#[derive(Debug)]
pub struct CalculationTags {
    /// members read from the context
    pub inputs: Vec<&'static str>,
    /// members stored into the context
    pub outputs: Vec<&'static str>,
}
///
/// ### Calculation-Graph | Calculation Dependencies
/// 
/// Returns Calculation Dependencies for the single calculation sequence
pub trait EvalTags {
    // fn static_tags() -> CalculationTags;
    fn tags(&self) -> CalculationTags;
}

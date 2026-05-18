use get_size::GetSize;
use context_macros::{ContextAccess, ContextLoad};
use std::fmt::Debug;

use crate::{algorithm::{ApparentFrequenciesCtx, Parameters, UnitAreaCtx}, domain::{InitialCtx, IecId}};

/// Сырой контекст для вычислений
/// Без изменений берем из SSS
#[derive(Debug, Clone, Default, ContextAccess, ContextLoad, GetSize)]
pub struct RawContext {
    /// Контроль версий для консистентности
    #[context(skip, skip_load)]
    pub(super) version: usize,
    #[context(read)]
    pub(super) initial: InitialCtx,
    #[context(read, write)]
    pub(super) apparent_frequencies: Option<ApparentFrequenciesCtx>,
    #[context(skip_load)]
    pub(super) parameters: Parameters,
    #[context(read, write)]
    pub(super) unit_area: Option<UnitAreaCtx>,
    // ...
}
impl RawContext {
    ///
    /// New instance [RawContext]
    /// - 'initial' - [InitialCtx] instance, where store initial data
    pub fn new(initial: InitialCtx) -> Self {
        Self {
            initial,
            ..Self::default()
        }
    }
}

use context_macros::ContextProperties;
use get_size::GetSize;
use serde::{Deserialize, Serialize};

use crate::algorithm::Moment;

/// Площади поверхности грузов
#[derive(Debug, Clone, Serialize, Deserialize, ContextProperties, GetSize)]
#[iec_id = "Ship.Stability.WindArea.UnitArea"]
pub struct UnitAreaCtx {
    /// Площадь парусности палубного груза, м^2
    pub av_dc: f64,
    /// Cтатический момент площади парусности палубного груза, м^3
    pub mv_dc: Moment,
    /// Изменение момента площади горизонтальных поверхностей палубного груза относительно палубы
    pub delta_moment_h: Moment, 
    /// Распределение площади парусности палубного груза по шпациям, м^2
    pub distr_v: Vec<f64>,
}

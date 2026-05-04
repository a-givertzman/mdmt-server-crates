use std::collections::{HashMap, HashSet, VecDeque};

use crate::{domain::{ContextTransaction, EvalTags}, kernel::{Eval, types::eval_result::EvalResult}};
/// Идентификатор конкретного вычислительного узла.
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
struct CalcId(pub String);
/// Идентификатор параметра IEC, используемый в контексте.
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
struct IecId(pub String);
/// Интерфейс вычислительного узла.
pub trait Calculus: Eval<(), EvalResult> + EvalTags + Send + Sync {
    /// Возвращает уникальный идентификатор расчета.
    fn id(&self) -> CalcId;
    /// Возвращает список IecId, которые расчет использует для чтения.
    fn inputs(&self) -> Vec<IecId>;
    /// Возвращает список IecId, которые расчет модифицирует/возвращает.
    fn outputs(&self) -> Vec<IecId>;
    // /// Выполняет бизнес-логику расчета, мутируя предоставленную транзакцию.
    // fn execute(&self, ctx: &mut ContextTransaction) -> Result<(), String>;
}
/// Диспетчер графа расчетов.
/// Хранит узлы и обеспечивает построение корректного плана выполнения.
pub struct Calculations {
    nodes: HashMap<CalcId, Box<dyn Calculus>>,
    inputs_map: HashMap<IecId, Vec<CalcId>>,
    outputs_map: HashMap<CalcId, Vec<IecId>>,
    global_order: Vec<CalcId>,
}
impl Calculations {
    /// Конструирует граф расчетов и проверяет его на отсутствие циклов.
    pub fn new(calc_nodes: Vec<Box<dyn Calculus>>) -> Result<Self, String> {
        let mut nodes = HashMap::new();
        let mut inputs_map: HashMap<IecId, Vec<CalcId>> = HashMap::new();
        let mut outputs_map: HashMap<CalcId, Vec<IecId>> = HashMap::new();
        for calc in calc_nodes {
            let id = calc.id();
            for input in calc.inputs() {
                inputs_map.entry(input).or_default().push(id.clone());
            }
            for output in calc.outputs() {
                outputs_map.entry(output).or_default().push(id.clone());
            }
            nodes.insert(id, calc);
        }
        let global_order = Self::build_topological_order(&nodes, &inputs_map, &outputs_map)?;
        Ok(Self {
            nodes,
            inputs_map,
            outputs_map,
            global_order,
        })
    }
}

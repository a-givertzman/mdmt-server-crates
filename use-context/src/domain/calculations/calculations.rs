use std::collections::{HashMap, HashSet, VecDeque};

use sal_core::error::Error;

use crate::{domain::{EvalTags, Event}, kernel::{Eval, types::eval_result::EvalResult}};
///
/// Идентификатор конкретного вычислительного узла.
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
struct CalcId(pub String);
///
/// Идентификатор параметра IEC, используемый в контексте.
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
struct IecId(pub String);
///
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
///
/// Диспетчер расчетов.
/// - Стрит граф расчетов на старте
/// - Хранит узлы и обеспечивает построение корректного плана вычислений.
pub struct Calculations {
    nodes: HashMap<CalcId, Box<dyn Calculus>>,
    inputs_map: HashMap<IecId, Vec<CalcId>>,
    outputs_map: HashMap<CalcId, Vec<IecId>>,
    calculation_graph: Vec<CalcId>,
}
impl Calculations {
    ///
    /// Конструирует граф расчетов и проверяет его на отсутствие циклов.
    pub fn new(calculuses: Vec<Box<dyn Calculus>>) -> Result<Self, String> {
        let mut nodes = HashMap::new();
        let mut inputs_map: HashMap<IecId, Vec<CalcId>> = HashMap::new();
        let mut outputs_map: HashMap<CalcId, Vec<IecId>> = HashMap::new();
        for calc in calculuses {
            let id = calc.id();
            for input in calc.inputs() {
                inputs_map.entry(input).or_default().push(id.clone());
            }
            for output in calc.outputs() {
                outputs_map.entry(output).or_default().push(id.clone());
            }
            nodes.insert(id, calc);
        }
        let calculation_graph = Self::build_topological_order(&nodes, &inputs_map, &outputs_map)?;
        Ok(Self {
            nodes,
            inputs_map,
            outputs_map,
            calculation_graph,
        })
    }
    ///
    /// Формирует отсортированный план выполнения на основе измененных ключей.
    pub fn build_plan(&self, changes: &[IecId]) -> Vec<&dyn Calculus> {
        let mut affected_calcs = HashSet::new();
        let mut queue = VecDeque::new();
        for key in changes {
            queue.push_back(key.clone());
        }
        while let Some(current_key) = queue.pop_front() {
            if let Some(dependent_calcs) = self.inputs_map.get(&current_key) {
                for calc_id in dependent_calcs {
                    if affected_calcs.insert(calc_id.clone()) {
                        if let Some(produced_keys) = self.outputs_map.get(calc_id) {
                            for out_key in produced_keys {
                                queue.push_back(out_key.clone());
                            }
                        }
                    }
                }
            }
        }
        self.calculation_graph
            .iter()
            .filter(|id| affected_calcs.contains(*id))
            .filter_map(|id| self.nodes.get(id).map(|c| c.as_ref()))
            .collect()
    }
    ///
    /// Внутренний метод для построения глобального порядка (алгоритм Кана).
    fn build_topological_order(
        nodes: &HashMap<CalcId, Box<dyn Calculus>>,
        inputs_map: &HashMap<IecId, Vec<CalcId>>,
        outputs_map: &HashMap<CalcId, Vec<IecId>>,
    ) -> Result<Vec<CalcId>, String> {
        let mut in_degree: HashMap<CalcId, usize> = nodes.keys().map(|id| (id.clone(), 0)).collect();
        let mut adj_list: HashMap<CalcId, Vec<CalcId>> = HashMap::new();
        for (calc_id, calc) in nodes {
            for out_key in calc.outputs() {
                if let Some(downstream_calcs) = inputs_map.get(&out_key) {
                    for downstream in downstream_calcs {
                        adj_list.entry(calc_id.clone()).or_default().push(downstream.clone());
                        *in_degree.get_mut(downstream).unwrap() += 1;
                    }
                }
            }
        }
        let mut queue: VecDeque<CalcId> = in_degree
            .iter()
            .filter(|(_, deg)| deg == 0)
            .map(|(id, _)| id.clone())
            .collect();
        let mut order = Vec::new();
        while let Some(node) = queue.pop_front() {
            order.push(node.clone());
            if let Some(neighbors) = adj_list.get(&node) {
                for neighbor in neighbors {
                    let deg = in_degree.get_mut(neighbor).unwrap();
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push_back(neighbor.clone());
                    }
                }
            }
        }
        if order.len() != nodes.len() {
            return Err("Обнаружен цикл в графе вычислений. Проверьте связи IecId.".to_string());
        }
        Ok(order)
    }
}
//
impl Eval<Event, Result<(), Error>> for Calculations {
    fn eval(&self, event: Event) -> Result<(), Error> {
        let changes: Vec<(IecId, serde_json::Value)> = vec![];    // To be read from received `Event`
        self.build_plan(changes.iter().map(|(iec_id, _)| iec_id).collect());
        Ok(())
    }
}

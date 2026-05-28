use std::collections::{HashMap, HashSet, VecDeque};
use sal_core::{dbg::Dbg, error::Error};
use crate::{domain::EvalTags, kernel::Eval};
///
/// Идентификатор конкретного вычислительного узла.
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub struct CalcId(pub &'static str);
///
/// Идентификатор параметра IEC, используемый в контексте.
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub(super) struct IecId(pub String);
///
/// Интерфейс вычислительного узла.
pub trait Calculus: Eval<(), Result<(), Error>> + EvalTags + Send + Sync {
    ///
    /// Возвращает уникальный идентификатор расчета.
    fn id(&self) -> CalcId;
}
///
/// ### Диспетчер расчетов.
/// - Строит граф зависимостей на старте 
/// - Обеспечивает корректный порядок вычислений.
pub struct CalculationGraph {
    /// Расчеты
    nodes: HashMap<CalcId, Box<dyn Calculus>>,
    /// На Какие CalcId влияет IecId
    inputs_map: HashMap<IecId, Vec<CalcId>>,
    /// Какие IecId генерирует CalcId
    outputs_map: HashMap<CalcId, Vec<IecId>>,
    /// Список смежности для быстрого поиска зависимых потомков
    adj_list: HashMap<CalcId, Vec<CalcId>>,
    /// Глобально отсортированный порядок выполнения
    global_order: Vec<CalcId>,
    dbg: Dbg,
}
impl CalculationGraph {
    ///
    /// Конструирует Диспетчер расчетов и проверяет граф на отсутствие циклов.
    pub fn new(parent: impl Into<String>, calculuses: impl IntoIterator<Item = Box<dyn Calculus>>) -> Result<Self, Error> {
        let dbg = Dbg::new(parent, "CalculationGraph");
        let mut nodes = HashMap::new();
        let mut inputs_map: HashMap<IecId, Vec<CalcId>> = HashMap::new();
        let mut outputs_map: HashMap<CalcId, Vec<IecId>> = HashMap::new();
        for calc in calculuses {
            let id = calc.id();
            let tags = calc.tags();
            for input in tags.inputs {
                inputs_map.entry(IecId(input.to_owned())).or_default().push(id);
            }
            for output in tags.outputs {
                outputs_map.entry(id).or_default().push(IecId(output.to_owned()));
            }
            nodes.insert(id, calc);
        }
        let (global_order, adj_list) = Self::build_topology(&nodes, &inputs_map, &outputs_map, &dbg)
            .map_err(|err| Error::new(&dbg, "new").pass(err))?;
        Ok(Self {
            nodes,
            inputs_map,
            outputs_map,
            adj_list,
            global_order,
            dbg,
        })
    }
    ///
    ///  Возвращает neighbors
    pub fn neighbors(&self, calc_id: &CalcId) -> Option<&Vec<CalcId>> {
        self.adj_list.get(&calc_id)
    }
    ///
    /// Формирует отсортированный план выполнения на основе изменившихся ключей.
    pub(super) fn plan(&self, changes: impl Iterator<Item = IecId>) -> Vec<&dyn Calculus> {
        let mut affected_calcs = HashSet::new();
        let mut queue = VecDeque::new();
        for key in changes {
            queue.push_back(key);
        }
        while let Some(current_key) = queue.pop_front() {
            if let Some(dependent_calcs) = self.inputs_map.get(&current_key) {
                for calc_id in dependent_calcs {
                    if affected_calcs.insert(calc_id) {
                        if let Some(produced_keys) = self.outputs_map.get(calc_id) {
                            for out_key in produced_keys {
                                queue.push_back(out_key.clone());
                            }
                        }
                    }
                }
            }
        }
        self.global_order
            .iter()
            .filter(|id| affected_calcs.contains(*id))
            .filter_map(|id| self.nodes.get(id).map(|c| c.as_ref()))
            .collect()
    }
    ///
    /// Построение глобального порядка (алгоритм Кана) и списка смежности.
    fn build_topology(
        nodes: &HashMap<CalcId, Box<dyn Calculus>>,
        inputs_map: &HashMap<IecId, Vec<CalcId>>,
        outputs_map: &HashMap<CalcId, Vec<IecId>>,
        dbg: &Dbg,
    ) -> Result<(Vec<CalcId>, HashMap<CalcId, Vec<CalcId>>), Error> {
        let mut in_degree: HashMap<CalcId, usize> = nodes.keys().map(|id| (*id, 0)).collect();
        let mut adj_list: HashMap<CalcId, Vec<CalcId>> = HashMap::new();
        for (calc_id, _) in nodes {
            if let Some(out_keys) = outputs_map.get(calc_id) {
                for out_key in out_keys {
                    if let Some(downstream_calcs) = inputs_map.get(&out_key) {
                        for downstream in downstream_calcs {
                            adj_list.entry(*calc_id).or_default().push(*downstream);
                            *in_degree.get_mut(downstream).unwrap() += 1;
                        }
                    }
                }
            }
        }
        let mut queue: VecDeque<CalcId> = in_degree
            .iter()
            .filter(|(_, deg)| **deg == 0)
            .map(|(id, _)| *id)
            .collect();
        let mut order = Vec::new();
        while let Some(node) = queue.pop_front() {
            order.push(node);
            if let Some(neighbors) = adj_list.get(&node) {
                for neighbor in neighbors {
                    let deg = in_degree.get_mut(neighbor).unwrap();
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push_back(*neighbor);
                    }
                }
            }
        }
        if order.len() != nodes.len() {
            return Err(Error::new(dbg, "build_topology").err("Обнаружен цикл в графе вычислений. Проверьте связи расчетов"));
        }
        Ok((order, adj_list))
    }
}
///
/// Basic tests
#[cfg(test)]
mod tests {
    use super::*;
    use sal_core::error::Error;
    use crate::domain::EvalTags;
    use crate::kernel::Eval;
    /// Мок-объект для эмуляции расчетов
    struct MockCalculus {
        id: CalcId,
        inputs: Vec<String>,
        outputs: Vec<String>,
    }
    impl MockCalculus {
        fn new(id: &'static str, inputs: Vec<&'static str>, outputs: Vec<&'static str>) -> Self {
            Self { id: CalcId(id), inputs: inputs.iter().map(|v| v.to_string()).collect(), outputs: outputs.iter().map(|v| v.to_string()).collect() }
        }
    }
    impl Calculus for MockCalculus {
        fn id(&self) -> CalcId { self.id }
    }
    impl EvalTags for MockCalculus {
        fn tags(&self) -> crate::domain::CalculationTags {
            crate::domain::CalculationTags {
                inputs: self.inputs.clone(),
                outputs: self.outputs.clone(),
            }
        }
    }
    impl Eval<(), Result<(), Error>> for MockCalculus {
        fn eval(&self, _args: ()) -> Result<(), Error> { Ok(()) }
    }
    #[test]
    fn test_graph_initialization_and_order() {
        let calc_a = Box::new(MockCalculus::new("A", vec![], vec!["val_a"])) as Box<dyn Calculus>;
        let calc_b = Box::new(MockCalculus::new("B", vec!["val_a"], vec!["val_b"])) as Box<dyn Calculus>;
        let calc_c = Box::new(MockCalculus::new("C", vec!["val_b"], vec!["val_c"])) as Box<dyn Calculus>;
        let graph = CalculationGraph::new("test", vec![calc_b, calc_c, calc_a]).expect("Graph should build");
        // Проверяем, что граф выстроил правильный топологический порядок, 
        // несмотря на то, что в конструктор расчеты переданы вперемешку
        let order = &graph.global_order;
        assert_eq!(order.len(), 3);
        let pos_a = order.iter().position(|id| *id == CalcId("A")).unwrap();
        let pos_b = order.iter().position(|id| *id == CalcId("B")).unwrap();
        let pos_c = order.iter().position(|id| *id == CalcId("C")).unwrap();
        assert!(pos_a < pos_b);
        assert!(pos_b < pos_c);
    }
    #[test]
    fn test_graph_cycle_detection() {
        // Создаем кольцевую зависимость: A -> B -> C -> A
        let calc_a = Box::new(MockCalculus::new("A", vec!["val_c"], vec!["val_a"])) as Box<dyn Calculus>;
        let calc_b = Box::new(MockCalculus::new("B", vec!["val_a"], vec!["val_b"])) as Box<dyn Calculus>;
        let calc_c = Box::new(MockCalculus::new("C", vec!["val_b"], vec!["val_c"])) as Box<dyn Calculus>;
        let result = CalculationGraph::new("test", vec![calc_a, calc_b, calc_c]);
        assert!(result.is_err());
        let err_msg = format!("{:?}", result.err().unwrap());
        assert!(err_msg.contains("Обнаружен цикл в графе"));
    }
    #[test]
    fn test_plan_generation() {
        // A -> B -> C
        // D -> C
        let calc_a = Box::new(MockCalculus::new("A", vec!["input_root"], vec!["val_a"])) as Box<dyn Calculus>;
        let calc_b = Box::new(MockCalculus::new("B", vec!["val_a"], vec!["val_b"])) as Box<dyn Calculus>;
        let calc_d = Box::new(MockCalculus::new("D", vec!["input_other"], vec!["val_d"])) as Box<dyn Calculus>;
        let calc_c = Box::new(MockCalculus::new("C", vec!["val_b", "val_d"], vec!["val_c"])) as Box<dyn Calculus>;
        let graph = CalculationGraph::new("test", vec![calc_a, calc_b, calc_c, calc_d]).expect("Graph should build");
        // Эмулируем изменение параметра, который генерируется узлом A
        // Ожидаем, что пересчитаются только B и C. Узел D затронут быть не должен.
        let changes = vec![IecId("val_a".to_owned())];
        let plan = graph.plan(changes.into_iter());
        assert_eq!(plan.len(), 2);
        assert_eq!(plan[0].id(), CalcId("B"));
        assert_eq!(plan[1].id(), CalcId("C"));
    }
    #[test]
    fn test_neighbors_adj_list() {
        let calc_a = Box::new(MockCalculus::new("A", vec![], vec!["val_a"])) as Box<dyn Calculus>;
        let calc_b = Box::new(MockCalculus::new("B", vec!["val_a"], vec!["val_b"])) as Box<dyn Calculus>;
        let calc_c = Box::new(MockCalculus::new("C", vec!["val_a"], vec!["val_c"])) as Box<dyn Calculus>;
        let graph = CalculationGraph::new("test", vec![calc_a, calc_b, calc_c]).expect("Graph should build");
        // Проверяем список смежности (соседей) для каскадной инвалидации
        let neighbors_a = graph.neighbors(&CalcId("A")).expect("A must have neighbors");
        assert_eq!(neighbors_a.len(), 2);
        assert!(neighbors_a.contains(&CalcId("B")));
        assert!(neighbors_a.contains(&CalcId("C")));
        assert!(graph.neighbors(&CalcId("B")).is_none() || graph.neighbors(&CalcId("B")).unwrap().is_empty());
    }
}
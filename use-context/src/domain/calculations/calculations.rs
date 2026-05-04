use std::{collections::{HashMap, HashSet, VecDeque}, sync::Arc};
use sal_core::{dbg::Dbg, error::Error};
use crate::{domain::{EvalTags, Event, ProjectNodeStatus, ProjectTree}, kernel::{Eval, sync::Link, types::channel::Sender}};
///
/// Идентификатор конкретного вычислительного узла.
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub struct CalcId(pub &'static str);
///
/// Идентификатор параметра IEC, используемый в контексте.
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
struct IecId(pub &'static str);
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
pub struct Calculations {
    /// Расчеты
    nodes: HashMap<CalcId, Box<dyn Calculus>>,
    /// На Какие CalcId влияет IecId
    inputs_map: HashMap<IecId, Vec<CalcId>>,
    /// Какие IecId генерирует CalcId
    outputs_map: HashMap<CalcId, Vec<IecId>>,
    /// Список смежности для быстрого поиска зависимых потомков
    adj_list: HashMap<CalcId, Vec<CalcId>>,
    /// Глобально отсортированный порядок выполнения
    calculation_graph: Vec<CalcId>,
    /// Ссылка на канал обновления статусов расчетов дерева проекта (`ProjectTree`)
    proj_tree_link: Sender<(CalcId, ProjectNodeStatus)>,
    dbg: Dbg,
}
impl Calculations {
    ///
    /// Конструирует Диспетчер расчетов и проверяет граф на отсутствие циклов.
    pub fn new(parent: impl Into<String>, proj_tree_link: Sender<(CalcId, ProjectNodeStatus)>, calculuses: Vec<Box<dyn Calculus>>) -> Result<Self, Error> {
        let dbg = Dbg::new(parent, "Calculations");
        let mut nodes = HashMap::new();
        let mut inputs_map: HashMap<IecId, Vec<CalcId>> = HashMap::new();
        let mut outputs_map: HashMap<CalcId, Vec<IecId>> = HashMap::new();
        for calc in calculuses {
            let id = calc.id();
            let tags = calc.tags();
            for input in tags.inputs {
                inputs_map.entry(IecId(input)).or_default().push(id);
            }
            for output in tags.outputs {
                outputs_map.entry(id).or_default().push(IecId(output));
            }
            nodes.insert(id, calc);
        }
        let (calculation_graph, adj_list) = Self::build_topology(&nodes, &inputs_map, &outputs_map, &dbg)
            .map_err(|err| Error::new(&dbg, "new").pass(err))?;
        Ok(Self {
            nodes,
            inputs_map,
            outputs_map,
            adj_list,
            calculation_graph,
            proj_tree_link,
            dbg,
        })
    }
    ///
    /// Формирует отсортированный план выполнения на основе изменившихся ключей.
    fn build_plan(&self, changes: impl Iterator<Item = IecId>) -> Vec<&dyn Calculus> {
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
                                queue.push_back(*out_key);
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
//
impl Eval<(Event, Link), Result<(), Error>> for Calculations {
    ///
    /// ### Автоматический пересчет
    /// - Основываясь на изменившихся значениях построит и запустит план вычислений
    /// - `args`
    ///     - Event с изменившимися значениями на фронте
    ///     - Link для отправки событий фронтенду
    fn eval(&self, args: (Event, Link)) -> Result<(), Error> {
        let (event, link) = args;
        let changes: Vec<(IecId, serde_json::Value)> = vec![]; // Читаем из полученного Event
        let calculations = self.build_plan(changes.iter().map(|(iec_id, _)| *iec_id));
        let mut skipped_nodes = HashSet::new();
        for calc in calculations {
            let calc_id = calc.id();
            if skipped_nodes.contains(&calc_id) {
                log::info!("{}.eval | Calculation '{:?}' skipped due to upstream failure.", self.dbg, calc_id);
                // Расчет потерял актуальность
                _ = self.proj_tree_link.send((calc_id, ProjectNodeStatus::Outdated));
                continue;
            }
            if let Err(err) = calc.eval(()) {
                log::warn!("{}.eval | Calculation '{:?}' failed: {:?}", self.dbg, calc_id, err);
                _ = self.proj_tree_link.send((calc_id, ProjectNodeStatus::Error));
                let mut q: VecDeque<&CalcId> = VecDeque::new();
                if let Some(neighbors) = self.adj_list.get(&calc_id) {
                    q.extend(neighbors);
                }
                while let Some(n) = q.pop_front() {
                    if skipped_nodes.insert(*n) {
                        if let Some(next_neighbors) = self.adj_list.get(n) {
                            q.extend(next_neighbors);
                        }
                    }
                }
            } else {
                // Расчет завершен успешно
                _ = self.proj_tree_link.send((calc_id, ProjectNodeStatus::Ready));
                _ = link.send(todo!("Event CmdErr, Calculation {:?} failed", calc_id));
            }
        }
        _ = link.send(todo!("Event CmdCon, Calculation Ok"));
        Ok(())
    }
}

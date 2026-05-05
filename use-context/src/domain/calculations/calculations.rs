use std::collections::{HashSet, VecDeque};
use sal_core::{dbg::Dbg, error::Error};
use crate::{domain::{CalcId, CalculationGraph, Calculus, Event, ProjectNodeStatus, calculations::IecId}, kernel::{Eval, sync::Link, types::channel::Sender}};
///
/// ### Диспетчер расчетов.
/// - Строит граф зависимостей на старте 
/// - Обеспечивает корректный порядок вычислений.
pub struct Calculations {
    /// Граф зависимостей расчетов
    calculation_graph: CalculationGraph,
    /// Ссылка на канал обновления статусов расчетов дерева проекта (`ProjectTree`)
    proj_tree_link: Sender<(CalcId, ProjectNodeStatus)>,
    dbg: Dbg,
}
impl Calculations {
    ///
    /// Конструирует Диспетчер расчетов и проверяет граф на отсутствие циклов.
    pub fn new(parent: impl Into<String>, tree_link: Sender<(CalcId, ProjectNodeStatus)>, calculuses: impl Iterator<Item = Box<dyn Calculus>>) -> Result<Self, Error> {
        let dbg = Dbg::new(parent, "calculations");
        let calculation_graph = CalculationGraph::new(&dbg, calculuses)
            .map_err(|err| Error::new(&dbg, "new").pass(err))?;
        Ok(Self {
            calculation_graph,
            proj_tree_link: tree_link,
            dbg,
        })
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
        let calculations = self.calculation_graph.plan(changes.iter().map(|(iec_id, _)| *iec_id));
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
                if let Some(neighbors) = self.calculation_graph.neighbors(&calc_id) {
                    q.extend(neighbors);
                }
                while let Some(n) = q.pop_front() {
                    if skipped_nodes.insert(*n) {
                        if let Some(next_neighbors) = self.calculation_graph.neighbors(n) {
                            q.extend(next_neighbors);
                        }
                    }
                }
            } else {
                // Расчет вернул ошибку
                _ = self.proj_tree_link.send((calc_id, ProjectNodeStatus::Ready));
                _ = link.send(todo!("Event CmdErr, Calculation {:?} failed", calc_id));
            }
        }
        _ = link.send(todo!("Event CmdCon, Calculation Ok"));
        Ok(())
    }
}

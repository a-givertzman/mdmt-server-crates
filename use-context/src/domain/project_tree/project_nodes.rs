use std::sync::Arc;

use arc_swap::ArcSwap;
use sal_core::dbg::Dbg;

use crate::{domain::{CalcId, ProjectNode, ProjectNodeStatus}, kernel::types::fx_map::FxDashMap};

///
/// ### Коллекция узлов дерева проекта
pub struct ProjectNodes {
    nodes: FxDashMap<String, Arc<ProjectNode>>,
    updated_nodes: ArcSwap<FxDashMap<String, Arc<ProjectNode>>>,
    dbg: Dbg,
}
//
impl ProjectNodes {
    pub fn new(parent: impl Into<String>) -> Self {
        Self {
            nodes: FxDashMap::default(),
            updated_nodes: ArcSwap::new(Arc::new(FxDashMap::default())),
            dbg: Dbg::new(parent, "ProjectNodes"),
        }
    }
    ///
    /// ### Агрегация статусов дерева наверх
    /// 
    /// **Механизм агрегации**
    /// - Происходит по события в PT-link, это очередь событий
    /// - В каждом цикле (64...120мс) вычитываться полностью.
    /// - Пересчет делаем по всем полученным событиям
    /// - Затем отправка статусов в UI
    pub fn update_status(&self, node_id: CalcId, node_status: ProjectNodeStatus) {
        
    }
    ///
    /// Возвращает изменившиеся ноды
    /// - Если изменился статус
    pub fn get_updated(&self) -> Vec<(String, Arc<ProjectNode>)> {
        // Атомарно забираем все изменившиеся ноды и оставляем на их месте пустую мапу
        let updated = self.updated_nodes.swap(Arc::new(FxDashMap::default()));
        // Вытаскиваем ноды по ключам
        match Arc::try_unwrap(updated) {
            Ok(map) => map.into_iter().collect(),
            Err(arc_map) => {
                // Если кто-то еще держал ссылку на эту мапу, клонируем элементы
                arc_map.iter().map(|r| (r.key().clone(), r.value().clone())).collect()
            }
        }
    }
}

///
/// Базовые тесты
#[cfg(test)]
mod tests {
    use crate::domain::ProjectNodeKind;

    use super::*;
    use std::thread;
    use std::time::Duration;
    #[test]
    fn test_arc_swap_nodes_flow() {
        let project = Arc::new(ProjectNodes::new("test_parent"));
        // 1. Симулируем фоновый поток, генерирующий события
        let p_clone = project.clone();
        let handle = thread::spawn(move || {
            // Эмулируем задержку между тиками очереди
            thread::sleep(Duration::from_millis(10));
            let node_arc = Arc::new(ProjectNode::new(0, 0, 0, 0, ProjectNodeKind {}, 0));
            // Вручную делаем то, что должен делать update_status
            p_clone.nodes.insert("node_1".to_string(), node_arc.clone());
            p_clone.updated_nodes.load().insert("node_1".to_string(), node_arc);
        });
        // 2. Главный поток забирает обновления
        // Сначала там пусто
        let empty_updates = project.get_updated();
        assert!(empty_updates.is_empty());
        // Ждем поток
        handle.join().unwrap();
        // 3. Теперь обновления должны появиться
        let mut updates = project.get_updated();
        assert_eq!(updates.len(), 1);
        let (key, node) = updates.pop().unwrap();
        assert_eq!(key, "node_1");
        assert_eq!(node.status, ProjectNodeStatus::Outdated);
        // 4. После вызова get_updated мапа должна была очиститься
        let updates_after_clear = project.get_updated();
        assert!(updates_after_clear.is_empty(), "Мапа обновлений должна быть пустой!");
    }
}

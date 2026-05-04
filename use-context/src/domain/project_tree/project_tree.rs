use std::{sync::Arc, time::Duration};
use sal_core::{dbg::Dbg, error::Error};
use sal_sync::{kernel::state::ExitNotify, services::{Service, entity::{Name, Object}}, sync::{Handles, Owner}, thread_pool::Scheduler};

use crate::{domain::{CalcId, Event, ProjectNodeStatus, ProjectNodes, ProjectTreeConf}, kernel::types::channel::{self, Receiver, RecvTimeoutError, Sender} };

///
/// ### Service | ProjectTree
/// 
/// Это навигационный граф, связывающий воедино исходные данные,
/// результаты расчетов,  математику, тэги зависимостей расчетовб 3D модель,
/// отчеты, и актуальные статусов для бэкенда и пользователя.
///
/// Работает в самостоятельном потоке
pub struct ProjectTree {
    name: Name,
    conf: ProjectTreeConf,
    /// Канал для отправки событий слиенту
    client_link: Owner<Sender<Event>>,
    /// Внешний кончик канала, в который расчеты будут отправлять статусы нод
    link_tx: Sender<(CalcId, ProjectNodeStatus)>,
    /// Тут получаем статусы нод от расчетов
    link_rx: Owner<Receiver<(CalcId, ProjectNodeStatus)>>,
    nodes: Arc<ProjectNodes>,
    scheduler: Scheduler,
    handles: Handles<()>,
    exit: Arc<ExitNotify>,
    dbg: Dbg,
}
//
//
impl ProjectTree {
    //
    /// Crteates new instance of the [ProjectTree] 
    pub fn new(parent: impl Into<String>, conf: ProjectTreeConf, client: Sender<Event>, scheduler: Scheduler) -> Self {
        let name = Name::new(parent, "ProjectTree");
        let dbg = Dbg::new(name.parent(), name.me());
        let (link_tx, rx) = channel::channel::unbounded();
        Self {
            name,
            conf,
            client_link: Owner::new(client),
            link_tx,
            link_rx: Owner::new(rx),
            nodes: Arc::new(ProjectNodes::new(&dbg)),
            scheduler,
            handles: Handles::new(&dbg),
            exit: Arc::new(ExitNotify::new(&dbg,None, None)),
            dbg,
        }
    }
    ///
    /// Формирование эвентов статусов для фронта
    /// - `prev` - версии нод до пересчета
    fn prepare_status_events(nodes: &Arc<ProjectNodes>) -> Vec<Event> {
        // Прверяем если статус ноды изменился, то генерим эвент с новым статусом
        let nodes = nodes.get_updated();
        nodes.into_iter().map(|(iec_id, node)| {
            Event {}
        }).collect()
    }
    ///
    /// Возвращает link для обновления событий
    pub fn link(&self) -> Sender<(CalcId, ProjectNodeStatus)> {
        self.link_tx.clone()
    }
}
//
//
impl Object for ProjectTree {
    fn name(&self) -> Name {
        self.name.clone()
    }
}
//
// 
impl std::fmt::Debug for ProjectTree {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProjectTree")
            .field("name", &self.name)
            .finish()
    }
}
//
// 
impl Service for ProjectTree {
    //
    fn run(&self) -> Result<(), Error> {
        let dbg = self.dbg.clone();
        let error = Error::new(&dbg, "run");
        log::info!("{dbg}.run | Starting...");
        let handle = self.scheduler.spawn({
            let dbg = dbg.clone();
            let exit = self.exit.clone();
            let link_rx = self.link_rx.take().ok_or(error.err("Can't take link_rx from self"))?;
            let client_link = self.client_link.take().ok_or(error.err("Can't take client_link from self"))?;
            let recv_timeout = Duration::from_millis(100);
            let nodes = self.nodes.clone();
            move || {
                log::info!("{dbg}.run | Ready");
                while !exit.get() {
                    let mut is_empty = false;
                    let mut events = vec![];
                    while !is_empty {
                        match link_rx.recv_timeout(recv_timeout) {
                            Ok(event) => {
                                events.push(event);
                            }
                            Err(err) => match err {
                                RecvTimeoutError::Timeout => is_empty = true,
                                _ => exit.exit(),
                            },
                        }
                    }
                    // Пересчет статусов нод дерева
                    for (node_id, node_status) in events {
                        nodes.update_status(node_id, node_status);
                    }
                    // Формирование всех изменившихся статусов нод дерева
                    let client_events = Self::prepare_status_events(&nodes);
                    for e in client_events {
                        if let Err(err) = client_link.send(e) {
                            log::warn!("{dbg}.run | Can't send event to the clint: {err}");
                        }
                    }
                }
                Ok(())
            }
        }).map_err(|err| error.pass_with("Start failed", err))?;
        self.handles.push(handle);
        log::info!("{dbg}.run | Starting - Ok");
        Ok(())
    }
    //
    fn is_finished(&self) -> bool {
        self.handles.is_finished()
    }
    //
    fn wait(&self) -> Result<(), Error> {
        self.handles.wait()
    }
    //
    fn exit(&self) {
        self.exit.exit();
    }    
}
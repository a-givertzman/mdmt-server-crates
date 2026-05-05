mod algorithm;
mod domain;
mod kernel;

use std::{sync::Arc, time::Instant};

use debugging::session::debug_session::{DebugSession, LogLevel};
use sal_core::{dbg::Dbg, error::Error};
use sal_sync::{sync::channel, thread_pool::ThreadPool};

use crate::{
    algorithm::{ApparentFrequenciesCtx, Bound, Bounds, Position, UnitAreaCtx, UnitAreaEval},
    domain::{ApiClient, Calculations, Conf, Context, ContextRead, ContextWrite, EvalTags, IecId, Initial, InitialCtx, ProjectTree, Properties},
    kernel::Eval,
};


fn main() -> Result<(), Error> {
    DebugSession::new()
        .filter(LogLevel::Debug)
        .init();
    let dbg = Dbg::own("main");
    let ship_id = "Ship";
    let project_id = "Project";
    let conf = Conf::read("./config.yaml").map_err(|err| Error::new(&dbg, "").err(err))?;
    let tp = ThreadPool::new(&dbg, conf.thread_pool);
    let (client, _) = channel::unbounded();
    let project_tree = ProjectTree::new(
        &dbg,
        conf.project_tree,
        client,
        tp.scheduler(),
    );
    let bounds = Bounds::new(
        vec![
            Bound::new(0.0, 1.0)?,                         // 1. Процент
            Bound::new(0.0, 255.0)?, Bound::new(0.0, 255.0)?,   // 2. 2D нормализация
            Bound::new(-90.0, 90.0)?, Bound::new(-180.0, 180.0)?, // 3. Гео-координаты
            Bound::new(20.0, 20000.0)?,                   // 4. Звуковые частоты
            Bound::new(0.0, 1920.0)?, Bound::new(0.0, 1080.0)?,  // 5. HD Разрешение
            Bound::new(1.0, 100.0)?, Bound::new(1.0, 100.0)?,    // 6. Сетка 100x100
            Bound::new(-1.0, 1.0)?, Bound::new(-1.0, 1.0)?, Bound::new(-1.0, 1.0)?, // 7. Unit cube
            Bound::new(0.0, 1000.0)?,                     // 8. Дистанция (м)
            Bound::new(18.0, 65.0)?,                       // 9. Рабочий возраст
            Bound::new(300.0, 800.0)?,                    // 10. Спектр света (нм)
            Bound::new(0.0, 360.0)?,                      // 11. Угол (градусы)
            Bound::new(-40.0, 125.0)?,                    // 12. Температура чипа
            Bound::new(0.8, 1.2)?,                        // 13. Множитель (±20%)
            Bound::new(0.0, 60.0)?, Bound::new(0.0, 60.0)?,     // 14. Время (мин/сек)
            Bound::new(100.0, 10000.0)?,                  // 15. Обороты двигателя
        ]
    )?;
    let ctx = Arc::new(Context::new(InitialCtx::new(ship_id, project_id, bounds)));
    let ua_calc = Box::new(UnitAreaEval::new(
        &dbg,
        Initial::new(
            &dbg,
            ctx.clone(),
        ),
    ));
    ua_calc.eval(()).unwrap();
    let calculations = Calculations::new(&dbg, project_tree.link(), [
    ]);
    let (send, _) = channel::unbounded();
    let client = Arc::new(ApiClient {});
    log::debug!("All prepared, starting threads...:");
    let h1 = std::thread::spawn({
        let t = Instant::now();
        let ctx = ctx.transaction(send.clone(), client.clone());
        log::debug!("Thread 1 | Start Transaction elapsed: {:?}", t.elapsed());
        move || {
            let initial: InitialCtx = ctx.read();
            let result = ApparentFrequenciesCtx {
                apparent_frequencies: vec![
                    (1.39, 40.55, 25.39),
                    (44.79, 54.97, 32.8),
                    (66.07, 25.23, 28.96),
                    (82.68, 8.06, 18.49),
                    (54.24, 31.51, 77.67),
                    (4.03, 72.35, 85.27),
                    (37.31, 61.39, 47.22),
                    (42.9, 74.86, 42.0),
                ],
            };
            let id = ApparentFrequenciesCtx::iec_id();
            log::debug!("Thread 1 | ApparentFrequenciesCtx ID: {id}");
            // let properties = result.properties();
            // log::debug!("Thread 1 | ApparentFrequenciesCtx properies: {:?}", properties);
            let ctx = ctx.write(result).unwrap();
            if let Err((_, err)) = ctx.commit() {
                log::warn!("Thread 1 | Error: {err}");
            }
    }});
    let h2 = std::thread::spawn({
        let t = Instant::now();
        let ctx = ctx.transaction(send, client);
        log::debug!("Thread 2 | Start Transaction elapsed: {:?}", t.elapsed());
        move || {
            let initial: InitialCtx = ctx.read();
            let result = UnitAreaCtx {
                av_dc: 34.12,
                mv_dc: Position::new(42.9, 42.8, 42.7),
                delta_moment_h: Position::zero(),
                distr_v: vec![1.39, 44.79, 66.07, 82.68, 54.24, 4.03, 37.31, 42.9],
            };
            let id = UnitAreaCtx::iec_id();
            log::debug!("Thread 2 | UnitAreaCtx ID: {id}");
            let properties = result.properties();
            log::debug!("Thread 2 | UnitAreaCtx properies: {:?}", properties);
            let ctx = ctx.write(result).unwrap();
            if let Err((_, err)) = ctx.commit() {
                log::warn!("Thread 2 | Error: {err}");
            }
    }});
    // let tags = UnitAreaEval::new();
    // log::info!("UnitArea tags: {:?}", tags);
    h1.join().unwrap();
    h2.join().unwrap();
    println!("Context: {:#?}", ctx);
    Ok(())
}

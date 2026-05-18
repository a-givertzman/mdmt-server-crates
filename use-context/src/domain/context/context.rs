use get_size::GetSize;
use sal_core::error::Error;
use sal_sync::sync::{RwLock, channel::Sender};
use std::{fmt::Debug, sync::Arc};

use crate::{algorithm::Parameters, domain::{ApiClient, ContextTransaction, Event, InitialCtx, RawContext, Snapshot}};

/// Контекст для вычислений
/// - Потокобезопасен
/// - Не публикует прямого доступа к данным
/// - Позволяет выполнять запись транзакцииями
pub struct Context {
    raw: Arc<RwLock<Arc<RawContext>>>,
}
impl Context {
    /// [Context] new instance
    /// - `initial` - [InitialCtx] instance, where store initial data
    pub fn new(initial: InitialCtx) -> Self {
        Self {
            raw: Arc::new(RwLock::new(Arc::new(RawContext {
                version: 0,
                initial,
                apparent_frequencies: None,
                // Предполагаем реализацию Default или аналогичную логику:
                parameters: Parameters::default(),
                unit_area: None,
            }))),
        }
    }
    ///
    /// Обогащает контекст данными из БД.
    /// Выполняется единожды на этапе старта приложения.
    pub fn with_snapshot(self, properties: impl IntoIterator<Item = (String, serde_json::Value)>) -> Self {
        let raw_clone = {
            let raw_guard = self.raw.read();
            (**raw_guard).clone()
        };
        let (raw, report) = raw_clone.with_snapshot(properties);
        log::debug!("Context.with_snapshot | load report: {:#?}", report);
        *self.raw.write() = Arc::new(raw);
        self
    }
    /// Returns [ContextTransaction] to start a transaction
    /// - [ContextTransaction] - accumulates multiple results to be applied to the [Context] later by calling [ContextTransaction]`.commit` or `.rollback`
    pub fn transaction(&self, link: Sender<Event>, api_client: Arc<ApiClient>) -> ContextTransaction {
        let current = self.raw.read();
        ContextTransaction {
            origin: Arc::clone(&self.raw),
            state: (**current).clone(),
            snapshot: Snapshot::new(link, api_client)
        }
    }
    ///
    /// Returns full size of the [Context] in the bytes
    pub fn get_size(&self) -> usize {
        self.raw.read().get_size()
    }
}
impl Default for Context {
    fn default() -> Self {
        Self {
            raw: Arc::new(
                RwLock::new(
                    Arc::new(
                        RawContext::new(InitialCtx::default()),
                    )
                )
            )
        }
    }
}
impl Debug for Context {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Context").field("raw", &(*self.raw.read())).finish()
    }
}


/// 
/// Basic tests
#[cfg(test)]
mod tests {
    use sal_sync::sync::channel;

    use super::*;
    // Для тестов нам потребуется создать базовый InitialCtx, если у него нет Default
    // Предполагаем, что InitialCtx::default() доступен, так как он используется в Context::default()
    #[test]
    fn test_context_initialization() {
        let ctx = Context::default();
        let current = ctx.raw.read();
        assert_eq!(current.version, 0, "Initial version must be 0");
    }
    #[test]
    fn test_successful_commit() {
        let ctx = Context::default();
        let (send, _) = channel::unbounded();
        let client = Arc::new(ApiClient {});
        let tx = ctx.transaction(send, client);
        // В рамках теста транзакция ничего не меняет, кроме версии
        let result = tx.commit();
        assert!(result.is_ok(), "Commit should succeed on untouched context");
        let current = ctx.raw.read();
        assert_eq!(current.version, 1, "Version must be incremented after commit");
    }
    #[test]
    fn test_concurrent_conflict_resolution() {
        let ctx = Context::default();
        let (send, _) = channel::unbounded();
        let client = Arc::new(ApiClient {});
        // Создаем две независимые транзакции из одной отправной точки (version = 0)
        let tx1 = ctx.transaction(send.clone(), client.clone());
        let tx2 = ctx.transaction(send, client);
        // Первая транзакция успешно завершается
        assert!(tx1.commit().is_ok(), "First commit should succeed");
        assert_eq!(ctx.raw.read().version, 1, "Version is now 1");
        // Вторая транзакция должна провалиться из-за несовпадения версий
        let result = tx2.commit();
        assert!(result.is_err(), "Second commit must fail due to version mismatch");
        if let Err((_failed_tx, err)) = result {
            // Проверяем, что вернулась именно ожидаемая ошибка
            let err_msg = format!("{:?}", err);
            assert!(err_msg.contains("Context already was changed"));
        }
    }
    #[test]
    fn test_force_commit_overrides_conflict() {
        let ctx = Context::default();
        let (send, _) = channel::unbounded();
        let client = Arc::new(ApiClient {});
        let tx1 = ctx.transaction(send.clone(), client.clone());
        let tx2 = ctx.transaction(send, client);
        // Первая транзакция меняет версию на 1
        assert!(tx1.commit().is_ok());
        // Вторая транзакция игнорирует конфликт и форсирует запись
        let result = tx2.force_commit();
        assert!(result.is_ok(), "Force commit should succeed regardless of version");
        // Версия оригинала должна стать 2 (version из origin_lock + 1)
        let current = ctx.raw.read();
        assert_eq!(current.version, 2, "Version must increment based on the latest origin version");
    }
    #[test]
    fn test_rollback_drops_changes() {
        let ctx = Context::default();
        let (send, _) = channel::unbounded();
        let client = Arc::new(ApiClient {});
        let tx = ctx.transaction(send, client);
        let initial_version = ctx.raw.read().version;
        tx.rollback();
        // Проверяем, что оригинал не изменился
        assert_eq!(ctx.raw.read().version, initial_version, "Version must remain unchanged after rollback");
    }
    #[test]
    fn test_concurrent_stress_occ() {
        use std::thread;
        // Оборачиваем Context в Arc для раздачи по потокам
        let ctx = Arc::new(Context::default());
        let (send, _) = channel::unbounded();
        let client = Arc::new(ApiClient {});
        let mut handles = vec![];
        let num_threads = 10;
        let commits_per_thread = 100;
        for _ in 0..num_threads {
            let thread_ctx = Arc::clone(&ctx);
            handles.push(thread::spawn({
                let send = send.clone();
                let client = client.clone();
                move || {
                for _ in 0..commits_per_thread {
                    loop {
                        // 1. Берем снимок (старт транзакции)
                        let tx = thread_ctx.transaction(send.clone(), client.clone());
                        // 2. Имитируем небольшую задержку на "вычисления"
                        thread::yield_now();
                        // 3. Пробуем зафиксировать результат
                        match tx.commit() {
                            Ok(_) => break, // Успешно записали, идем к следующему расчету
                            Err(_) => continue, // Конфликт версий! Берем новый снимок и повторяем расчет
                        }
                    }
                }
            }}));
        }
        // Дожидаемся завершения всех потоков
        for handle in handles {
            handle.join().expect("Thread panicked");
        }
        let final_state = ctx.raw.read();
        let expected_version = num_threads * commits_per_thread;
        // Если хотя бы одна транзакция перетерла чужие данные или потерялась, 
        // итоговая версия не совпадет с ожидаемым количеством успешных коммитов
        assert_eq!(
            final_state.version, 
            expected_version, 
            "All commits must be strictly serialized without lost updates"
        );
    }
    #[test]
    fn test_concurrent_high_load_occ() {
        let initial = InitialCtx::default();
        let (send, _) = channel::unbounded();
        let client = Arc::new(ApiClient {});
        let context = Arc::new(Context::new(initial));
        let num_threads = 10;
        let updates_per_thread = 100;
        let mut handles = vec![];
        for _ in 0..num_threads {
            let ctx_clone = Arc::clone(&context);
            handles.push(std::thread::spawn({
                let send = send.clone();
                let client = client.clone();
                move || {
                for _ in 0..updates_per_thread {
                    loop {
                        let tx = ctx_clone.transaction(send.clone(), client.clone());
                        // Имитируем успешный коммит. В реальной жизни здесь будут изменения данных.
                        match tx.commit() {
                            Ok(_) => break, // Успех, идем к следующему обновлению
                            Err(_) => continue, // Конфликт версий, повторяем попытку
                        }
                    }
                }
            }}));
        }
        for handle in handles {
            handle.join().unwrap();
        }
        let final_tx = context.transaction(send, client);
        // Проверяем, что ни одна транзакция не потерялась
        assert_eq!(final_tx.state.version, num_threads * updates_per_thread);
    }
    #[test]
    fn test_context_get_size() {
        let context = Context::new(InitialCtx::default());
        // Просто проверяем, что метод отрабатывает и возвращает ненулевой размер
        assert!(context.get_size() > 0);
    }
}

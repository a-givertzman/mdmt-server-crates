//! Временный стейт используемый вычислительным пайплайном для накопления результатов
//! - Атомарная вставки в базу данных
//! - Так же закладываем архитектурное решение для 
//! - Отправка клиентам в случае успеха
//! - При неудачных вычислениях удаляется из памяти
//! - Адаптер `Context` <-> `DB.propertyes`
mod snapshot;

pub use snapshot::*;

///
/// Trait for converting [Context] members into key-value properties
/// - Context -> `DB.properties` adapter
pub trait Properties {
    fn properties(&self) -> Vec<(String, String)>;
}

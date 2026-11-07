mod context_transaction;
mod context;
mod initial_ctx;
mod initial;
mod raw_context;

pub use context_transaction::*;
pub use context::*;
pub use initial_ctx::*;
pub use initial::*;
pub use raw_context::*;

use sal_core::error::Error;

use crate::kernel::JsonVal;

pub trait ContextRead<T> {
    fn read(&self) -> Option<T>;
}

pub trait ContextReadRef<T> {
    fn read_ref(&self) -> Option<&T>;
}

pub trait ContextWrite<T>: Sized {
    fn write(self, value: T) -> Result<Self, sal_core::error::Error>;
}

pub trait IecId {
    fn iec_id() -> &'static str;
}

pub trait Properties {
    fn properties(&self) -> Result<Vec<(String, crate::kernel::JsonVal)>, sal_core::error::Error>;
}

///
/// ### Результат чтения/записи database-JSON в `RawContext`
#[derive(Debug)]
pub enum DbContextAccessError {
    /// Неизвестный IEC-идентификатор
    Unknown(sal_core::error::Error),
    /// Дублирование IEC-идентификатора в структуре элемента контекста
    Duplicated(sal_core::error::Error),
    /// Ошибка доступа или конвертации
    Err(sal_core::error::Error),
}
///
/// ### Trait for converting domain types of the [Context] members into database - Json and vice versa.
/// - Context -> `database-properties` adapter
pub trait DbProperties {
    /// ### Проверяет наличие поля в элементе контекста.
    fn has_field(&self, field: &str) -> bool;

    /// ### Записывает в элемент контекста одно поле по `iec_id` в формате domain-JSON.
    /// - `iec_id` - полный IEC-идентификатор значения.
    /// - `field` - имя поля в структуре элемента контекста.
    fn write_json(&mut self, iec_id: &str, field: &str, val: &JsonVal,
    ) -> Result<(), DbContextAccessError>;

    /// ### Читает из элемента контекста по `iec_id` одно поле в формате domain-JSON.
    /// - `iec_id` - полный IEC-идентификатор значения.
    /// - `field` - имя поля в структуре элемента контекста.
    fn read_json(&self, iec_id: &str, field: &str,
    ) -> Result<JsonVal, DbContextAccessError>;
}

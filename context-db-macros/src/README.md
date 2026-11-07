# context-db-macros

Макросы доступа к полям `RawContext` в формате domain-JSON — в том виде,
в котором значения хранятся в `db.properties.value`.

БД хранит не элемент контекста целиком, а отдельные его поля:
`db.property_meta.key = <iec_id элемента>.<имя поля>`, `db.properties.value` — JSON
этого поля. Поэтому загрузка идёт полем, а не объектом. `UiConverter` здесь
не участвует: UI-представление и domain-JSON — разные форматы.

| Derive | Цель | Что генерирует |
| --- | --- | --- |
| `DbContextAccess` | `RawContext` | inherent-методы `db_read` / `db_write` |
| `DbContextProperties` | элемент контекста | `impl DbProperties` |

## Контракт потребителя

Макрос не объявляет типы приложения. В `crate::domain` должны быть:

```rust
pub trait IecId {
    fn iec_id() -> &'static str;
}

#[derive(Debug)]
pub enum DbContextAccessError {
    Unknown(sal_core::error::Error),
    Duplicated(sal_core::error::Error),
    Err(sal_core::error::Error),
}

pub trait DbProperties {
    fn has_field(&self, field: &str) -> bool;
    fn write_json(&mut self, iec_id: &str, field: &str, val: &JsonVal)
        -> Result<(), DbContextAccessError>;
    fn read_json(&self, iec_id: &str, field: &str)
        -> Result<JsonVal, DbContextAccessError>;
}
```

`JsonVal` — это `crate::kernel::JsonVal` (`serde_json::Value`).
Элементы контекста обязаны реализовывать `Serialize + Deserialize`.
Типы, которые загружаются в `Option<Ctx>`, обязаны реализовывать `Default`:
макрос создаёт элемент при первой записи.

## DbContextAccess

Генерирует на структуре два метода:

```rust
pub fn db_read(&self, iec_id: impl AsRef<str>) -> Result<JsonVal, DbContextAccessError>;
pub fn db_write(&mut self, iec_id: impl AsRef<str>, val: &JsonVal) -> Result<(), DbContextAccessError>;
```

`iec_id` разбирается по последней точке: `<iec_id элемента>.<имя поля>`.
Префикс сравнивается с `<ТипПоля as IecId>::iec_id()`, имя поля передаётся
в `DbProperties`.

- структура обязана иметь именованные поля и не иметь generic-параметров;
- поле исключается флагом `#[context(skip_load)]`;
- `ui_skip`, `skip`, `read`, `read_ref`, `write` и любые неизвестные флаги
  игнорируются — это флаги соседних derive;
- повтор любого флага в `#[context(...)]` — ошибка компиляции.

Поведение:

| Ситуация | `db_read` | `db_write` |
| --- | --- | --- |
| `iec_id` без точки | `Unknown` | `Unknown` |
| Префикс не найден | `Unknown` | `Unknown` |
| Поле не найдено в элементе | `Unknown` | `Unknown`, элемент не создаётся |
| Одинаковый IEC-префикс у двух полей | `Duplicated` | Значение пишется в первое поле, затем `Duplicated` |
| `Option<Ctx>` = `None`, поле известно | `JsonVal::Null` | Создаётся `Ctx::default()`, затем запись |
| `Option<T>` внутри элемента | `null` для `None` | `None` для `null` |
| Ошибка `serde` | `Err` | `Err`, значение поля не меняется |

Дублирование определяется по `IecId::iec_id()`: если два поля `RawContext`
претендуют на один префикс, чтение неоднозначно, а запись выбирает первое поле.

## DbContextProperties

Генерирует `impl DbProperties` для элемента контекста. На каждое поле без
`skip_load` создаётся ветка `match` по PascalCase и snake_case написаниям
имени поля — так же, как в UI-слое.

- `has_field` — проверка имени поля без обращения к значению;
- `read_json` — `serde_json::to_value(&self.field)`;
- `write_json` — `serde::Deserialize::deserialize(val)`, затем присваивание;
  значение принимается по ссылке, клонирования нет;
- `Option<T>` внутри элемента обрабатывается обычным `serde`: `None`
  сериализуется в `null` и восстанавливается из `null`;
- структура обязана иметь именованные поля; generic-структуры требуют явных
  bounds `Serialize + DeserializeOwned` на полях.

Ограничения: макрос не читает `#[serde(rename)]`, `#[serde(rename_all)]`,
`#[serde(flatten)]` и `#[serde(skip_deserializing)]`. Ключ в БД — имя поля
структуры. Если для поля нужен другой ключ, используйте `skip_load` и
обрабатывайте его вручную.

## Пример

```rust
use context_db_macros::{DbContextAccess, DbContextProperties};
use context_macros::{ContextAccess, ContextProperties};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, ContextProperties, DbContextProperties)]
#[iec_id = "Ship"]
pub struct ShipCtx {
    pub name: String,
    pub keel_area: Option<f64>,
}

#[derive(Debug, Clone, Default, ContextAccess, DbContextAccess)]
pub struct RawContext {
    #[context(read, write)]
    pub ship: Option<ShipCtx>,
}
```

```rust
let mut ctx = RawContext::default();
ctx.db_write("Ship.name", &serde_json::json!("Sofia")).unwrap();
assert_eq!(ctx.db_read("Ship.name").unwrap(), serde_json::json!("Sofia"));
assert_eq!(ctx.db_read("Ship.Name").unwrap(), serde_json::json!("Sofia"));
assert_eq!(ctx.db_read("Ship.keel_area").unwrap(), serde_json::Value::Null);
```

## Отличия от UI-слоя

| | `context-ui-macros` | `context-db-macros` |
| --- | --- | --- |
| Формат | UI-JSON через `UiConverter` | domain-JSON через `serde` |
| Флаг исключения | `ui_skip` | `skip_load` |
| `Option<Ctx>` = `None` при чтении | `JsonVal::Null` | `JsonVal::Null` |
| `Option<Ctx>` = `None` при записи | ошибка | создаётся `Ctx::default()` |
| Имена полей | PascalCase и snake_case | PascalCase и snake_case |

`ui_skip` и `skip_load` независимы: поле может быть скрыто в UI, но храниться
в БД, и наоборот.
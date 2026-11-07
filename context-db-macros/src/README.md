# Макрос для автоматизации записи и чтения стандартных  (database) JSON значений в контекст (`RawContext`)


---

## Поведение сгенерированного кода

| Ситуация | `db_read` | `db_write` |
| --- | --- | --- |
| `iec_id` без точки | `Unknown` | `Unknown` |
| Префикс не найден | `Unknown` | `Unknown` |
| Поле не найдено в элементе | `Unknown` | `Unknown` |
| Дублируется IEC-префикс | `Duplicated` | Значение пишется, затем `Duplicated` |
| `Option<Ctx>` = `None` | `JsonVal::Null` для известного поля | Создаётся `Ctx::default()`, затем запись |
| `Option<T>` внутри элемента | `to_value` даёт `Null` для `None` | `Deserialize` даёт `None` для `null` |
| Ошибка `serde` | `Err` | `Err`, поле не меняется |

---

## Минимальный пример использования

```rust
use context_db_macros::{DbContextAccess, DbContextProperties};
use context_macros::ContextProperties;
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
assert_eq!(ctx.db_read("Ship.Name").unwrap(), serde_json::json!("Sofia"));
assert_eq!(ctx.db_read("Ship.keel_area").unwrap(), serde_json::Value::Null);
```

---

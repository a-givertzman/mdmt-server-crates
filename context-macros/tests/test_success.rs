use context_macros::ContextLoad;
use serde::Deserialize;
use std::collections::HashMap;

// Мокаем трейт
pub trait IecId {
    fn iec_id() -> &'static str;
}

// Рабочие типы
#[derive(Debug, Deserialize, PartialEq, Default, Clone)]
pub struct Displacement {
    pub val: f64,
}
impl IecId for Displacement {
    fn iec_id() -> &'static str { "Ship.Displacement" }
}

#[derive(Debug, Deserialize, PartialEq, Default, Clone)]
pub struct Draft {
    pub val: f64,
}
impl IecId for Draft {
    fn iec_id() -> &'static str { "Ship.Draft" }
}

// "Системный" тип. Нет ни IecId, ни поддержки Serde
#[derive(Debug, PartialEq, Default, Clone)]
pub struct SystemCache {
    pub ready: bool,
}

#[derive(ContextLoad, Default, Debug, PartialEq)]
pub struct TestContext {
    pub displacement: Option<Displacement>,
    pub draft: Draft,
    #[context(skip, skip_load)]
    pub cache: SystemCache,
    #[context(skip_load)]
    pub version: usize,
}

#[test]
fn test_context_load_success() {
    // 1. Предварительная инициализация (Builder/DI этап)
    let mut ctx = TestContext::default();
    ctx.version = 42;
    ctx.cache.ready = true;
    ctx.draft.val = 1.0; // Исходное значение, которое не должно затереться

    // 2. Готовим "снэпшот" из БД
    let mut props = HashMap::new();
    // Передаем правильные данные
    props.insert("Ship.Displacement".to_string(), serde_json::json!({ "val": 15000.0 }));
    // Передаем сломанные данные (проверка перехвата ошибок парсинга)
    props.insert("Ship.Draft".to_string(), serde_json::json!("not_a_number"));
    // Передаем мусорный ключ
    props.insert("Unknown.Key".to_string(), serde_json::json!(true)); 

    // 3. Накатываем
    let (enriched_ctx, report) = ctx.with_snapshot(props);

    // 4. Проверки обогащения
    assert_eq!(enriched_ctx.displacement.unwrap().val, 15000.0);
    // Draft не затерся из-за ошибки парсинга
    assert_eq!(enriched_ctx.draft.val, 1.0, "Draft не должен был измениться при ошибке JSON");
    // Скипнутые поля сохранили свое состояние
    assert_eq!(enriched_ctx.version, 42, "Skip-поле version не должно было измениться");
    assert_eq!(enriched_ctx.cache.ready, true, "Skip-поле cache не должно было измениться");

    // 5. Проверки отчета
    assert!(report.loaded.contains(&"Ship.Displacement".to_string()));
    assert!(report.unused_in_db.contains(&"Unknown.Key".to_string()));
    assert_eq!(report.errors.len(), 1);
    assert_eq!(report.errors[0].0, "Ship.Draft");
}

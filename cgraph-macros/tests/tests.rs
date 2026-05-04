use cgraph_macros::eval_depend;

// Мокаем доменные интерфейсы для теста
pub mod domain {
    pub struct CalculationTags {
        pub inputs: Vec<String>,
        pub outputs: Vec<String>,
    }
    pub trait EvalTags {
        fn tags(&self) -> CalculationTags;
    }
    pub trait IecId {
        fn iec_id() -> &'static str;
    }
}

// Фейковые контексты
struct InitialCtx;
impl domain::IecId for InitialCtx {
    fn iec_id() -> &'static str { "IEC_INITIAL_CTX" }
}

struct ResultCtx;
impl domain::IecId for ResultCtx {
    fn iec_id() -> &'static str { "IEC_RESULT_CTX" }
}

// Фейковый транзакционный контекст
struct ContextTransaction;
impl ContextTransaction {
    fn read_ref<T>(&self) -> T { unimplemented!() }
    fn write<T>(&mut self, _val: T) { unimplemented!() }
}

struct MyCalcStep;

#[eval_depend]
impl MyCalcStep {
    fn eval(&self, ctx: &mut ContextTransaction) {
        // Проверяем разные виды типизированного доступа
        let _in1: &InitialCtx = ctx.read_ref();
        ctx.write::<ResultCtx>(ResultCtx);
    }
}

#[test]
fn test_successful_tags_generation() {
    let step = MyCalcStep;
    // Если макрос отработал верно, у нас появился метод tags()
    let tags = step.tags();
    assert_eq!(tags.inputs, vec!["IEC_INITIAL_CTX"]);
    assert_eq!(tags.outputs, vec!["IEC_RESULT_CTX"]);
}

use context_macros::ContextLoad;

pub trait IecId {
    fn iec_id() -> &'static str;
}

struct BadType; // Забыли реализовать IecId и Deserialize

#[derive(ContextLoad)]
pub struct BadContext {
    // Забыли повесить #[context(skip_load)]
    pub bad_field: BadType, 
}

fn main() {}

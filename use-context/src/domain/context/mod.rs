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
    fn properties(&self) -> Result<Vec<(String, String)>, serde_json::Error>;
}



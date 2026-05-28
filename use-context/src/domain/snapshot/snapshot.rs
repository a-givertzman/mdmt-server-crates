use std::sync::Arc;

use bincode::Encode;
use sal_core::error::Error;
use sal_sync::sync::channel::Sender;

use crate::domain::Properties;

///
/// Transactional [Snapshot]
/// - Accumulating calculation results.
/// - Atomic database inserts
/// - UI synchronization.
pub struct Snapshot {
    link: Sender<Event>,
    api_client: Arc<ApiClient>,
    items: Vec<(String, String)>,
}
impl Snapshot {
    ///
    /// Creates a new [Snapshot] instance.
    pub fn new(link: Sender<Event>, api_client: Arc<ApiClient>) -> Self {
        Self {
            link,
            api_client,
            items: Vec::new(),
        }
    }
    ///
    /// Reads specified properties from the database
    pub fn fetch(&self, keys: Vec<&str>) -> Result<(), Error> {
        if keys.is_empty() {
            self.api_client.request(Sql(format!("select all")))
        } else {
            self.api_client.request(Sql(format!("select where key in {:?}", keys)))
        }
    }
    ///
    /// Adds a [Context] member to the transaction
    pub fn add(&mut self, items: impl Properties) {
        for item in items.properties() {
            self.items.push(item);
        }
    }
    ///
    /// Sendins the current Snapshot items to the UI
    /// - Useful for user confirmation in case of consistency conflicts
    pub fn send(&self) {
        // Логика отправки событий на UI
        let result = self.link.send(
            Event::from(&self.items),
        );
        if let Err(err) = result {
            log::error!("Snapshot.semd | Error: {:?}", err);
        }
    }
    ///
    /// Completes the transaction
    /// - Applies all members to the database
    /// - Prevents double commits.
    pub fn commit(self) -> Result<(), Error> {
        let upsert = Upsert::new();
        for item in self.items {
            upsert.insert(item)
        }
        self.api_client.request(upsert.build())
            .map_err(|err| Error::new("Snapshot", "commit").pass(err))
    }
    ///
    /// Cancels the transaction
    /// - Discards all accumulated items
    pub fn rollback(self) {
        drop(self);
    }
}

//
// Temprorary structures

/// ### Fake ! To be removed...
/// Replace it with the real `Event`
#[derive(Debug, Encode)]
pub struct Event {}
impl Event {
    pub fn reply_ok(&self, ) -> Self {
        Event {}
    }
    pub fn reply_err(&self, err: impl Into<String>) -> Self {
        Event {}
    }
}
impl From<&Vec<(String, String)>> for Event {
    fn from(value: &Vec<(String, String)>) -> Self {
        Self {  }
    }
}
/// To be removed
struct Sql(pub String);
/// ### Fake ! To be removed...
/// Replace it with the real `ApiClient`
pub struct ApiClient {}
impl ApiClient {
    pub fn request(&self, sql: Sql) -> Result<(), Error> {
        Ok(())
    }
}
/// To be removed
struct Upsert {}
impl Upsert {
    pub fn new() -> Self {
        Self {}
    }
    pub fn insert(&self, item: (String, String)) {
        
    }
    pub fn build(&self) -> Sql {
        Sql("".into())
    }
}
///
/// Basic tests
#[cfg(test)]
mod snapshot_tests {
    use context_macros::ContextProperties;
    use sal_sync::sync::channel;
    use serde::Serialize;
    use super::*;

    #[derive(Debug, Serialize, ContextProperties)]
    #[iec_id = "Mock.Property"]
    struct MockProperty {
        val: i32,
    }
    // impl Properties for MockProperty {
    //     fn properties(&self) -> Vec<(&'static str, String)> {
    //         vec![("Test.Id", format!("{{\"val\":{}}}", self.val))]
    //     }
    // }
    #[test]
    fn test_snapshot_add_accumulates_data() {
        let (send, _) = channel::unbounded();
        let client = Arc::new(ApiClient {});
        let mut snapshot = Snapshot::new(send, client); // Предполагаем, что есть метод new()
        let mock_data = MockProperty { val: 42 };
        snapshot.add(&mock_data);
        assert_eq!(snapshot.items.len(), 1);
        assert_eq!(snapshot.items[0].0, "Mock.Property");
        assert_eq!(snapshot.items[0].1, "{\"val\":42}");
    }
    #[test]
    fn test_snapshot_multiple_adds() {
        let (send, _) = channel::unbounded();
        let client = Arc::new(ApiClient {});
        let mut snapshot = Snapshot::new(send, client); // Предполагаем, что есть метод new()
        snapshot.add(&MockProperty { val: 1 });
        snapshot.add(&MockProperty { val: 2 });
        assert_eq!(snapshot.items.len(), 2);
    }
    // Тест для commit и rollback лучше писать с использованием моков API-клиента.
    // Вызов commit(self) заберет владение, что само по себе гарантирует
    // невозможность повторного использования на уровне компиляции.
}
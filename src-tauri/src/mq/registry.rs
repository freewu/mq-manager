use std::collections::BTreeMap;
use std::sync::Arc;

use crate::mq::provider::MqProvider;
use crate::mq::types::ProviderDescriptor;

/// Holds every compiled-in driver.
///
/// Registration order is the order shown in the “New connection” wizard.
#[derive(Default)]
pub struct ProviderRegistry {
    providers: Vec<Arc<dyn MqProvider>>,
    index: BTreeMap<String, usize>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, provider: Arc<dyn MqProvider>) {
        let id = provider.descriptor().id;
        if self.index.contains_key(&id) {
            tracing::warn!(provider = %id, "duplicate provider registration ignored");
            return;
        }
        self.index.insert(id, self.providers.len());
        self.providers.push(provider);
    }

    pub fn get(&self, id: &str) -> Option<Arc<dyn MqProvider>> {
        self.index
            .get(id)
            .and_then(|i| self.providers.get(*i))
            .cloned()
    }

    pub fn descriptors(&self) -> Vec<ProviderDescriptor> {
        self.providers.iter().map(|p| p.descriptor()).collect()
    }
}

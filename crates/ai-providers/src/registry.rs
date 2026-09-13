//! Runtime registry of provider instances.

use std::collections::HashMap;
use std::sync::Arc;

use assistant_core::AiProvider;

/// Holds the active providers by name and the current default.
#[derive(Default)]
pub struct ProviderRegistry {
    providers: HashMap<String, Arc<dyn AiProvider>>,
    default: Option<String>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, provider: Arc<dyn AiProvider>) {
        let name = provider.name().to_string();
        if self.default.is_none() {
            self.default = Some(name.clone());
        }
        self.providers.insert(name, provider);
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn AiProvider>> {
        self.providers.get(name).cloned()
    }

    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.providers.keys().cloned().collect();
        names.sort();
        names
    }

    pub fn len(&self) -> usize {
        self.providers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }

    pub fn default_name(&self) -> Option<&str> {
        self.default.as_deref()
    }

    pub fn set_default(&mut self, name: &str) -> bool {
        if self.providers.contains_key(name) {
            self.default = Some(name.to_string());
            true
        } else {
            false
        }
    }

    pub fn default_provider(&self) -> Option<Arc<dyn AiProvider>> {
        self.default.as_deref().and_then(|n| self.get(n))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assistant_core::MockProvider;

    #[test]
    fn registry_tracks_default() {
        let mut r = ProviderRegistry::new();
        r.register(Arc::new(MockProvider::new()));
        assert_eq!(r.default_name(), Some("mock"));
        assert!(r.set_default("mock"));
        assert!(!r.set_default("nope"));
        assert_eq!(r.names(), vec!["mock".to_string()]);
    }
}

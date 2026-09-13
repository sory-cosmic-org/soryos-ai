//! Registry mapping tool names to implementations.

use assistant_core::{Tool, ToolSchema};
use std::collections::HashMap;
use std::sync::Arc;

/// Owns the available tools and exposes their schemas to providers.
#[derive(Default, Clone)]
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl std::fmt::Debug for ToolRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolRegistry")
            .field("tools", &self.names())
            .finish()
    }
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.tools.keys().cloned().collect();
        names.sort();
        names
    }

    pub fn len(&self) -> usize {
        self.tools.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    pub fn schemas(&self) -> Vec<ToolSchema> {
        self.tools
            .values()
            .map(|t| ToolSchema {
                name: t.name().to_string(),
                description: t.description().to_string(),
                parameters: t.parameters_schema(),
            })
            .collect()
    }

    pub fn tools(&self) -> Vec<Arc<dyn Tool>> {
        self.tools.values().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SystemInfoTool;

    #[test]
    fn registry_registers_and_lists() {
        let mut r = ToolRegistry::new();
        assert!(r.is_empty());
        r.register(Arc::new(SystemInfoTool));
        assert_eq!(r.len(), 1);
        assert_eq!(r.names(), vec!["system_info".to_string()]);
        assert_eq!(r.schemas().len(), 1);
    }
}

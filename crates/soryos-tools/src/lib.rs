//! Extensible tool system for SoryOS AI Assistant.
//!
//! New capabilities are added by implementing [`assistant_core::Tool`] and
//! registering the instance in [`ToolRegistry`]:
//!
//! ```ignore
//! registry.register(Arc::new(MyTool::new()));
//! ```

pub mod filesystem;
pub mod registry;
pub mod shell;
pub mod system;

pub use filesystem::{ListDirTool, ReadFileTool, WriteFileTool};
pub use registry::ToolRegistry;
pub use shell::ShellTool;
pub use system::SystemInfoTool;

use std::sync::Arc;

/// Build the default toolset wired by the desktop app.
pub fn default_tools() -> Vec<Arc<dyn assistant_core::Tool>> {
    vec![
        Arc::new(ReadFileTool),
        Arc::new(WriteFileTool),
        Arc::new(ListDirTool),
        Arc::new(ShellTool::default()),
        Arc::new(SystemInfoTool),
    ]
}

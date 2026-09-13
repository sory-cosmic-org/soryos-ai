//! Security layer: permissions, policy gate and user confirmation.
//!
//! Every tool call flows through [`PolicyGate`] (an
//! [`assistant_core::ToolGate`]) before execution:
//!
//! ```text
//! AI -> Tool Request -> Security Policy -> Permission Check
//!   -> User Confirmation (if needed) -> Tool Execution
//! ```

pub mod confirmation;
pub mod model_policy;
pub mod permissions;
pub mod policy;

pub use confirmation::{AlwaysApprove, AlwaysDeny, ConfirmerFn, TerminalConfirmer};
pub use model_policy::{check_free_only, parse_price, ModelPricingPolicy, PolicyViolation};
pub use permissions::{ActionKind, PermissionLevel};
pub use policy::SecurityPolicy;

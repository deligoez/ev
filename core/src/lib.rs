//! Data model, rules and storage for `ev`, the agent-first home inventory.

mod error;
mod fold;
mod model;
mod photo;
mod plan;
mod store;

pub use error::{Error, Result};
pub use fold::fold;
pub use model::{Disposition, Kind, NewNode, Node, NodeRef, PathSegment, State};
pub use photo::{Crop, open_upright};
pub use store::{Inventory, SCHEMA_VERSION};

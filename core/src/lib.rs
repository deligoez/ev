//! Data model, rules and storage for `ev`, the agent-first home inventory.

mod error;
mod fold;
mod grid;
mod kits;
mod map;
mod marks;
mod model;
mod photo;
mod placement;
mod plan;
mod sh3d;
mod store;

pub use error::{Error, Result};
pub use fold::fold;
pub use grid::{Cells, GridCorners};
pub use map::reading_order;
pub use model::{Disposition, Kind, NewNode, Node, NodeRef, PathSegment, State};
pub use photo::{Crop, open_upright};
pub use store::{Inventory, SCHEMA_VERSION};

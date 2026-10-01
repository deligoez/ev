//! Data model, rules and storage for `ev`, the agent-first home inventory.

mod docs;
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
mod purchases;
mod store;

pub use docs::{DOC_KINDS, NewDoc};
pub use error::{Error, Result};
pub use fold::fold;
pub use grid::{Cells, GridCorners};
pub use map::{SketchChange, parse_pair, parse_points, reading_order};
pub use model::{Disposition, Kind, NewNode, Node, NodeRef, PathSegment, State};
pub use photo::{Crop, open_upright};
pub use purchases::{BUCKETS, DISMISSALS};
pub use store::{Inventory, SCHEMA_VERSION};

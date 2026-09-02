pub mod adjacency;
pub mod aggregate;
pub mod anchor;
pub mod connectivity;
pub mod events;
pub mod manager;

pub use adjacency::{is_face_adjacent, ADJACENCY_OFFSETS_6};
pub use aggregate::{AggregateVoxel, DetachedAggregate};
pub use anchor::AnchorPolicy;
pub use connectivity::{check_structural_connectivity, ConnectivityConfig, ConnectivityStatus};
pub use events::{StructuralEvent, StructuralMutationType};
pub use manager::StructuralSystem;

pub mod gathering;
pub mod mutation;
pub mod placement;
pub mod raycast;
pub mod types;

pub use gathering::{
    calculate_yield, can_gather, execute_gather_transaction, handle_player_gather,
    resolve_resource, validate_gather_action, ResourceGatheringRegistry,
};
pub use mutation::{
    can_place, can_remove, execute_interaction_transaction, handle_player_interaction,
    validate_interaction_action,
};
pub use placement::{
    build_placement_proposal, can_place_voxel, execute_placement_transaction,
    handle_player_placement, validate_placement_proposal, validate_support, BuildRuleDefinition,
    BuildRuleRegistry,
};
pub use raycast::{
    raycast_player_interaction, raycast_player_interaction_with_reach, raycast_voxels,
};
pub use types::{
    BlockOrientation, CollectionResult, GatheringError, GatheringResult, InteractionAction,
    InteractionCooldown, InteractionMutationError, PlacementError, PlacementProposal,
    PlacementRejectionReason, PlacementResult, PlacementValidity, ResourceDefinition, VoxelHit,
    VoxelMutationResult, VoxelRaycastResult, DEFAULT_INTERACTION_COOLDOWN,
    DEFAULT_INTERACTION_REACH,
};

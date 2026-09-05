pub mod gathering;
pub mod interactables;
pub mod mutation;
pub mod placement;
pub mod raycast;
pub mod tools;
pub mod types;

pub use gathering::{
    calculate_yield, can_gather, execute_gather_transaction, handle_player_gather,
    resolve_resource, validate_gather_action, ResourceGatheringRegistry,
};
pub use interactables::{
    detect_interactable_target, evaluate_transition, execute_interaction, filter_available_actions,
    handle_player_generic_interaction, query_interactable_target, validate_interaction,
    InteractableInstance, InteractableRegistry,
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
pub use tools::{
    calculate_tool_effectiveness, can_gather_with_tool, execute_tool_gather_transaction,
    handle_player_tool_gather, resolve_tool, validate_tool_gather_action,
    validate_tool_gather_internal, validate_tool_requirement, ToolRegistry,
};
pub use types::{
    AudioCue, BlockOrientation, CollectionResult, FeedbackId, GatheringError, GatheringResult,
    InteractableAction, InteractableComponent, InteractableDefinition, InteractableId,
    InteractableState, InteractableTarget, InteractionAction, InteractionCooldown,
    InteractionError, InteractionFeedback, InteractionMutationError, InteractionProposal,
    InteractionResult, PlacementError, PlacementProposal, PlacementRejectionReason,
    PlacementResult, PlacementValidity, ResourceDefinition, ToolAction, ToolCategory,
    ToolDefinition, ToolEffectiveness, ToolError, ToolGatheringResult, ToolId, ToolRequirement,
    ToolState, VisualCue, VoxelHit, VoxelMutationResult, VoxelRaycastResult,
    DEFAULT_INTERACTION_COOLDOWN, DEFAULT_INTERACTION_REACH,
};

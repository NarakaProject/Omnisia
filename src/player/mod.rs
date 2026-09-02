pub mod collider;
pub mod collision;
pub mod config;
pub mod controller;
pub mod state;

pub use collider::Capsule;
pub use collision::{
    check_capsule_clearance, check_ground_support, resolve_swept_step, swept_axis_x, swept_axis_y,
    swept_axis_z, CollisionStepStats, GroundContactResult, SweptHit,
};
pub use config::PlayerConfig;
pub use controller::{PlayerController, PlayerInput};
pub use state::PlayerState;

pub mod camera;
pub mod clear;
pub mod env;
pub mod status;
pub mod time;

use super::command::CommandRegistry;

/// Creates a default `CommandRegistry` populated with all standard developer commands.
pub fn create_default_registry() -> CommandRegistry {
    let mut registry = CommandRegistry::new();
    registry
        .register(Box::new(camera::CameraCommand))
        .expect("register camera");
    registry
        .register(Box::new(clear::ClearCommand))
        .expect("register clear");
    registry
        .register(Box::new(env::EnvCommand))
        .expect("register env");
    registry
        .register(Box::new(status::StatusCommand))
        .expect("register status");
    registry
        .register(Box::new(time::TimeCommand))
        .expect("register time");
    registry
}

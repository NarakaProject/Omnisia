use super::super::command::{CommandResult, ConsoleCommand};
use super::super::context::DeveloperExecutionContext;

pub struct StatusCommand;

impl ConsoleCommand for StatusCommand {
    fn name(&self) -> &'static str {
        "status"
    }

    fn description(&self) -> &'static str {
        "Displays high-level engine, camera, environment, and streaming status."
    }

    fn usage(&self) -> &'static str {
        "status"
    }

    fn detailed_help(&self) -> Option<&'static str> {
        Some("Reports camera mode, active coordinates, environment clock, resident chunks, and frame performance.")
    }

    fn execute(&self, _args: &[String], ctx: &mut DeveloperExecutionContext) -> CommandResult {
        let cam_pos = ctx.camera.active_camera_position();
        let status = format!(
            "Omnisia Engine Status:\n\
            \x20 Camera Mode:     {}\n\
            \x20 Active Camera:   ({:.2}, {:.2}, {:.2})m\n\
            \x20 Player Position: ({:.2}, {:.2}, {:.2})m (Feet)\n\
            \x20 Environment:     {} ({:.4}) [{}]\n\
            \x20 Resident Chunks: {} chunks\n\
            \x20 Performance:     {:.1} FPS ({:.2} ms)",
            ctx.camera.mode().name(),
            cam_pos.x,
            cam_pos.y,
            cam_pos.z,
            ctx.camera.player_position().x,
            ctx.camera.player_position().y,
            ctx.camera.player_position().z,
            ctx.environment.clock.time_string(),
            ctx.environment.clock.day_fraction,
            if ctx.environment.clock.paused {
                "PAUSED"
            } else {
                "RUNNING"
            },
            ctx.resident_chunks,
            ctx.fps,
            ctx.frame_time_ms
        );
        CommandResult::Success(status)
    }
}

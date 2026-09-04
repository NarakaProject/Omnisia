use glam::Vec3;

use super::super::command::{CommandResult, ConsoleCommand};
use super::super::context::{CameraMode, DeveloperExecutionContext};

pub struct CameraCommand;

impl ConsoleCommand for CameraCommand {
    fn name(&self) -> &'static str {
        "camera"
    }

    fn description(&self) -> &'static str {
        "Inspects and controls developer free camera and camera modes."
    }

    fn usage(&self) -> &'static str {
        "camera <free|player|speed|position|rotation|status> [args]"
    }

    fn detailed_help(&self) -> Option<&'static str> {
        Some(
            "Subcommands:\n\
            \x20 camera status                    Display camera mode, position, orientation, and speed\n\
            \x20 camera free                      Switch to Developer Free-Flight camera\n\
            \x20 camera player                    Restore Player Gameplay camera\n\
            \x20 camera speed                     Display current developer camera speed\n\
            \x20 camera speed <val>               Set developer camera speed in m/s (0.0 < val <= 2000.0)\n\
            \x20 camera position                  Display active camera position\n\
            \x20 camera position <x> <y> <z>      Teleport developer free camera to world coordinates\n\
            \x20 camera rotation                  Display active camera yaw and pitch degrees\n\n\
            Safety Invariant (Amendment 2 & 9):\n\
            \x20 Setting camera position modifies the Developer camera only and requires 'camera free'.\n\
            \x20 The player character position and physics state remain strictly unchanged."
        )
    }

    fn execute(&self, args: &[String], ctx: &mut DeveloperExecutionContext) -> CommandResult {
        if args.is_empty() {
            return self.get_status(ctx);
        }

        let subcmd = args[0].to_lowercase();
        match subcmd.as_str() {
            "status" => self.get_status(ctx),
            "free" => {
                ctx.camera.set_mode(CameraMode::Developer);
                CommandResult::Success(format!(
                    "Switched to Developer Free Camera at ({:.2}, {:.2}, {:.2})m. Fly with WASD/Space/Shift.",
                    ctx.camera.dev_camera.position.x,
                    ctx.camera.dev_camera.position.y,
                    ctx.camera.dev_camera.position.z
                ))
            }
            "player" => {
                ctx.camera.set_mode(CameraMode::Player);
                CommandResult::Success(format!(
                    "Restored Player Gameplay Camera at eye ({:.2}, {:.2}, {:.2})m.",
                    ctx.camera.player_eye_position().x,
                    ctx.camera.player_eye_position().y,
                    ctx.camera.player_eye_position().z
                ))
            }
            "speed" => {
                if args.len() < 2 {
                    CommandResult::Success(format!(
                        "Developer camera speed: {:.2} m/s [{}]",
                        ctx.camera.dev_camera.speed,
                        ctx.camera.dev_camera.active_preset.name()
                    ))
                } else {
                    if !ctx.camera.is_developer() {
                        return CommandResult::Error(String::from(
                            "Developer camera is not active. Switch to developer camera with 'camera free' first.",
                        ));
                    }
                    let raw_val = &args[1];
                    match raw_val.parse::<f32>() {
                        Ok(speed) => {
                            if !speed.is_finite() || speed <= 0.0 {
                                CommandResult::Error(format!(
                                    "invalid argument <speed>: '{}' is not a positive finite number",
                                    raw_val
                                ))
                            } else {
                                match ctx.camera.set_dev_speed(speed) {
                                    Ok(()) => CommandResult::Success(format!(
                                        "Developer camera speed set to {:.2} m/s",
                                        speed
                                    )),
                                    Err(err) => CommandResult::Error(format!(
                                        "error setting speed: {}",
                                        err
                                    )),
                                }
                            }
                        }
                        Err(_) => CommandResult::Error(format!(
                            "invalid argument <speed>: expected floating-point number, got '{}'",
                            raw_val
                        )),
                    }
                }
            }
            "position" => {
                if args.len() < 2 {
                    let pos = ctx.camera.active_camera_position();
                    CommandResult::Success(format!(
                        "Active camera position ({}) is ({:.2}, {:.2}, {:.2})m",
                        ctx.camera.mode.name(),
                        pos.x,
                        pos.y,
                        pos.z
                    ))
                } else {
                    if !ctx.camera.is_developer() {
                        return CommandResult::Error(String::from(
                            "Developer camera is not active. Switch to developer camera with 'camera free' first.",
                        ));
                    }
                    if args.len() < 4 {
                        return CommandResult::Error(String::from(
                            "missing coordinates <x> <y> <z>\nUsage: camera position <x> <y> <z>",
                        ));
                    }
                    let x_res = args[1].parse::<f32>();
                    let y_res = args[2].parse::<f32>();
                    let z_res = args[3].parse::<f32>();

                    match (x_res, y_res, z_res) {
                        (Ok(x), Ok(y), Ok(z)) => {
                            if !x.is_finite() || !y.is_finite() || !z.is_finite() {
                                CommandResult::Error(String::from(
                                    "invalid argument: coordinates must be finite numbers",
                                ))
                            } else {
                                let new_pos = Vec3::new(x, y, z);
                                ctx.camera.set_dev_position(new_pos);
                                CommandResult::Success(format!(
                                    "Developer camera teleported to ({:.2}, {:.2}, {:.2})m (Player position untouched).",
                                    x, y, z
                                ))
                            }
                        }
                        _ => CommandResult::Error(String::from(
                            "invalid coordinates: expected 3 floating-point numbers\nUsage: camera position <x> <y> <z>",
                        )),
                    }
                }
            }
            "rotation" => {
                let (yaw, pitch) = match ctx.camera.mode {
                    CameraMode::Player => (
                        ctx.camera.dev_camera.yaw_deg,
                        ctx.camera.dev_camera.pitch_deg,
                    ),
                    CameraMode::Developer => (
                        ctx.camera.dev_camera.yaw_deg,
                        ctx.camera.dev_camera.pitch_deg,
                    ),
                };
                CommandResult::Success(format!(
                    "Camera orientation: Yaw: {:.2}°, Pitch: {:.2}°",
                    yaw, pitch
                ))
            }
            _ => CommandResult::Error(format!(
                "unknown camera subcommand \"{}\". Type \"help camera\" for usage.",
                subcmd
            )),
        }
    }
}

impl CameraCommand {
    fn get_status(&self, ctx: &DeveloperExecutionContext) -> CommandResult {
        let mode = ctx.camera.mode();
        let dev_pos = ctx.camera.dev_camera.position;
        let player_feet = ctx.camera.player_position();
        let player_eye = ctx.camera.player_eye_position();

        let status = format!(
            "Camera Diagnostics:\n\
            \x20 Active Mode:      {}\n\
            \x20 Dev Camera Pos:   ({:.2}, {:.2}, {:.2})m\n\
            \x20 Dev Camera Speed: {:.1} m/s [{}]\n\
            \x20 Dev Orientation:  Yaw: {:.1}°, Pitch: {:.1}°\n\
            \x20 Player Feet Pos:  ({:.2}, {:.2}, {:.2})m (Read-only)\n\
            \x20 Player Eye Pos:   ({:.2}, {:.2}, {:.2})m (Read-only)",
            mode.name(),
            dev_pos.x,
            dev_pos.y,
            dev_pos.z,
            ctx.camera.dev_camera.speed,
            ctx.camera.dev_camera.active_preset.name(),
            ctx.camera.dev_camera.yaw_deg,
            ctx.camera.dev_camera.pitch_deg,
            player_feet.x,
            player_feet.y,
            player_feet.z,
            player_eye.x,
            player_eye.y,
            player_eye.z
        );
        CommandResult::Success(status)
    }
}

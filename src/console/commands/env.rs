use super::super::command::{CommandResult, ConsoleCommand};
use super::super::context::DeveloperExecutionContext;

pub struct EnvCommand;

impl ConsoleCommand for EnvCommand {
    fn name(&self) -> &'static str {
        "env"
    }

    fn description(&self) -> &'static str {
        "Inspects derived atmospheric environment parameters and lunar state."
    }

    fn usage(&self) -> &'static str {
        "env <status|moon> [args]"
    }

    fn detailed_help(&self) -> Option<&'static str> {
        Some(
            "Subcommands:\n\
            \x20 env status           Display comprehensive atmospheric and lighting parameters\n\
            \x20 env moon             Inspect continuous moon phase and canonical classification\n\
            \x20 env moon set <phase> Directly set continuous moon phase in [0.0, 1.0)\n\n\
            Phase 10.5 Invariant:\n\
            \x20 Continuous moon_phase is the authoritative visual parameter;\n\
            \x20 the 8-phase enum is for classification and diagnostics.",
        )
    }

    fn execute(&self, args: &[String], ctx: &mut DeveloperExecutionContext) -> CommandResult {
        if args.is_empty() {
            return self.get_status(ctx);
        }

        let subcmd = args[0].to_lowercase();
        match subcmd.as_str() {
            "status" => self.get_status(ctx),
            "moon" => {
                if args.len() >= 3 && args[1].to_lowercase() == "set" {
                    let raw_val = &args[2];
                    let phase_opt = if let Ok(phase) = raw_val.parse::<f32>() {
                        if phase.is_finite() && (0.0..1.0).contains(&phase) {
                            Some(phase)
                        } else {
                            return CommandResult::Error(format!(
                                "invalid argument <phase>: expected number in range [0, 1), got {}",
                                phase
                            ));
                        }
                    } else {
                        match raw_val.to_lowercase().as_str() {
                            "new" | "new_moon" => Some(0.0),
                            "waxing_crescent" => Some(0.125),
                            "first_quarter" => Some(0.25),
                            "waxing_gibbous" => Some(0.375),
                            "full" | "full_moon" => Some(0.5),
                            "waning_gibbous" => Some(0.625),
                            "last_quarter" => Some(0.75),
                            "waning_crescent" => Some(0.875),
                            _ => None,
                        }
                    };

                    match phase_opt {
                        Some(phase) => {
                            ctx.environment.clock.set_moon_phase(phase);
                            CommandResult::Success(format!(
                                "Moon phase set to {:.3} ({})",
                                phase,
                                ctx.environment.clock.named_moon_phase().name()
                            ))
                        }
                        None => CommandResult::Error(format!(
                            "invalid argument <phase>: expected floating-point number in [0, 1) or named phase (new, first_quarter, full, last_quarter), got '{}'",
                            raw_val
                        )),
                    }
                } else {
                    let clock = &ctx.environment.clock;
                    let phase = clock.moon_phase();
                    let named = clock.named_moon_phase();
                    CommandResult::Success(format!(
                        "Lunar Environment State:\n\
                        \x20 Continuous Phase: {:.4} (Range [0.0, 1.0))\n\
                        \x20 Classification:   {}\n\
                        \x20 Lunar Cycle:      {:.1} game days\n\
                        \x20 Total Elapsed:    {:.1}s",
                        phase,
                        named.name(),
                        clock.lunar_cycle_days,
                        clock.total_elapsed_secs
                    ))
                }
            }
            _ => CommandResult::Error(format!(
                "unknown env subcommand \"{}\". Type \"help env\" for usage.",
                subcmd
            )),
        }
    }
}

impl EnvCommand {
    fn get_status(&self, ctx: &DeveloperExecutionContext) -> CommandResult {
        let clock = &ctx.environment.clock;
        let celestial = &ctx.environment.celestial;

        let status = format!(
            "Atmospheric Environment Overview:\n\
            \x20 Day Fraction:     {:.4} ({})\n\
            \x20 Progression:      {} (Scale: {:.2}x)\n\
            \x20 Sun Elevation:    {:.4}\n\
            \x20 Sun Direction:    ({:.3}, {:.3}, {:.3})\n\
            \x20 Moon Direction:   ({:.3}, {:.3}, {:.3})\n\
            \x20 Moon Phase:       {:.3} ({})\n\
            \x20 Twilight Factor:  {:.4}\n\
            \x20 Daylight Factor:  {:.4}\n\
            \x20 Star Visibility:  {:.4}\n\
            \x20 Horizon Color:    RGB({:.2}, {:.2}, {:.2})\n\
            \x20 Zenith Color:     RGB({:.2}, {:.2}, {:.2})\n\
            \x20 Sun Color:        RGB({:.2}, {:.2}, {:.2})\n\
            \x20 Ambient Color:    RGB({:.2}, {:.2}, {:.2})",
            clock.day_fraction,
            clock.time_string(),
            if clock.paused { "PAUSED" } else { "ACTIVE" },
            clock.time_scale,
            celestial.sun_elevation,
            celestial.sun_direction.x,
            celestial.sun_direction.y,
            celestial.sun_direction.z,
            celestial.moon_direction.x,
            celestial.moon_direction.y,
            celestial.moon_direction.z,
            clock.moon_phase(),
            clock.named_moon_phase().name(),
            celestial.twilight_factor,
            celestial.day_factor,
            celestial.star_visibility,
            celestial.horizon_color[0],
            celestial.horizon_color[1],
            celestial.horizon_color[2],
            celestial.zenith_color[0],
            celestial.zenith_color[1],
            celestial.zenith_color[2],
            celestial.celestial_light_color[0],
            celestial.celestial_light_color[1],
            celestial.celestial_light_color[2],
            celestial.ambient_light_color[0],
            celestial.ambient_light_color[1],
            celestial.ambient_light_color[2]
        );
        CommandResult::Success(status)
    }
}

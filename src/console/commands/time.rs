use super::super::command::{CommandResult, ConsoleCommand};
use super::super::context::DeveloperExecutionContext;

pub struct TimeCommand;

impl ConsoleCommand for TimeCommand {
    fn name(&self) -> &'static str {
        "time"
    }

    fn description(&self) -> &'static str {
        "Controls environment time progression, scale, pause state, and canonical anchors."
    }

    fn usage(&self) -> &'static str {
        "time <get|pause|resume|scale|set> [args]"
    }

    fn detailed_help(&self) -> Option<&'static str> {
        Some(
            "Subcommands:\n\
            \x20 time get                     Show current time, day fraction, scale, and celestial state\n\
            \x20 time pause                   Freeze environment time advancement\n\
            \x20 time resume                  Resume environment time advancement\n\
            \x20 time scale                   Display current time scale multiplier\n\
            \x20 time scale <multiplier>      Set progression scale (0.0 < value <= 1000.0)\n\
            \x20 time set <day_fraction>      Set celestial time directly in [0.0, 1.0)\n\n\
            Canonical Anchors:\n\
            \x20 0.00 = Midnight (Sun at nadir -Y)\n\
            \x20 0.25 = Sunrise  (Sun crossing horizon +X)\n\
            \x20 0.50 = Noon     (Sun at zenith +Y)\n\
            \x20 0.75 = Sunset   (Sun crossing horizon -X)\n\n\
            Examples:\n\
            \x20 time set 0.5\n\
            \x20 time pause\n\
            \x20 time scale 10"
        )
    }

    fn execute(&self, args: &[String], ctx: &mut DeveloperExecutionContext) -> CommandResult {
        if args.is_empty() {
            return self.get_time(ctx);
        }

        let subcmd = args[0].to_lowercase();
        match subcmd.as_str() {
            "get" => self.get_time(ctx),
            "pause" => {
                ctx.environment.pause();
                CommandResult::Success(format!(
                    "Environment time paused at day fraction {:.4} ({})",
                    ctx.environment.clock.day_fraction,
                    ctx.environment.clock.time_string()
                ))
            }
            "resume" => {
                ctx.environment.resume();
                CommandResult::Success(format!(
                    "Environment time resumed at scale {:.2}x",
                    ctx.environment.clock.time_scale
                ))
            }
            "scale" => {
                if args.len() < 2 {
                    CommandResult::Success(format!(
                        "Current environment time scale: {:.2}x",
                        ctx.environment.clock.time_scale
                    ))
                } else {
                    let raw_val = &args[1];
                    match raw_val.parse::<f32>() {
                        Ok(scale) => {
                            if !scale.is_finite() || scale <= 0.0 {
                                CommandResult::Error(format!(
                                    "invalid argument <scale>: '{}' is not a positive finite number",
                                    raw_val
                                ))
                            } else if scale > 1000.0 {
                                CommandResult::Error(format!(
                                    "invalid argument <scale>: {} exceeds developer maximum bound of 1000.0x",
                                    scale
                                ))
                            } else {
                                match ctx.environment.set_time_scale(scale) {
                                    Ok(()) => CommandResult::Success(format!(
                                        "Environment time scale set to {:.2}x",
                                        scale
                                    )),
                                    Err(err) => CommandResult::Error(format!(
                                        "error setting time scale: {}",
                                        err
                                    )),
                                }
                            }
                        }
                        Err(_) => CommandResult::Error(format!(
                            "invalid argument <scale>: expected floating-point number, got '{}'",
                            raw_val
                        )),
                    }
                }
            }
            "set" => {
                if args.len() < 2 {
                    CommandResult::Error(String::from(
                        "missing argument <day_fraction>\nUsage: time set <day_fraction>",
                    ))
                } else {
                    let raw_val = &args[1];
                    match raw_val.parse::<f32>() {
                        Ok(fraction) => {
                            if !fraction.is_finite() || fraction < 0.0 || fraction >= 1.0 {
                                CommandResult::Error(format!(
                                    "invalid argument <day_fraction>: expected number in range [0, 1), got {}",
                                    fraction
                                ))
                            } else {
                                ctx.environment.set_day_fraction(fraction);
                                CommandResult::Success(format!(
                                    "Environment time set to day fraction {:.4} ({})",
                                    fraction,
                                    ctx.environment.clock.time_string()
                                ))
                            }
                        }
                        Err(_) => CommandResult::Error(format!(
                            "invalid argument <day_fraction>: expected number in range [0, 1), got '{}'",
                            raw_val
                        )),
                    }
                }
            }
            _ => CommandResult::Error(format!(
                "unknown time subcommand \"{}\". Type \"help time\" for usage.",
                subcmd
            )),
        }
    }
}

impl TimeCommand {
    fn get_time(&self, ctx: &DeveloperExecutionContext) -> CommandResult {
        let clock = &ctx.environment.clock;
        let celestial = &ctx.environment.celestial;
        let phase_name = match clock.day_fraction {
            f if (0.22..=0.28).contains(&f) => "Sunrise",
            f if (0.45..=0.55).contains(&f) => "Solar Noon",
            f if (0.72..=0.78).contains(&f) => "Sunset",
            f if f >= 0.95 || f <= 0.05 => "Midnight",
            f if f < 0.22 || f > 0.78 => "Night",
            _ => "Day",
        };

        let status = format!(
            "Environment Time Status:\n\
            \x20 Time:           {} (Fraction: {:.4})\n\
            \x20 Approximate:    {}\n\
            \x20 State:          {}\n\
            \x20 Scale:          {:.2}x (Full day: {:.0}s / {:.1}m)\n\
            \x20 Sun Elevation:  {:.3}\n\
            \x20 Sun Direction:  ({:.3}, {:.3}, {:.3})\n\
            \x20 Moon Phase:     {:.3} ({})\n\
            \x20 Twilight Factor:{:.3}\n\
            \x20 Star Visibility:{:.3}",
            clock.time_string(),
            clock.day_fraction,
            phase_name,
            if clock.paused { "PAUSED" } else { "RUNNING" },
            clock.time_scale,
            clock.day_length_secs,
            clock.day_length_secs / 60.0,
            celestial.sun_elevation,
            celestial.sun_direction.x,
            celestial.sun_direction.y,
            celestial.sun_direction.z,
            clock.moon_phase(),
            clock.named_moon_phase().name(),
            celestial.twilight_factor,
            celestial.star_visibility
        );
        CommandResult::Success(status)
    }
}

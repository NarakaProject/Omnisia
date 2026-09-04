/// Canonical classification of the lunar cycle into eight named phases.
///
/// INVARIANT: This enum is for classification, diagnostics, and UI display only.
/// The authoritative celestial and visual model strictly consumes the continuous
/// `moon_phase: f32 ∈ [0.0, 1.0)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MoonPhase {
    NewMoon,
    WaxingCrescent,
    FirstQuarter,
    WaxingGibbous,
    FullMoon,
    WaningGibbous,
    LastQuarter,
    WaningCrescent,
}

impl MoonPhase {
    /// Classifies a continuous moon phase in `[0.0, 1.0)` into one of the 8 canonical phases.
    ///
    /// Each primary phase (New, First Quarter, Full, Last Quarter) is centered on its canonical
    /// fraction (0.0, 0.25, 0.5, 0.75) with a window of ±0.0625 (1/16 of a cycle).
    pub fn from_phase(phase: f32) -> Self {
        let p = phase.rem_euclid(1.0);
        if p >= 0.9375 || p < 0.0625 {
            MoonPhase::NewMoon
        } else if p < 0.1875 {
            MoonPhase::WaxingCrescent
        } else if p < 0.3125 {
            MoonPhase::FirstQuarter
        } else if p < 0.4375 {
            MoonPhase::WaxingGibbous
        } else if p < 0.5625 {
            MoonPhase::FullMoon
        } else if p < 0.6875 {
            MoonPhase::WaningGibbous
        } else if p < 0.8125 {
            MoonPhase::LastQuarter
        } else {
            MoonPhase::WaningCrescent
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            MoonPhase::NewMoon => "New Moon",
            MoonPhase::WaxingCrescent => "Waxing Crescent",
            MoonPhase::FirstQuarter => "First Quarter",
            MoonPhase::WaxingGibbous => "Waxing Gibbous",
            MoonPhase::FullMoon => "Full Moon",
            MoonPhase::WaningGibbous => "Waning Gibbous",
            MoonPhase::LastQuarter => "Last Quarter",
            MoonPhase::WaningCrescent => "Waning Crescent",
        }
    }
}

/// Deterministic, lightweight environment time representation.
///
/// AUTHORITY BOUNDARY:
/// `EnvironmentClock` is a derived visual environment model. It does NOT possess
/// authority over simulation chunks, physics, or gameplay logic.
///
/// TIME CONVENTION:
/// - `day_fraction ∈ [0.0, 1.0)`
///   - `0.00`: Midnight (Sun at nadir -Y)
///   - `0.25`: Sunrise  (Sun crossing horizon +X)
///   - `0.50`: Solar Noon (Sun at zenith +Y)
///   - `0.75`: Sunset   (Sun crossing horizon -X)
#[derive(Debug, Clone, PartialEq)]
pub struct EnvironmentClock {
    /// Normalized position in the current 24-hour cycle, bounded strictly to `[0.0, 1.0)`.
    pub day_fraction: f32,
    /// Duration of one full day in simulation seconds (default: 1200.0s = 20 minutes).
    pub day_length_secs: f32,
    /// Multiplier applied to elapsed delta time (default: 1.0).
    pub time_scale: f32,
    /// Whether environment time advancement is currently frozen.
    pub paused: bool,
    /// High-precision accumulated simulation seconds for lunar cycle derivation.
    pub total_elapsed_secs: f64,
    /// Number of game days in one full lunar cycle (default: 28.0 days).
    pub lunar_cycle_days: f32,
    /// Base offset for initial moon phase in `[0.0, 1.0)`.
    pub initial_moon_phase: f32,
}

impl Default for EnvironmentClock {
    fn default() -> Self {
        Self {
            // Start at sunrise (0.25 = 06:00) by default
            day_fraction: 0.25,
            day_length_secs: 1200.0,
            time_scale: 1.0,
            paused: false,
            total_elapsed_secs: 0.0,
            lunar_cycle_days: 28.0,
            initial_moon_phase: 0.5, // Default start with Full Moon for clear initial visuals
        }
    }
}

impl EnvironmentClock {
    /// Creates an environment clock with a specific initial day fraction and day length.
    pub fn new(initial_day_fraction: f32, day_length_secs: f32) -> Self {
        Self {
            day_fraction: initial_day_fraction.rem_euclid(1.0),
            day_length_secs: day_length_secs.max(1.0),
            time_scale: 1.0,
            paused: false,
            total_elapsed_secs: 0.0,
            lunar_cycle_days: 28.0,
            initial_moon_phase: 0.5,
        }
    }

    /// Advances the environment clock by an explicit simulation delta in seconds.
    ///
    /// GUARANTEES:
    /// - Deterministic: given the same state and `dt_secs`, produces identical results.
    /// - Independent of OS / wall-clock time (`SystemTime`).
    /// - Strictly keeps `day_fraction` inside `[0.0, 1.0)`.
    /// - Respects `self.paused`: if paused, time does NOT advance.
    pub fn advance(&mut self, dt_secs: f32) {
        if self.paused || !dt_secs.is_finite() || dt_secs <= 0.0 {
            return;
        }

        let scaled_dt = dt_secs * self.time_scale;
        let day_delta = scaled_dt / self.day_length_secs;
        self.day_fraction = (self.day_fraction + day_delta).rem_euclid(1.0);
        self.total_elapsed_secs += scaled_dt as f64;
    }

    /// Freezes environment progression.
    #[inline]
    pub fn pause(&mut self) {
        self.paused = true;
    }

    /// Resumes environment progression.
    #[inline]
    pub fn resume(&mut self) {
        self.paused = false;
    }

    /// Returns whether the environment clock is currently paused.
    #[inline]
    pub fn is_paused(&self) -> bool {
        self.paused
    }

    /// Sets the time progression scale within the bounded developer range `(0.0, 1000.0]`.
    pub fn set_time_scale(&mut self, scale: f32) -> Result<(), &'static str> {
        if !scale.is_finite() || scale <= 0.0 {
            return Err("expected positive finite number");
        }
        if scale > 1000.0 {
            return Err("time scale exceeds maximum developer bound of 1000.0");
        }
        self.time_scale = scale;
        Ok(())
    }

    /// Sets the normalized day fraction directly, wrapping into `[0.0, 1.0)`.
    pub fn set_day_fraction(&mut self, fraction: f32) {
        if fraction.is_finite() {
            self.day_fraction = fraction.rem_euclid(1.0);
        }
    }

    /// Returns the current time of day in decimal hours `[0.0, 24.0)`.
    #[inline]
    pub fn time_of_day_hours(&self) -> f32 {
        self.day_fraction * 24.0
    }

    /// Returns a human-readable 24-hour time string "HH:MM".
    pub fn time_string(&self) -> String {
        let total_minutes = (self.day_fraction * 24.0 * 60.0).round() as u32;
        let hours = (total_minutes / 60) % 24;
        let minutes = total_minutes % 60;
        format!("{:02}:{:02}", hours, minutes)
    }

    /// Returns the continuous moon phase in `[0.0, 1.0)`.
    ///
    /// The phase progresses continuously as simulation days elapse:
    /// `0.0` = New Moon, `0.25` = First Quarter, `0.5` = Full Moon, `0.75` = Last Quarter.
    pub fn moon_phase(&self) -> f32 {
        let elapsed_days = self.total_elapsed_secs / (self.day_length_secs as f64);
        let lunar_cycle = self.lunar_cycle_days as f64;
        let continuous = self.initial_moon_phase as f64 + (elapsed_days / lunar_cycle);
        continuous.rem_euclid(1.0) as f32
    }

    /// Sets the continuous moon phase directly in `[0.0, 1.0)`.
    pub fn set_moon_phase(&mut self, phase: f32) {
        if phase.is_finite() {
            self.initial_moon_phase = phase.rem_euclid(1.0);
            self.total_elapsed_secs = 0.0;
        }
    }

    /// Classifies the continuous moon phase into one of the 8 canonical named phases.
    #[inline]
    pub fn named_moon_phase(&self) -> MoonPhase {
        MoonPhase::from_phase(self.moon_phase())
    }

    /// Returns a bounded star animation phase in `[0.0, 60.0)` to avoid float precision loss.
    #[inline]
    pub fn bounded_star_time(&self) -> f32 {
        (self.total_elapsed_secs % 60.0) as f32
    }
}

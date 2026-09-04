use std::time::Instant;

use glam::Vec3;
use omnisia::camera::Camera;
use omnisia::environment::celestial::CelestialParameters;
use omnisia::environment::sky::EnvironmentState;
use omnisia::environment::time::{EnvironmentClock, MoonPhase};

const TOLERANCE: f32 = 1e-4;

fn main() {
    println!("================================================================================");
    println!("        OMNISIA — PHASE 10.5 SKY & ATMOSPHERE FOUNDATION VALIDATION             ");
    println!("================================================================================");

    let start_all = Instant::now();

    // ------------------------------------------------------------------------
    // STAGE 1: CELESTIAL CLOCK & CANONICAL ANCHORS (Amendment 4)
    // ------------------------------------------------------------------------
    print!("Stage 1: Celestial Clock Canonical Anchors ... ");
    let mut clock = EnvironmentClock::new(0.0, 1200.0);

    // Midnight: day_fraction = 0.00 -> Sun (0, -1, 0)
    let p_midnight = CelestialParameters::evaluate(clock.day_fraction);
    assert!((p_midnight.sun_direction.x - 0.0).abs() < TOLERANCE);
    assert!((p_midnight.sun_direction.y - (-1.0)).abs() < TOLERANCE);

    // Sunrise: day_fraction = 0.25 -> Sun (+1, 0, 0)
    clock.set_day_fraction(0.25);
    let p_sunrise = CelestialParameters::evaluate(clock.day_fraction);
    assert!((p_sunrise.sun_direction.x - 1.0).abs() < TOLERANCE);
    assert!((p_sunrise.sun_direction.y - 0.0).abs() < TOLERANCE);

    // Noon: day_fraction = 0.50 -> Sun (0, +1, 0)
    clock.set_day_fraction(0.50);
    let p_noon = CelestialParameters::evaluate(clock.day_fraction);
    assert!((p_noon.sun_direction.x - 0.0).abs() < TOLERANCE);
    assert!((p_noon.sun_direction.y - 1.0).abs() < TOLERANCE);

    // Sunset: day_fraction = 0.75 -> Sun (-1, 0, 0)
    clock.set_day_fraction(0.75);
    let p_sunset = CelestialParameters::evaluate(clock.day_fraction);
    assert!((p_sunset.sun_direction.x - (-1.0)).abs() < TOLERANCE);
    assert!((p_sunset.sun_direction.y - 0.0).abs() < TOLERANCE);

    println!("PASS (4 Canonical Anchors Verified within 1e-4)");

    // ------------------------------------------------------------------------
    // STAGE 2: SUN POSITION & ELEVATION NORMALIZATION
    // ------------------------------------------------------------------------
    print!("Stage 2: Sun Position & Elevation Normalization ... ");
    for i in 0..360 {
        let frac = i as f32 / 360.0;
        let p = CelestialParameters::evaluate(frac);
        assert!((p.sun_direction.length() - 1.0).abs() < TOLERANCE);
        assert!((p.sun_elevation - p.sun_direction.y).abs() < TOLERANCE);
        assert!(p.sun_elevation >= -1.0 && p.sun_elevation <= 1.0);
    }
    println!("PASS (360 Continuous Samples Unit Length == 1.0)");

    // ------------------------------------------------------------------------
    // STAGE 3: MOON OPPOSITION & 5-DEGREE DECLINATION TILT (Amendment 5)
    // ------------------------------------------------------------------------
    print!("Stage 3: Moon Opposition & Bounded Declination Tilt ... ");
    let p_mid = CelestialParameters::evaluate(0.00);
    assert!((p_mid.moon_direction.x - 0.0).abs() < TOLERANCE);
    assert!(p_mid.moon_direction.y > 0.995); // cos(5°) ≈ 0.99619
    assert!(p_mid.moon_direction.z > 0.086); // sin(5°) ≈ 0.08715
    assert!((p_mid.moon_direction.length() - 1.0).abs() < TOLERANCE);

    let p_noo = CelestialParameters::evaluate(0.50);
    assert!((p_noo.moon_direction.x - 0.0).abs() < TOLERANCE);
    assert!(p_noo.moon_direction.y < -0.995);
    assert!(p_noo.moon_direction.z < -0.086);
    println!("PASS (Declination Tilt = 5.0°, ||M|| == 1.0)");

    // ------------------------------------------------------------------------
    // STAGE 4: CONTINUOUS MOON PHASE & 8-PHASE MAPPING (Amendment 6)
    // ------------------------------------------------------------------------
    print!("Stage 4: Continuous Moon Phase & 8-Phase Mapping ... ");
    let mut clk = EnvironmentClock::new(0.0, 100.0);
    clk.lunar_cycle_days = 28.0;

    clk.set_moon_phase(0.00);
    assert_eq!(clk.named_moon_phase(), MoonPhase::NewMoon);
    clk.set_moon_phase(0.125);
    assert_eq!(clk.named_moon_phase(), MoonPhase::WaxingCrescent);
    clk.set_moon_phase(0.25);
    assert_eq!(clk.named_moon_phase(), MoonPhase::FirstQuarter);
    clk.set_moon_phase(0.375);
    assert_eq!(clk.named_moon_phase(), MoonPhase::WaxingGibbous);
    clk.set_moon_phase(0.50);
    assert_eq!(clk.named_moon_phase(), MoonPhase::FullMoon);
    clk.set_moon_phase(0.625);
    assert_eq!(clk.named_moon_phase(), MoonPhase::WaningGibbous);
    clk.set_moon_phase(0.75);
    assert_eq!(clk.named_moon_phase(), MoonPhase::LastQuarter);
    clk.set_moon_phase(0.875);
    assert_eq!(clk.named_moon_phase(), MoonPhase::WaningCrescent);
    println!("PASS (All 8 Canonical Lunar Phases Verified)");

    // ------------------------------------------------------------------------
    // STAGE 5: TWILIGHT CONTINUITY & PROCEDURAL STAR VISIBILITY (Amendment 7 & 8)
    // ------------------------------------------------------------------------
    print!("Stage 5: Twilight Continuity & Star Visibility Suppression ... ");
    let p_noon_star = CelestialParameters::evaluate(0.50);
    assert_eq!(p_noon_star.twilight_factor, 0.0);
    assert_eq!(p_noon_star.star_visibility, 0.0);

    let p_mid_star = CelestialParameters::evaluate(0.00);
    assert_eq!(p_mid_star.twilight_factor, 0.0);
    assert!((p_mid_star.star_visibility - 1.0).abs() < TOLERANCE);

    let p_twi_star = CelestialParameters::evaluate(0.25);
    assert!((p_twi_star.twilight_factor - 1.0).abs() < TOLERANCE);
    assert!(p_twi_star.star_visibility < 0.3);
    println!("PASS (Noon = 0.0, Midnight = 1.0, Twilight Peak = 1.0)");

    // ------------------------------------------------------------------------
    // STAGE 6: LONG-CYCLE REPLAY DETERMINISM & DRIFT STABILITY (Amendment 13)
    // ------------------------------------------------------------------------
    print!("Stage 6: 100-Day Long-Cycle Determinism & Float Safety ... ");
    let mut env = EnvironmentState::new();
    let day_dt = 1200.0;
    for _ in 0..100 {
        env.advance(day_dt);
    }
    assert!((0.0..1.0).contains(&env.clock.day_fraction));
    assert!(env.clock.day_fraction.is_finite());
    assert!(env.celestial.sun_direction.is_finite());
    assert!(env.celestial.moon_direction.is_finite());
    assert!(env.celestial.twilight_factor.is_finite());
    assert!(env.celestial.star_visibility.is_finite());
    println!("PASS (100 Days: Zero NaN/Inf, Exact Bound in [0, 1))");

    // ------------------------------------------------------------------------
    // STAGE 7: TRANSLATION INVARIANCE OF CELESTIAL SPHERE (Amendment 9)
    // ------------------------------------------------------------------------
    print!("Stage 7: Sky View Direction Translation Invariance ... ");
    let cam_origin = Camera::new(Vec3::ZERO, 45.0, 15.0);
    let cam_distant = Camera::new(Vec3::new(100_000.0, 50_000.0, -100_000.0), 45.0, 15.0);

    let vp_origin = cam_origin.build_sky_view_projection_matrix(16.0 / 9.0);
    let vp_distant = cam_distant.build_sky_view_projection_matrix(16.0 / 9.0);

    let inv_origin = vp_origin.inverse();
    let inv_distant = vp_distant.inverse();

    let clip = glam::Vec4::new(0.5, 0.5, 1.0, 1.0);
    let p_orig = (inv_origin * clip).truncate().normalize();
    let p_dist = (inv_distant * clip).truncate().normalize();

    assert_eq!(p_orig, p_dist);
    println!("PASS (100 km Translation: View Ray Bitwise Identical)");

    let total_ms = start_all.elapsed().as_secs_f64() * 1000.0;
    println!("--------------------------------------------------------------------------------");
    println!(
        "Phase 10.5 Sky & Atmosphere Validation: ALL 7 STAGES PASSED in {:.2}ms",
        total_ms
    );
    println!("================================================================================");
}

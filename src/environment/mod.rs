pub mod aurora;
pub mod celestial;
pub mod sky;
pub mod time;

pub use aurora::{evaluate_aurora_reference, AuroraParameters, AuroraReferenceResult};
pub use celestial::{lerp_vec3, smoothstep, CelestialParameters};
pub use sky::{EnvironmentState, SkyUniform};
pub use time::{EnvironmentClock, MoonPhase};

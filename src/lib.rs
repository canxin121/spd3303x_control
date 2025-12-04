pub mod instrument;

// Re-export the primary types so users can depend on the crate
// without knowing the internal module layout, mirroring sdg2000x_control.
pub use instrument::*;

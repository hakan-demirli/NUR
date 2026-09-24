pub mod hunks;
pub mod patch_applicator;
pub mod preflight_checks;

pub use patch_applicator::apply_patch;
pub use preflight_checks::run_preflight_checks;

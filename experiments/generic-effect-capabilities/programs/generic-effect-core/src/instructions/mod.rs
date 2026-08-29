//! Raw private instruction handlers for the disposable experiment.

pub mod activate_stored_authorization;
pub mod approve_exact_delegate;
pub mod cancel_stored_authorization;
pub mod capture_immutable_release;
pub mod execute_effect_full;
mod execution_preflight;
pub mod initialize_stored_authorization;
pub mod replace_stored_authorization;
pub mod write_stored_authorization;

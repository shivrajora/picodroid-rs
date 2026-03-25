#[cfg(not(any(test, feature = "sim")))]
pub mod flash;
#[cfg(not(any(test, feature = "sim")))]
pub mod pending;
#[cfg(not(any(test, feature = "sim")))]
mod protocol;
#[cfg(not(any(test, feature = "sim")))]
mod task;

#[cfg(not(any(test, feature = "sim")))]
pub use task::run_pdb_task;

//! kube-subcontroller executes a set of subroutines ("subcontrollers")
//! in response to configuration changes from the apiserver,
//! e.g. run a subcontroller watching a new GVK when a CRD is created.
//!
//! The process subscribes to events from a certain object list-watch (e.g. CRD),
//! which indicate the associated subcontrollers for this key to start, stop or restart.
//! There would be up to N &times; M subcontrollers running,
//! where N is the number of keys (e.g. number of CRDs),
//! and M is the number of subcontrollers to run for each key.

#![allow(unused_imports)]

pub mod config;
pub use config::Config;
mod runner;
pub mod subscriber;
pub use runner::{run, RunError};

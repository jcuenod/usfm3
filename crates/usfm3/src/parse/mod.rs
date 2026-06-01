//! The two-stage parse pipeline: [`scan`] (SIMD structural pass) then [`build`]
//! (recovering tape construction). Entry point is [`crate::parse`].

pub(crate) mod build;
pub(crate) mod scan;

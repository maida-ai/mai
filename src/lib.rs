//! MVP authority foundations with explicit trusted-local receive admission.
pub mod lifecycle;

pub mod authority;
pub mod engine;
pub mod repository_scope;
pub mod state;

pub mod approval;
pub mod bootstrap;
pub mod lease;
pub mod native_path;
pub mod policy;
pub mod process;
pub mod slots;
pub mod validator;
pub mod workspace;

pub mod landing;
pub mod manifest;
pub mod receive;

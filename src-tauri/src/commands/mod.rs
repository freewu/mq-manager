//! Tauri IPC surface. Every command is a thin adapter: resolve the live
//! connection, call the neutral `MqConnection` API, return the DTO.
//!
//! Because the commands only ever touch the neutral traits, adding a broker
//! never requires touching this layer.

pub mod app;
pub mod cluster;
pub mod connections;
pub mod groups;
pub mod messages;
pub mod topics;

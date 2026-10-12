//! Lógica do daemon `ghubd`.

pub mod backend;
pub mod battery_sysfs;
pub mod mouse;
pub mod notify;
pub mod profile;
pub mod topology;
pub mod tray_icon;
pub mod tray_model;
pub mod watcher;
pub mod manager;
pub mod service;
#[cfg(test)]
mod testing;

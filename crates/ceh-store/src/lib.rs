//! Read-only legacy data adapters. No native vault or browser access.
pub mod legacy_yaml;
pub mod migrate;
pub mod notices;
pub mod periods;
pub mod profiles;
pub mod statistics;
pub mod timeline;
pub mod writer;

pub mod profile_write;

pub mod notification_metadata;

pub mod selection;

pub mod live;

pub mod appointment_budget;

pub mod legacy_import;

mod file_lock;

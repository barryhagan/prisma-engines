#![cfg_attr(target_arch = "wasm32", allow(unused_imports))]

#[cfg(feature = "postgresql-native")]
mod native;
#[cfg(feature = "postgresql-native")]
pub use native::*;

#[cfg(not(feature = "postgresql-native"))]
mod wasm;
#[cfg(not(feature = "postgresql-native"))]
pub use wasm::*;

use super::{Circumstances, MigratePostgresUrl, PostgresProvider, setup_connection};

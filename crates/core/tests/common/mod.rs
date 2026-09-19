//! Shared by the integration tests. `common/mod.rs`, not `common.rs`, so cargo does
//! not build it as a test binary of its own.

use lekhai::Engine;

pub fn assets() -> String {
    format!("{}/assets", env!("CARGO_MANIFEST_DIR"))
}

/// The engine loaded from the shipped models. Real assets rather than mocks,
/// deliberately: a model/code mismatch is the failure that actually happens here.
pub fn engine() -> Engine {
    Engine::load(&assets()).expect("load the engine from crates/core/assets")
}

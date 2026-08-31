//! This crate is the `getrandom` 0.4 counterpart of `ic-dummy-getrandom-for-wasm`,
//! which only registers a backend for `getrandom` 0.2.
//!
//! `getrandom` refuses to compile for `wasm32-unknown-unknown` by default, because it
//! has no way to obtain entropy there. Canisters must not use it anyway: the only
//! source of randomness on the IC is the `raw_rand` endpoint of the management
//! canister. Registering a backend that always fails turns the compile time error
//! into a runtime error for the code paths that are never taken on chain.
//!
//! The backend is selected by `--cfg getrandom_backend="custom"`, which
//! `.cargo/config.toml` sets for the `wasm32-unknown-unknown` target. The registration
//! lives in its own crate because the `__getrandom_v03_custom` symbol MUST be defined
//! exactly once per binary; every canister depends on this crate rather than
//! registering its own.

#[cfg(all(
    target_arch = "wasm32",
    target_vendor = "unknown",
    target_os = "unknown"
))]
/// A getrandom implementation that always fails.
#[unsafe(no_mangle)]
unsafe extern "Rust" fn __getrandom_v03_custom(
    _dest: *mut u8,
    _len: usize,
) -> Result<(), getrandom::Error> {
    Err(getrandom::Error::UNSUPPORTED)
}

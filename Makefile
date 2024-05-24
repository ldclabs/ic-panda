BUILD_ENV := rust

.PHONY: build-wasm build-did

lint:
	@cargo fmt
	@cargo clippy --all-targets --all-features

fix:
	@cargo clippy --fix --workspace --tests

# cargo install twiggy
twiggy:
	twiggy top -n 12 target/wasm32-unknown-unknown/release/ic_panda_luckypool.wasm

# cargo install ic-wasm
build-wasm:
	cargo build --release --target wasm32-unknown-unknown --package ic_panda_ai
	cargo build --release --target wasm32-unknown-unknown --package ic_panda_luckypool

shrink-wasm:
	ic-wasm -o target/wasm32-unknown-unknown/release/ic_panda_ai_optimized.wasm target/wasm32-unknown-unknown/release/ic_panda_ai.wasm shrink

# cargo install candid-extractor
build-did:
	candid-extractor target/wasm32-unknown-unknown/release/ic_panda_ai.wasm > src/ic_panda_ai/ic_panda_ai.did
	candid-extractor target/wasm32-unknown-unknown/release/ic_panda_luckypool.wasm > src/ic_panda_luckypool/ic_panda_luckypool.did

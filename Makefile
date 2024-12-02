BUILD_ENV := rust

.PHONY: build-wasm build-did

lint:
	@cargo fmt
	@cargo clippy --all-targets --all-features

fix:
	@cargo clippy --fix --workspace --tests

test:
	@cargo test --workspace -- --nocapture

# cargo install twiggy
twiggy:
	twiggy top -n 12 target/wasm32-unknown-unknown/release/ic_panda_luckypool.wasm

# cargo install ic-wasm
build-wasm:
	cargo build --release --target wasm32-unknown-unknown --package ic_message
	cargo build --release --target wasm32-unknown-unknown --package ic_message_channel
	cargo build --release --target wasm32-unknown-unknown --package ic_message_profile
	cargo build --release --target wasm32-unknown-unknown --package ic_panda_luckypool

# cargo install candid-extractor
build-did:
	candid-extractor target/wasm32-unknown-unknown/release/ic_message.wasm > src/ic_message/ic_message.did
	candid-extractor target/wasm32-unknown-unknown/release/ic_message_channel.wasm > src/ic_message_channel/ic_message_channel.did
	candid-extractor target/wasm32-unknown-unknown/release/ic_message_profile.wasm > src/ic_message_profile/ic_message_profile.did
	candid-extractor target/wasm32-unknown-unknown/release/ic_panda_luckypool.wasm > src/ic_panda_luckypool/ic_panda_luckypool.did

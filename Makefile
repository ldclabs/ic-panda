BUILD_ENV := rust

.PHONY: build-wasm build-did

lint:
	@cargo fmt
	@cargo clippy --all-targets --all-features

fix:
	@cargo clippy --fix --workspace --tests

test:
	@cargo test --workspace -- --nocapture

# cargo install ic-wasm
build-wasm:
	cargo build --release --target wasm32-unknown-unknown --package ic_delegation_store
	cargo build --release --target wasm32-unknown-unknown --package ic_dmsg_minter
	cargo build --release --target wasm32-unknown-unknown --package ic_message
	cargo build --release --target wasm32-unknown-unknown --package ic_message_channel
	cargo build --release --target wasm32-unknown-unknown --package ic_message_profile
	cargo build --release --target wasm32-unknown-unknown --package ic_name_identity
	cargo build --release --target wasm32-unknown-unknown --package ic_panda_luckypool
	cargo build --release --target wasm32-unknown-unknown --package ic_signin_with

# cargo install candid-extractor
build-did:
	candid-extractor target/wasm32-unknown-unknown/release/ic_delegation_store.wasm > src/ic_delegation_store/ic_delegation_store.did
	candid-extractor target/wasm32-unknown-unknown/release/ic_dmsg_minter.wasm > src/ic_dmsg_minter/ic_dmsg_minter.did
	candid-extractor target/wasm32-unknown-unknown/release/ic_message.wasm > src/ic_message/ic_message.did
	candid-extractor target/wasm32-unknown-unknown/release/ic_message_channel.wasm > src/ic_message_channel/ic_message_channel.did
	candid-extractor target/wasm32-unknown-unknown/release/ic_message_profile.wasm > src/ic_message_profile/ic_message_profile.did
	candid-extractor target/wasm32-unknown-unknown/release/ic_name_identity.wasm > src/ic_name_identity/ic_name_identity.did
	candid-extractor target/wasm32-unknown-unknown/release/ic_panda_luckypool.wasm > src/ic_panda_luckypool/ic_panda_luckypool.did
	candid-extractor target/wasm32-unknown-unknown/release/ic_signin_with.wasm > src/ic_signin_with/ic_signin_with.did
	dfx generate
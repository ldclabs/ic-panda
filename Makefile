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
	cargo build --release --target wasm32-unknown-unknown -p ic_delegation_store -p ic_dmsg_minter -p ic_message -p ic_message_channel -p ic_message_profile -p ic_name_identity -p ic_panda_luckypool -p ic_signin_with

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
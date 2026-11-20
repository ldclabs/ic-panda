# dMsg for Android

Native Kotlin/Compose app using the same in-process Rust client as iOS. The [validation log](VALIDATION.md) records the iOS versions used for comparison, completed checks and outstanding work.

For the interaction fixes, emulator and local ledger results, and remaining acceptance checks, see the [Android UX review (Chinese)](docs/ux-review-2026-11-18.zh.md).

For the owner, transitions, and regression tests for each user flow, see the [Android state machine index (Chinese)](docs/state-machines.zh.md).

## Local development

Vault editors checkpoint complete, possibly unfinished input through the shared
Rust core into the account's encrypted local workspace. Backgrounding and Activity
recreation retain the editor. Locking removes its plaintext; unlocking the same
account or reopening the app restores the active draft and its original item
revision. Keep draft closes the editor without discarding its local checkpoint.
Save atomically commits the item and consumes the draft, and late writes cannot
revive a saved or discarded draft. These drafts are device-local and do not enter
cloud synchronization or device-transfer records.

Install the Android SDK 36, build tools 36, NDK 28.2.13676358 and a JDK 21. Set ANDROID_HOME and JAVA_HOME in the local environment. Install the Rust targets with:

    rustup target add aarch64-linux-android x86_64-linux-android

From this directory:

    ./gradlew :app:testProductionNoneDebugUnitTest
    ./gradlew :app:assembleProductionNoneDebug -Pdmsg.abis=arm64-v8a

The production debug application ID is net.dmsg.android.dev (staging adds .staging); the intended production ID is net.dmsg.android. The none flavor does not include Firebase. The gms flavor is compiled separately; adding an SDK alone does not configure notification delivery.

For a local production GMS build, place the Firebase Android download at `app/google-services.json` and run `./gradlew :app:bundleProductionGmsRelease`. Gradle extracts the public fields for `net.dmsg.android`. To supply the normalized client JSON instead, pass `-Pdmsg.googleConfig=/absolute/path/to/android-client.json`. Only GMS variants matching the configured package and environment embed it. Push registration still requires user consent.

Generated Kotlin and libraries live in app/build/generated/dmsg. Gradle builds current bindings and the requested ABI slices, then verifies source/output fingerprints before native packaging. The app verifies its generated fingerprint against the loaded Rust library before opening a workspace. Never edit generated bindings or use stale libraries to fix a compiler error.

The rustls JVM component uses the published org.rustls Maven group and the companion version from Cargo.lock. Gradle verification-metadata.xml pins downloaded artifacts. Changes to dependency verification must be reviewed against their upstream source. The AAR owns @xml/network_security_config: its default is HTTPS-only, with a CCADB list of CA revocation hosts allowed to fetch signed CRLs over HTTP. Do not shadow that resource; doing so makes valid CRL-only certificates fail as revoked. No certificate/hostname verification is disabled. See [upstream issue 221](https://github.com/rustls/rustls-platform-verifier/issues/221).

## First publication

See [the first-release guide](FIRST_RELEASE_zh.md). Local builds do not require a Play developer account. Production App Links, native Passkey origin admission, remote notifications and billing need separately configured services; they cannot be verified by emulator fixture screens.

## Platform security

An Android hardware-secret record has a versioned opaque handle and context. RSA-OAEP public wrapping is noninteractive; per-use authenticated private decryption opens the secret. This preserves the core's noninteractive creation contract without invoking a biometric prompt while Client is borrowed. Independent P-256 keys handle device anchors and user-presence approvals.

Each prepared signing operation binds one eligible handle to its final, reviewed message and a bounded deadline. The Android Signature is authenticated outside Client, consumed once, and discarded on cancellation or scope release. Wallet batches prepare and consume one operation at a time. Strict-biometry policy is explicit; Android never fabricates an enrollment-set digest. TEE/StrongBox and actual authenticator combinations still require physical-device validation.

Only the main application owns a Client. AutoFill, document copies, incoming sharing and background ciphertext tasks must use the bounded interfaces described in the architecture; no component may open a second workspace writer or receive account root keys.
Password AutoFill maintains an encrypted local login catalog while the vault is unlocked.
The authenticated system picker supports searching all logins, one-time filling and
remembering several accounts for the same signed app or verified website. The
selected dataset fills the requesting fields immediately. If the catalog or
authentication expires during an active selection, the picker reauthenticates
and resumes the confirmed account only if the item and displayed login still
match. Remembered choices also require the same verified destination. Retrieved file copies
retain their separate expiry and authentication policy.


Build dimensions are production/staging × none/gms. Pass -Pdmsg.stagingConfig=/path/to/local/deployment.json for staging; required fields are relay, handle, user_homes and connect_url, with optional auth/wallet_hub. These are build inputs, not deep-link parameters. Production continues to use the shared Rust production.toml.

Release signing is injected through DMSG_ANDROID_KEYSTORE, DMSG_ANDROID_STORE_PASSWORD, DMSG_ANDROID_KEY_ALIAS and DMSG_ANDROID_KEY_PASSWORD. Without them, local release outputs remain unsigned. Never use the development certificate for production Passkey admission. Gradle dependency locks and verification metadata are checked in with the Android project; update them only with reviewed dependency changes.

For actual package validation, run python3 ../../scripts/android/verify-package.py app/build/outputs/apk/productionNone/release/app-production-none-release-unsigned.apk. This checks all packaged ABI libraries, not only the Rust linker flag. Device runtime and final store-signed artifacts remain separate checks.

## Google Play internal testing

Before each upload, increment `versionCode` in `app/build.gradle.kts` and set the user-facing `versionName`. From the repository root run:

```sh
python3 scripts/android/internal-test.py
```

Supply the production Firebase configuration as described above. On macOS the script can use the dMsg upload keystore in `~/Library/Application Support/dMsg/PlaySigning/` and the login Keychain item with service `net.dmsg.android.play-upload` and account `dmsg-upload`. For another keystore, set the four release signing variables listed above. The script builds both ABIs, verifies the upload signature and native libraries, and prints the versioned AAB path under `misc/dmsg-play-internal/` for manual upload.

## Repeatable validation

From the repository root, `bash scripts/android/verify.sh resources` checks all 23 languages, format placeholders, generated resources and shared notification vocabulary. Missing translations fail the gate; English fallback does not count as a translation. `build` additionally checks shared contracts, JVM tests, four build variants, Lint, both production Release APK/AAB outputs and every packaged native library. Set `DMSG_ANDROID_ABIS` only when deliberately running a subset.

For `device`, set `ANDROID_SERIAL` to a dedicated emulator: it installs and removes QA/test packages, runs both none and GMS fixture suites, then tests the ordinary native build and its live TLS probe. The opt-in 1000 MiB, 1000-item vault and real hardware probes remain separate. No fixture can sign in, pay, or grant a real entitlement. Real hardware, cross-device service flows and store-signed split installation still require the release matrix in VALIDATION.md.

For a configured CI build, run the dMsg Android workflow manually with `firebase_bundle` enabled. The package job reads the `DMSG_ANDROID_CLIENT_JSON` configuration variable from the `dmsg-release` GitHub environment. Supply the normalized client JSON as its value.

The Android workflow runs these gates on pull requests affecting the app or shared core. Its Linux API 31/36 x86_64 emulator jobs report their actual page size; they do not replace the separate ARM64 16 KiB check. The workflow has no signing or service secrets and does not publish. SDK provisioning uses the [Android command-line tools](https://developer.android.com/tools/sdkmanager) and the installed emulator interface described in the [official emulator guide](https://developer.android.com/studio/run/emulator-commandline). JDK setup follows [actions/setup-java](https://github.com/actions/setup-java).


## Isolated UI QA

The qa build type is a separate test package, using the Rust ui-fixtures feature and generated/dmsg-qa outputs. Ordinary debug and release builds have no fixture constructor. The tests open fresh encrypted offline databases through real Rust APIs; fixtures cannot authenticate or deliver content. Run on a dedicated emulator:

```bash
./gradlew :app:testProductionNoneQaUnitTest :app:connectedProductionNoneQaAndroidTest -Pdmsg.testBuildType=qa -Pdmsg.abis=arm64-v8a
```

The default instrumentation build type remains debug. QA screenshots contain synthetic data only, and can be pulled from /sdcard/Download/dmsg-qa. Full release, network, physical-hardware and provider checks are separate gates in VALIDATION.md.


QR scanning uses a lifecycle-bound CameraX 1.6.2 preview and the shared Rust QR decoder in both flavors. The scanner accepts up to 4 million luminance pixels per camera frame. Selected images can contain up to 8 MiB of compressed data and 40 million source pixels and are decoded at reduced resolution. Scanning fills a field for review. Pairing and OTP storage each require confirmation. Users can select an image or paste text if they decline camera access. Camera hardware and cross-device enrollment have separate validation gates. [CameraX releases](https://developer.android.com/jetpack/androidx/releases/camera)

For standalone QA profiling with real input timing, install the QA variant and launch `net.dmsg.android.qa/net.dmsg.android.QaWorkspaceActivity` with the boolean `scale_probe` extra. It creates 1000 synthetic encrypted records in a fresh fixture workspace and preloads their pages; wait for the `dMsgScaleProbe` ready log before measuring. This Activity and seed path are absent from ordinary Debug/Release. Compose test-clock timings must not be presented as real user frame-rate measurements.


For a real minified APK or installed AAB splits, use `scripts/android/smoke-release.py` from the repository root with `--serial`, `--disposable-emulator` and `--output`. The target must be a dedicated emulator with the ordinary `net.dmsg.android` package at its English no-account welcome. `--login-probe` opts into public deployment reads and opening Chrome; the script never records the login fragment or enters credentials. It records three cold-process launches (warm OS caches) and their actual UI trees. This external driver avoids injecting the instrumentation runner's shared classes into a shrunk release process. Direct in-process Release instrumentation encountered an AndroidX tracing method removed by app R8; that failure is recorded in VALIDATION.md, not counted as a test pass.

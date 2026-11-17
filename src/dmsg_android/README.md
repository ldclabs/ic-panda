# dMsg for Android

Native Kotlin/Compose app using the same in-process Rust client as iOS. The Android implementation is being aligned with iOS 1.2.1 (32); see [validation](VALIDATION.md) for actual checks and outstanding work. There is no daemon or web wrapper.

## Local development

Install the Android SDK 36, build tools 36, NDK 28.2.13676358 and a JDK 21. Set ANDROID_HOME and JAVA_HOME in the local environment. Install the Rust targets with:

    rustup target add aarch64-linux-android x86_64-linux-android

From this directory:

    ./gradlew :app:testProductionNoneDebugUnitTest
    ./gradlew :app:assembleProductionNoneDebug -Pdmsg.abis=arm64-v8a

The production debug application ID is net.dmsg.android.dev (staging adds .staging); the intended production ID is net.dmsg.android. The none flavor does not include Firebase. The gms flavor is compiled separately; adding an SDK alone does not configure notification delivery.

Generated Kotlin and libraries live in app/build/generated/dmsg. Gradle builds current bindings and the requested ABI slices, then verifies source/output fingerprints before native packaging. The app verifies its generated fingerprint against the loaded Rust library before opening a workspace. Never edit generated bindings or use stale libraries to fix a compiler error.

The rustls JVM component uses the published org.rustls Maven group and the companion version from Cargo.lock. Gradle verification-metadata.xml pins downloaded artifacts. Changes to dependency verification must be reviewed against their upstream source. The AAR owns @xml/network_security_config: its default is HTTPS-only, with a CCADB list of CA revocation hosts allowed to fetch signed CRLs over HTTP. Do not shadow that resource; doing so makes valid CRL-only certificates fail as revoked. No certificate/hostname verification is disabled. See [upstream issue 221](https://github.com/rustls/rustls-platform-verifier/issues/221).

## First publication

See [the first-release guide](FIRST_RELEASE_zh.md). Local builds do not require a Play developer account. Production App Links, native Passkey origin admission, remote notifications and billing need separately configured services; they cannot be verified by emulator fixture screens.

## Platform security

An Android hardware-secret record has a versioned opaque handle and context. RSA-OAEP public wrapping is noninteractive; per-use authenticated private decryption opens the secret. This preserves the core's noninteractive creation contract without invoking a biometric prompt while Client is borrowed. Independent P-256 keys handle device anchors and user-presence approvals.

Each prepared signing operation binds one eligible handle to its final, reviewed message and a bounded deadline. The Android Signature is authenticated outside Client, consumed once, and discarded on cancellation or scope release. Wallet batches prepare and consume one operation at a time. Strict-biometry policy is explicit; Android never fabricates an enrollment-set digest. TEE/StrongBox and actual authenticator combinations still require physical-device validation.

Only the main application owns a Client. AutoFill, document copies, incoming sharing and background ciphertext tasks must use the bounded interfaces described in the architecture; no component may open a second workspace writer or receive account root keys.

Build dimensions are production/staging × none/gms. Pass -Pdmsg.stagingConfig=/path/to/local/deployment.json for staging; required fields are relay, handle, user_homes and connect_url, with optional auth/wallet_hub. These are build inputs, not deep-link parameters. Production continues to use the shared Rust production.toml.

Release signing is injected through DMSG_ANDROID_KEYSTORE, DMSG_ANDROID_STORE_PASSWORD, DMSG_ANDROID_KEY_ALIAS and DMSG_ANDROID_KEY_PASSWORD. Without them, local release outputs remain unsigned. Never use the development certificate for production Passkey admission. Gradle dependency locks and verification metadata are checked in with the Android project; update them only with reviewed dependency changes.

For actual package validation, run python3 ../../scripts/android/verify-package.py app/build/outputs/apk/productionNone/release/app-production-none-release-unsigned.apk. This checks all packaged ABI libraries, not only the Rust linker flag. Device runtime and final store-signed artifacts remain separate checks.

## Repeatable validation

From the repository root, `bash scripts/android/verify.sh resources` checks all 23 languages, format placeholders, generated resources and shared notification vocabulary. Missing translations fail the gate; English fallback does not count as a translation. `build` additionally checks shared contracts, JVM tests, four build variants, Lint, both production Release APK/AAB outputs and every packaged native library. Set `DMSG_ANDROID_ABIS` only when deliberately running a subset.

For `device`, set `ANDROID_SERIAL` to a dedicated emulator: it installs and removes QA/test packages, runs both none and GMS fixture suites, then tests the ordinary native build and its live TLS probe. The opt-in 1000 MiB, 1000-item vault and real hardware probes remain separate. No fixture can sign in, pay, or grant a real entitlement. Real hardware, cross-device service flows and store-signed split installation still require the release matrix in VALIDATION.md.

The Android workflow runs these gates on pull requests affecting the app or shared core. Its Linux API 31/36 x86_64 emulator jobs report their actual page size; they do not replace the separate ARM64 16 KiB check. The workflow has no signing or service secrets and does not publish. SDK provisioning uses the [Android command-line tools](https://developer.android.com/tools/sdkmanager) and the installed emulator interface described in the [official emulator guide](https://developer.android.com/studio/run/emulator-commandline). JDK setup follows [actions/setup-java](https://github.com/actions/setup-java).


## Isolated UI QA

The qa build type is a separate test package, using the Rust ui-fixtures feature and generated/dmsg-qa outputs. Ordinary debug and release builds have no fixture constructor. The tests open fresh encrypted offline databases through real Rust APIs; fixtures cannot authenticate or deliver content. Run on a dedicated emulator:

```bash
./gradlew :app:testProductionNoneQaUnitTest :app:connectedProductionNoneQaAndroidTest -Pdmsg.testBuildType=qa -Pdmsg.abis=arm64-v8a
```

The default instrumentation build type remains debug. QA screenshots contain synthetic data only, and can be pulled from /sdcard/Download/dmsg-qa. Full release, network, physical-hardware and provider checks are separate gates in VALIDATION.md.


QR scanning uses [CameraX1.6.2](https://developer.android.com/jetpack/androidx/releases/camera) with a lifecycle-bound preview and the shared Rust QR decoder. The scanner has no GMS dependency. Camera frames are bounded to4million luminance pixels; selected images are bounded to8MiB compressed/40million source pixels and decoded at reduced resolution. QR content only fills a reviewable field; pairing and OTP storage still require their own confirmation/save. Camera permission denial keeps image/paste alternatives available. Camera hardware and cross-device enrollment remain separate validation gates.

For standalone QA profiling with real input timing, install the QA variant and launch `net.dmsg.android.qa/net.dmsg.android.QaWorkspaceActivity` with the boolean `scale_probe` extra. It creates 1000 synthetic encrypted records in a fresh fixture workspace and preloads their pages; wait for the `dMsgScaleProbe` ready log before measuring. This Activity and seed path are absent from ordinary Debug/Release. Compose test-clock timings must not be presented as real user frame-rate measurements.


For a real minified APK or installed AAB splits, use `scripts/android/smoke-release.py` from the repository root with `--serial`, `--disposable-emulator` and `--output`. The target must be a dedicated emulator with the ordinary `net.dmsg.android` package at its English no-account welcome. `--login-probe` opts into public deployment reads and opening Chrome; the script never records the login fragment or enters credentials. It records three cold-process launches (warm OS caches) and their actual UI trees. This external driver avoids injecting the instrumentation runner's shared classes into a shrunk release process. Direct in-process Release instrumentation encountered an AndroidX tracing method removed by app R8; that failure is recorded in VALIDATION.md, not counted as a test pass.

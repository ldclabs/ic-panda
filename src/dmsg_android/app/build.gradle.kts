import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.plugin.compose")
}

val requestedAbis = providers.gradleProperty("dmsg.abis").orElse("arm64-v8a,x86_64").get().split(",")
val instrumentationType = providers.gradleProperty("dmsg.testBuildType").orElse("debug").get()
require(requestedAbis.isNotEmpty() && requestedAbis.toSet().size == requestedAbis.size && requestedAbis.all { it in setOf("arm64-v8a", "x86_64") }) { "dmsg.abis must contain arm64-v8a and/or x86_64" }
val googleConfig = providers.gradleProperty("dmsg.googleConfig").orNull?.let { path ->
    val source = file(path)
    require(source.length() in 1..16384) { "dmsg.googleConfig must be a bounded public Firebase client configuration" }
    val decoded = try { groovy.json.JsonSlurper().parseText(source.readText()) }
        catch (_: Exception) { throw GradleException("dmsg.googleConfig must contain a public Firebase client JSON object") }
    val fields = decoded as? Map<*, *> ?: throw GradleException("dmsg.googleConfig must be an object")
    val allowed = setOf("package_name", "environment", "firebase_app_id", "api_key", "project_id", "cloud_project_number")
    // Reject service-account files before generating BuildConfig or any APK resources.
    require(fields.keys == allowed && fields.values.all { it is String && it.isNotBlank() && it.length <= 256 && it.none(Char::isWhitespace) }) {
        "dmsg.googleConfig accepts only public Firebase client fields; service-account credentials must remain on the server"
    }
    groovy.json.JsonOutput.toJson(fields)
}.orEmpty()
val stagingConfig = providers.gradleProperty("dmsg.stagingConfig").orNull?.let { file(it).readText() }.orEmpty()
fun quoted(value: String): String = "\"" + value.replace("\\", "\\\\").replace("\"", "\\\"").replace("\n", "\\n").replace("\r", "\\r") + "\""

android {
    namespace = "net.dmsg.android"
    compileSdk = 36
    ndkVersion = "28.2.13676358"
    defaultConfig {
        applicationId = "net.dmsg.android"
        minSdk = 31
        targetSdk = 36
        versionCode = 1
        versionName = "0.1.0"
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        buildConfigField("String", "GOOGLE_CONFIG", quoted(googleConfig))
        ndk { abiFilters += requestedAbis }
    }
    flavorDimensions += listOf("environment", "push")
    productFlavors {
        create("production") { dimension = "environment"; buildConfigField("boolean", "IS_STAGING", "false"); buildConfigField("String", "DEPLOYMENT", "\"\"") }
        create("staging") { dimension = "environment"; applicationIdSuffix = ".staging"; buildConfigField("boolean", "IS_STAGING", "true"); buildConfigField("String", "DEPLOYMENT", quoted(stagingConfig)) }
        create("none") { dimension = "push"; buildConfigField("boolean", "HAS_FCM", "false") }
        create("gms") { dimension = "push"; buildConfigField("boolean", "HAS_FCM", "true") }
    }
    val releaseStore = providers.environmentVariable("DMSG_ANDROID_KEYSTORE").orNull
    if (releaseStore != null) signingConfigs.create("release") {
        storeFile = file(releaseStore)
        storePassword = providers.environmentVariable("DMSG_ANDROID_STORE_PASSWORD").get()
        keyAlias = providers.environmentVariable("DMSG_ANDROID_KEY_ALIAS").get()
        keyPassword = providers.environmentVariable("DMSG_ANDROID_KEY_PASSWORD").get()
    }
    buildTypes {
        debug { applicationIdSuffix = ".dev"; versionNameSuffix = "-dev" }
        create("qa") {
            initWith(getByName("debug")); applicationIdSuffix = ".qa"; versionNameSuffix = "-qa"
            matchingFallbacks += "debug"
        }
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            ndk { debugSymbolLevel = "FULL" }
            if (releaseStore != null) signingConfig = signingConfigs.getByName("release")
        }
    }
    testBuildType = instrumentationType
    if (testBuildType == "qa") sourceSets.getByName("androidTest").java.srcDir("src/androidTestQa/java")
    buildFeatures { compose = true; buildConfig = true }
    compileOptions { sourceCompatibility = JavaVersion.VERSION_17; targetCompatibility = JavaVersion.VERSION_17 }
    packaging {
        jniLibs { useLegacyPackaging = false }
        resources.excludes += setOf("META-INF/AL2.0", "META-INF/LGPL2.1")
    }
    // Every supported language is selectable offline, including store-installed splits.
    bundle { language { enableSplit = false } }
    testOptions { unitTests.isReturnDefaultValues = true }
}
dependencyLocking { lockAllConfigurations() }
kotlin { compilerOptions { jvmTarget.set(JvmTarget.JVM_17) } }

val repoRoot = rootProject.projectDir.resolve("../..")
abstract class GenerateDmsgBindings : Exec() {
    @get:OutputDirectory abstract val kotlinDirectory: DirectoryProperty
}
abstract class BuildDmsgNative : Exec() {
    @get:OutputDirectory abstract val nativeDirectory: DirectoryProperty
}
fun coreTasks(qa: Boolean): Pair<TaskProvider<GenerateDmsgBindings>, TaskProvider<BuildDmsgNative>> {
    val suffix = if (qa) "Qa" else ""
    val generated = if (qa) "dmsg-qa" else "dmsg"
    val flags = if (qa) listOf("--fixtures") else emptyList()
    val bindings = tasks.register<GenerateDmsgBindings>("generate${suffix}Bindings") {
        kotlinDirectory.set(layout.buildDirectory.dir("generated/$generated/kotlin"))
        workingDir(repoRoot)
        commandLine(listOf("bash", "scripts/android/build-core.sh", "--bindings-only") + flags)
        doNotTrackState("build-core.py verifies the generated Kotlin bindings")
    }
    val native = tasks.register<BuildDmsgNative>("build${suffix}Rust") {
        dependsOn(bindings)
        nativeDirectory.set(layout.buildDirectory.dir("generated/$generated/jniLibs"))
        workingDir(repoRoot)
        commandLine(listOf("bash", "scripts/android/build-core.sh", "--native-only", "--abis", requestedAbis.joinToString(",")) + flags)
        doNotTrackState("build-core.py verifies the requested native libraries")
    }
    val verify = tasks.register<Exec>("verify${suffix}Rust") {
        dependsOn(native)
        workingDir(repoRoot)
        commandLine(listOf("bash", "scripts/android/build-core.sh", "--verify", "--abis", requestedAbis.joinToString(",")) + flags)
    }
    tasks.configureEach {
        if (name.startsWith("merge") && name.endsWith("NativeLibs") && name.contains("Qa") == qa) dependsOn(verify)
    }
    return bindings to native
}
val productionCore = coreTasks(false)
val qaCore = coreTasks(true)
androidComponents.onVariants { variant ->
    val (bindings, native) = if (variant.buildType == "qa") qaCore else productionCore
    variant.sources.java?.addGeneratedSourceDirectory(bindings) { it.kotlinDirectory }
    variant.sources.jniLibs?.addGeneratedSourceDirectory(native) { it.nativeDirectory }
}

dependencies {
    implementation("androidx.core:core-ktx:1.18.0")
    implementation("androidx.activity:activity-compose:1.13.0")
    implementation("androidx.lifecycle:lifecycle-runtime-compose:2.10.0")
    implementation("androidx.lifecycle:lifecycle-viewmodel-compose:2.10.0")
    implementation("androidx.window:window:1.5.1")
    implementation("androidx.compose.ui:ui:1.11.3")
    implementation("androidx.compose.ui:ui-tooling-preview:1.11.3")
    implementation("androidx.compose.foundation:foundation:1.11.3")
    implementation("androidx.compose.material3:material3:1.4.0")
    implementation("androidx.compose.material:material-icons-extended:1.7.8")
    implementation("androidx.browser:browser:1.9.0")
    implementation("androidx.camera:camera-camera2:1.6.2")
    implementation("androidx.camera:camera-lifecycle:1.6.2")
    implementation("androidx.camera:camera-view:1.6.2")
    implementation("androidx.credentials:credentials:1.6.0")
    "gmsImplementation"("androidx.credentials:credentials-play-services-auth:1.6.0")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.10.2")
    implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.7.3")
    implementation("net.java.dev.jna:jna:5.18.1@aar")
    // The upstream README still shows the former group in one example. Use
    // the published POM coordinates and the JVM version selected by Cargo.
    val rustlsAndroidVersion = providers.fileContents(rootProject.layout.projectDirectory.file("../../Cargo.lock"))
        .asText.map { lock ->
            Regex("name = \"rustls-platform-verifier-android\"\\s+version = \"([^\"]+)\"")
                .find(lock)?.groupValues?.get(1) ?: error("rustls Android component missing from Cargo.lock")
        }
    implementation("org.rustls:rustls-platform-verifier:${rustlsAndroidVersion.get()}")
    "gmsImplementation"("com.google.firebase:firebase-messaging:24.1.0")
    "gmsImplementation"("com.google.android.play:integrity:1.6.0")
    "gmsImplementation"("com.android.billingclient:billing:9.1.0")
    debugImplementation("androidx.compose.ui:ui-tooling:1.11.3")
    debugImplementation("androidx.compose.ui:ui-test-manifest:1.11.3")
    "qaImplementation"("androidx.compose.ui:ui-test-manifest:1.11.3")
    testImplementation("junit:junit:4.13.2")
    testImplementation("org.json:json:20250517")
    androidTestImplementation("androidx.test.ext:junit:1.3.0")
    androidTestImplementation("androidx.test:runner:1.7.0")
    androidTestImplementation("androidx.compose.ui:ui-test-junit4:1.11.3")
    if (instrumentationType == "qa") androidTestImplementation("androidx.window:window-testing:1.5.1")
}

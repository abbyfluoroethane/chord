import org.gradle.process.ExecOperations
import javax.inject.Inject

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
    alias(libs.plugins.roborazzi)
}

// The Rust workspace is the parent of chord-android.
val repoRoot: File = rootDir.parentFile

// ABIs for the native core. Override with -Pchord.abis=x86_64 for a fast emulator-only build.
val rustAbis: List<String> =
    (findProperty("chord.abis") as String? ?: "arm64-v8a,x86_64").split(",").map { it.trim() }

/** Builds chord-ffi with cargo-ndk and copies one libchord_ffi.so per ABI to [outputDir]. */
abstract class CargoNdkTask : DefaultTask() {
    @get:Inject abstract val exec: ExecOperations
    @get:Input abstract val abis: ListProperty<String>
    @get:Input abstract val minSdk: Property<Int>
    @get:Internal abstract val workspace: DirectoryProperty
    @get:InputFiles @get:PathSensitive(PathSensitivity.RELATIVE) abstract val sources: ConfigurableFileCollection
    @get:OutputDirectory abstract val outputDir: DirectoryProperty

    @TaskAction
    fun build() {
        val out = outputDir.get().asFile
        out.deleteRecursively()
        exec.exec {
            workingDir = workspace.get().asFile
            commandLine(
                buildList {
                    addAll(listOf("cargo", "ndk"))
                    abis.get().forEach { addAll(listOf("-t", it)) }
                    addAll(listOf("-P", minSdk.get().toString(), "-o", out.absolutePath))
                    addAll(listOf("build", "-p", "chord-ffi", "--profile", "android"))
                },
            )
        }
    }
}

/**
 * Generates the Kotlin bindings with uniffi-bindgen. It reads the unstripped library in the
 * cargo target directory: cargo-ndk strips the copy in jniLibs, and that removes the UniFFI
 * metadata.
 */
abstract class UniffiBindgenTask : DefaultTask() {
    @get:Inject abstract val exec: ExecOperations
    @get:Internal abstract val workspace: DirectoryProperty
    // Only an input to order this task after cargoNdk and rerun it when the library changes.
    @get:InputDirectory @get:PathSensitive(PathSensitivity.RELATIVE) abstract val nativeLibs: DirectoryProperty
    @get:Input abstract val rustTarget: Property<String>
    @get:OutputDirectory abstract val outputDir: DirectoryProperty

    @TaskAction
    fun generate() {
        val out = outputDir.get().asFile
        out.deleteRecursively()
        val targetDir = System.getenv("CARGO_TARGET_DIR")?.let(::File) ?: workspace.get().asFile.resolve("target")
        val lib = targetDir.resolve("${rustTarget.get()}/android/libchord_ffi.so")
        exec.exec {
            workingDir = workspace.get().asFile
            commandLine(
                "cargo", "run", "-q", "-p", "chord-ffi", "--bin", "uniffi-bindgen", "--",
                "generate", "--library", lib.absolutePath, "--language", "kotlin",
                "--no-format", "--out-dir", out.absolutePath,
            )
        }
    }
}

val rustSources = files(
    repoRoot.resolve("Cargo.toml"),
    repoRoot.resolve("Cargo.lock"),
    fileTree(repoRoot.resolve("chord-core")) { exclude("target/**") },
    fileTree(repoRoot.resolve("chord-ffi")) { exclude("target/**", "kotlin-test/**") },
    fileTree(repoRoot.resolve("vendor")),
)

val cargoNdk = tasks.register<CargoNdkTask>("cargoNdk") {
    abis.set(rustAbis)
    minSdk.set(26)
    workspace.set(repoRoot)
    sources.from(rustSources)
    outputDir.set(layout.buildDirectory.dir("rust/jniLibs"))
}

val uniffiBindgen = tasks.register<UniffiBindgenTask>("uniffiBindgen") {
    workspace.set(repoRoot)
    nativeLibs.set(cargoNdk.flatMap { it.outputDir })
    rustTarget.set(
        when (rustAbis.first()) {
            "arm64-v8a" -> "aarch64-linux-android"
            "x86_64" -> "x86_64-linux-android"
            else -> error("unsupported ABI: ${rustAbis.first()}")
        },
    )
    outputDir.set(layout.buildDirectory.dir("generated/uniffi"))
}

android {
    namespace = "space.foid.chord"
    compileSdk = 37
    // The same NDK builds the core (cargo-ndk) and strips the libraries in the APK.
    ndkVersion = "29.0.14206865"

    defaultConfig {
        applicationId = "space.foid.chord"
        // Open question in the spec. 26 is the proposal.
        minSdk = 26
        targetSdk = 36
        versionCode = 1
        versionName = "0.1.0"
        ndk { abiFilters += rustAbis }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    buildFeatures {
        compose = true
    }

    testOptions {
        unitTests {
            isIncludeAndroidResources = true
            all {
                // Robolectric reads FileDescriptor internals. JDK 17+ hides them.
                it.jvmArgs("--add-exports=java.base/jdk.internal.access=ALL-UNNAMED", "--add-opens=java.base/java.io=ALL-UNNAMED")
                it.maxHeapSize = "2g"
            }
        }
    }
}

// -Pchord.prebuilt=<dir> skips cargo and takes <dir>/jniLibs and <dir>/uniffi from an
// earlier build. Use it for UI work, so a worktree does not rebuild the core.
// ./gradlew exportPrebuilt writes such a directory.
val prebuilt: File? = (findProperty("chord.prebuilt") as String?)?.let(::File)

androidComponents {
    onVariants { variant ->
        if (prebuilt != null) {
            variant.sources.jniLibs?.addStaticSourceDirectory(prebuilt.resolve("jniLibs").path)
            variant.sources.kotlin?.addStaticSourceDirectory(prebuilt.resolve("uniffi").path)
        } else {
            variant.sources.jniLibs?.addGeneratedSourceDirectory(cargoNdk, CargoNdkTask::outputDir)
            variant.sources.kotlin?.addGeneratedSourceDirectory(uniffiBindgen, UniffiBindgenTask::outputDir)
        }
    }
}

tasks.register<Sync>("exportPrebuilt") {
    description = "Copies the built core and bindings to -Pchord.exportTo (default: ../target/android-prebuilt)."
    from(cargoNdk.flatMap { it.outputDir }) { into("jniLibs") }
    from(uniffiBindgen.flatMap { it.outputDir }) { into("uniffi") }
    into((findProperty("chord.exportTo") as String?) ?: repoRoot.resolve("target/android-prebuilt").path)
}

// The Kotlin part of rustls-platform-verifier must have the same version as the Rust crate
// rustls-platform-verifier-android. Read it from Cargo.lock, so that the two never differ.
val rustlsPlatformVerifierVersion: String =
    repoRoot.resolve("Cargo.lock").readLines().let { lines ->
        val name = lines.indexOfFirst { it.trim() == "name = \"rustls-platform-verifier-android\"" }
        check(name >= 0) { "rustls-platform-verifier-android is not in Cargo.lock" }
        lines.drop(name + 1).first { it.trimStart().startsWith("version = ") }
            .substringAfter('"').substringBefore('"')
    }

configurations.configureEach {
    resolutionStrategy.eachDependency {
        if (requested.group == "org.rustls" && requested.name == "rustls-platform-verifier") {
            useVersion(rustlsPlatformVerifierVersion)
        }
    }
}

dependencies {
    // Certificate verification for HTTPS uploads on Android. See docs/android-tls.md.
    implementation(libs.rustls.platform.verifier)
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.core.splashscreen)
    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.navigation.compose)
    implementation(libs.androidx.lifecycle.runtime.compose)
    implementation(libs.androidx.lifecycle.viewmodel.compose)
    implementation(libs.androidx.lifecycle.service)
    implementation(libs.androidx.lifecycle.process)
    implementation(platform(libs.compose.bom))
    implementation(libs.compose.ui)
    implementation(libs.compose.foundation)
    implementation(libs.compose.material3)
    implementation(libs.compose.ui.tooling.preview)
    implementation(libs.kotlinx.coroutines.android)
    implementation("${libs.jna.get()}@aar")
    debugImplementation(libs.compose.ui.tooling)
    debugImplementation(libs.compose.ui.test.manifest)

    testImplementation(libs.junit)
    testImplementation(libs.kotlinx.coroutines.test)
    testImplementation(libs.robolectric)
    testImplementation(platform(libs.compose.bom))
    testImplementation(libs.compose.ui.test.junit4)
    testImplementation(libs.roborazzi)
    testImplementation(libs.roborazzi.compose)
    testImplementation(libs.roborazzi.junit.rule)
}

roborazzi {
    outputDir.set(file("src/test/screenshots"))
}

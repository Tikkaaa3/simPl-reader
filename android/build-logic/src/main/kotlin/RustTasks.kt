import org.gradle.api.DefaultTask
import org.gradle.api.file.ConfigurableFileCollection
import org.gradle.api.file.DirectoryProperty
import org.gradle.api.file.RegularFileProperty
import org.gradle.api.provider.ListProperty
import org.gradle.api.provider.Property
import org.gradle.api.tasks.Input
import org.gradle.api.tasks.InputDirectory
import org.gradle.api.tasks.InputFile
import org.gradle.api.tasks.InputFiles
import org.gradle.api.tasks.Internal
import org.gradle.api.tasks.OutputDirectory
import org.gradle.api.tasks.PathSensitive
import org.gradle.api.tasks.PathSensitivity
import org.gradle.api.tasks.TaskAction
import org.gradle.process.ExecOperations
import java.io.File
import javax.inject.Inject

/**
 * `cargo ndk … build -p <crate>`, then copies only `lib<libraryName>.so` into a
 * per-variant `jniLibs/<abi>/` tree. (`cargo ndk -o` would also copy dependency
 * cdylibs such as pdfium-render's, which the app never loads.)
 */
abstract class CargoNdkBuild @Inject constructor(private val exec: ExecOperations) : DefaultTask() {
    @get:Input abstract val cargo: Property<String>
    @get:Internal abstract val workspaceDir: DirectoryProperty
    @get:Input abstract val crate: Property<String>
    @get:Input abstract val libraryName: Property<String>
    @get:Input abstract val abis: ListProperty<String>
    @get:Input abstract val profile: Property<String>
    @get:Input abstract val minSdk: Property<Int>
    @get:Input abstract val ndkDir: Property<String>

    @get:InputFiles
    @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val sources: ConfigurableFileCollection

    @get:OutputDirectory abstract val outputDir: DirectoryProperty

    @TaskAction
    fun build() {
        val out = outputDir.get().asFile
        out.deleteRecursively()
        out.mkdirs()
        val cargoArgs = mutableListOf("ndk")
        abis.get().forEach { cargoArgs += listOf("-t", it) }
        cargoArgs += listOf("-P", minSdk.get().toString())
        cargoArgs += listOf("build", "-p", crate.get(), "--locked")
        // The dev profile is Cargo's default; naming it would be redundant.
        if (profile.get() != "dev") cargoArgs += listOf("--profile", profile.get())
        exec.exec {
            workingDir = workspaceDir.get().asFile
            environment("ANDROID_NDK_HOME", ndkDir.get())
            executable = cargo.get()
            args(cargoArgs)
        }

        val workspace = workspaceDir.get().asFile
        val targetDir = System.getenv("CARGO_TARGET_DIR")?.let { File(it).absoluteFile.normalize() }
            ?: File(workspace, "target")
        val profileDir = if (profile.get() == "dev") "debug" else profile.get()
        val library = "lib${libraryName.get()}.so"
        for (abi in abis.get()) {
            val triple = TRIPLES[abi] ?: error("Unsupported Android ABI: $abi")
            val built = File(targetDir, "$triple/$profileDir/$library")
            check(built.isFile) { "cargo-ndk did not produce $built" }
            built.copyTo(File(out, "$abi/$library"))
        }
    }

    private companion object {
        val TRIPLES = mapOf(
            "arm64-v8a" to "aarch64-linux-android",
            "armeabi-v7a" to "armv7-linux-androideabi",
            "x86_64" to "x86_64-linux-android",
            "x86" to "i686-linux-android",
        )
    }
}

/**
 * Stages the pinned Android PDFium (`scripts/pdfium-android.ps1`, which verifies
 * the archives) as `jniLibs/<abi>/libpdfium.so` plus its legal notices.
 */
abstract class StagePdfium @Inject constructor(private val exec: ExecOperations) : DefaultTask() {
    /** The script carries the pinned URLs, lengths and SHA-256 sums. */
    @get:InputFile
    @get:PathSensitive(PathSensitivity.NONE)
    abstract val script: RegularFileProperty

    @get:Input abstract val offline: Property<Boolean>
    @get:Internal abstract val destination: DirectoryProperty
    @get:OutputDirectory abstract val jniLibsDir: DirectoryProperty
    @get:OutputDirectory abstract val noticesDir: DirectoryProperty

    @TaskAction
    fun stage() {
        val windows = System.getProperty("os.name").startsWith("Windows")
        exec.exec {
            executable = if (windows) "powershell.exe" else "pwsh"
            args("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", script.get().asFile.absolutePath)
            args("-Destination", destination.get().asFile.absolutePath)
            if (offline.get()) args("-Offline")
        }
    }
}

/** Kotlin bindings from the built library, with the workspace-pinned generator. */
abstract class UniffiBindgen @Inject constructor(private val exec: ExecOperations) : DefaultTask() {
    @get:Input abstract val cargo: Property<String>
    @get:Internal abstract val workspaceDir: DirectoryProperty

    @get:InputDirectory
    @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val nativeLibs: DirectoryProperty

    /** Generator version (Cargo.lock) and binding configuration (uniffi.toml). */
    @get:InputFiles
    @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val bindgenSources: ConfigurableFileCollection

    @get:Input abstract val abi: Property<String>
    @get:Input abstract val libraryName: Property<String>
    @get:OutputDirectory abstract val outputDir: DirectoryProperty

    @TaskAction
    fun generate() {
        val out = outputDir.get().asFile
        out.deleteRecursively()
        out.mkdirs()
        val library = nativeLibs.get().dir(abi.get()).file("lib${libraryName.get()}.so").asFile
        check(library.isFile) { "Rust library missing for ${abi.get()}: $library" }
        exec.exec {
            workingDir = workspaceDir.get().asFile
            executable = cargo.get()
            args(
                "run", "-q", "-p", "uniffi-bindgen", "--locked", "--",
                "generate", "--library", library.absolutePath,
                "--language", "kotlin", "--out-dir", out.absolutePath, "--no-format",
            )
        }
    }
}

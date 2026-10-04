import com.android.build.api.variant.ApplicationAndroidComponentsExtension
import org.gradle.api.Plugin
import org.gradle.api.Project
import org.gradle.api.file.DirectoryProperty
import org.gradle.api.provider.ListProperty
import org.gradle.api.provider.Property
import java.io.File

/** Which Rust crate becomes the app's JNI library, and for which ABIs. */
abstract class RustAndroidExtension {
    /** Cargo workspace root (the repository root, one level above `android/`). */
    abstract val workspaceDir: DirectoryProperty

    /** Cargo package that builds the cdylib, e.g. `reader-ffi`. */
    abstract val crate: Property<String>

    /** Library stem: `lib<libraryName>.so`. */
    abstract val libraryName: Property<String>

    /** Android ABIs to compile; keep in sync with `ndk.abiFilters`. */
    abstract val abis: ListProperty<String>

    /** ABI whose library UniFFI reads to generate the Kotlin bindings. */
    abstract val bindingsAbi: Property<String>

    /** Package the pinned PDFium (`libpdfium.so`) that the Rust core loads by name. */
    abstract val pdfium: Property<Boolean>
}

/**
 * Builds the Rust core with cargo-ndk for every variant and generates its
 * Kotlin bindings with the workspace-pinned `uniffi-bindgen`. Both outputs are
 * registered as generated variant sources, so `assemble*` always packages a
 * library and bindings from the same Rust build.
 */
class RustAndroidPlugin : Plugin<Project> {
    override fun apply(project: Project) {
        val extension = project.extensions.create("rustAndroid", RustAndroidExtension::class.java)
        extension.workspaceDir.convention(project.rootProject.layout.projectDirectory.dir(".."))
        extension.abis.convention(listOf("arm64-v8a", "x86_64"))
        extension.bindingsAbi.convention("x86_64")
        extension.pdfium.convention(false)

        project.plugins.withId("com.android.application") {
            val components = project.extensions.getByType(ApplicationAndroidComponentsExtension::class.java)
            val cargoPath = cargoExecutable()
            val pdfium = project.tasks.register("stagePdfium", StagePdfium::class.java) {
                group = "rust"
                description = "Stages the pinned Android PDFium libraries and notices."
                val staged = project.layout.buildDirectory.dir("pdfium")
                script.set(extension.workspaceDir.file("scripts/pdfium-android.ps1"))
                offline.set(project.gradle.startParameter.isOffline)
                destination.set(staged)
                jniLibsDir.set(staged.map { it.dir("jniLibs") })
                noticesDir.set(staged.map { it.dir("third-party") })
            }
            components.onVariants { variant ->
                val suffix = variant.name.replaceFirstChar { it.uppercase() }
                // Debug builds use Cargo's dev profile; others the Android release
                // profile (release settings with the symbols UniFFI reads).
                val cargoProfile = if (variant.buildType == "debug") "dev" else "android-release"
                val workspace = extension.workspaceDir

                val build = project.tasks.register("cargoNdkBuild$suffix", CargoNdkBuild::class.java) {
                    group = "rust"
                    description = "Builds the Rust core for the ${variant.name} variant with cargo-ndk."
                    cargo.set(cargoPath)
                    workspaceDir.set(workspace)
                    crate.set(extension.crate)
                    libraryName.set(extension.libraryName)
                    abis.set(extension.abis)
                    profile.set(cargoProfile)
                    minSdk.set(variant.minSdk.apiLevel)
                    ndkDir.set(components.sdkComponents.ndkDirectory.map { it.asFile.absolutePath })
                    sources.from(rustSources(project, workspace))
                    outputDir.set(project.layout.buildDirectory.dir("rust/${variant.name}/jniLibs"))
                }

                val bindings = project.tasks.register("uniffiBindgen$suffix", UniffiBindgen::class.java) {
                    group = "rust"
                    description = "Generates Kotlin bindings for the ${variant.name} Rust core."
                    cargo.set(cargoPath)
                    workspaceDir.set(workspace)
                    nativeLibs.set(build.flatMap { it.outputDir })
                    abi.set(extension.bindingsAbi)
                    libraryName.set(extension.libraryName)
                    bindgenSources.from(
                        workspace.file("Cargo.lock"),
                        workspace.dir("crates/reader-ffi").map { it.file("uniffi.toml") },
                    )
                    outputDir.set(project.layout.buildDirectory.dir("rust/${variant.name}/kotlin"))
                }

                variant.sources.jniLibs?.addGeneratedSourceDirectory(build, CargoNdkBuild::outputDir)
                if (extension.pdfium.get()) {
                    variant.sources.jniLibs?.addGeneratedSourceDirectory(pdfium, StagePdfium::jniLibsDir)
                }
                val kotlin = variant.sources.kotlin ?: variant.sources.java
                kotlin?.addGeneratedSourceDirectory(bindings, UniffiBindgen::outputDir)
            }
        }
    }

    /** Everything a Rust build of the core can depend on; `target/` is excluded. */
    private fun rustSources(project: Project, workspace: DirectoryProperty) =
        workspace.map { root ->
            project.fileTree(root) {
                include(
                    "Cargo.toml",
                    "Cargo.lock",
                    "rust-toolchain.toml",
                    ".cargo/**",
                    "crates/**",
                    "patches/**",
                    "assets/**",
                )
                exclude("**/target/**")
            }
        }

    /** rustup's cargo proxy; Gradle daemons started from an IDE may lack it on PATH. */
    private fun cargoExecutable(): String {
        val exe = if (System.getProperty("os.name").startsWith("Windows")) "cargo.exe" else "cargo"
        val home = System.getenv("CARGO_HOME") ?: File(System.getProperty("user.home"), ".cargo").path
        val candidate = File(home, "bin/$exe")
        return if (candidate.isFile) candidate.absolutePath else "cargo"
    }
}

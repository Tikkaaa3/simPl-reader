import org.gradle.api.DefaultTask
import org.gradle.api.file.ConfigurableFileCollection
import org.gradle.api.file.DirectoryProperty
import org.gradle.api.provider.ListProperty
import org.gradle.api.provider.Property
import org.gradle.api.tasks.*
import org.gradle.process.ExecOperations
import java.io.File
import javax.inject.Inject

/** Full notices for the native and JVM graphs, kept alongside the packaged assets. */
abstract class StageAndroidNotices @Inject constructor(private val exec: ExecOperations) : DefaultTask() {
    @get:Internal abstract val workspace: DirectoryProperty
    @get:Internal abstract val gradleCache: DirectoryProperty
    @get:Input abstract val coordinates: ListProperty<String>
    @get:Input abstract val offline: Property<Boolean>
    @get:InputFiles @get:PathSensitive(PathSensitivity.RELATIVE) abstract val sources: ConfigurableFileCollection
    @get:OutputDirectory abstract val assets: DirectoryProperty

    @TaskAction fun stage() {
        val root = workspace.get().asFile
        val output = assets.get().dir("licenses").asFile
        require(output.canonicalFile.toPath().startsWith(File(root, "android/app/build").canonicalFile.toPath())) {
            "Generated notices must stay inside android/app/build"
        }
        // Remove stale dependency notices before indexing a changed graph.
        output.deleteRecursively()
        output.mkdirs()
        val inventory = File(temporaryDir, "jvm-coordinates.txt")
        inventory.writeText(coordinates.get().joinToString("\n"))
        val native = File(output, "native")
        listOf("arm64" to "aarch64-linux-android", "x86_64" to "x86_64-linux-android").forEach { (abi, target) ->
            exec.exec {
                workingDir(root)
                commandLine("powershell.exe", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", "scripts/collect-licenses.ps1",
                    "-Destination", native.absolutePath, "-Crate", "reader-ffi", "-Target", target)
                if (offline.get()) args("-Offline")
            }.assertNormalExitValue()
            require(File(native, "RUST-DEPENDENCIES.txt").renameTo(File(native, "RUST-DEPENDENCIES-$abi.txt")))
        }
        exec.exec {
            workingDir(root)
            commandLine("python", "scripts/android-licenses.py", "--coordinates", inventory.absolutePath,
                "--gradle-cache", gradleCache.get().asFile.absolutePath, "--destination", output.absolutePath)
            if (offline.get()) args("--offline")
        }.assertNormalExitValue()
    }
}

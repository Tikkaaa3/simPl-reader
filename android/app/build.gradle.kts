import org.gradle.api.artifacts.component.ModuleComponentIdentifier

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
    alias(libs.plugins.simpl.rust.android)
    alias(libs.plugins.baselineprofile)
}

// Phones run arm64; x86_64 is for the emulator on this Windows machine.
val appAbis = listOf("arm64-v8a", "x86_64")
val releaseVersion = providers.gradleProperty("simplVersion").orElse("0.1.0").get()
val versionParts = Regex("^(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)$")
    .matchEntire(releaseVersion)?.groupValues?.drop(1)?.map(String::toInt)
    ?: error("simplVersion must be a numeric x.y.z version")
require(versionParts[0] <= 209 && versionParts[1] <= 99 && versionParts[2] <= 99)
val releaseVersionCode = versionParts[0] * 10_000_000 + versionParts[1] * 100_000 + versionParts[2] * 1_000 + 1
val signingVariables = listOf("SIMPL_ANDROID_KEYSTORE", "SIMPL_ANDROID_STORE_PASSWORD", "SIMPL_ANDROID_KEY_ALIAS", "SIMPL_ANDROID_KEY_PASSWORD")
val signingValues = signingVariables.map { providers.environmentVariable(it).orNull }
require(signingValues.all { it.isNullOrBlank() } || signingValues.all { !it.isNullOrBlank() }) {
    "Provide all four SIMPL_ANDROID signing variables, or none for an unsigned development build"
}

// Reuse the repository's font bytes and licenses through AGP's generated sources.
abstract class StageThemeAssets : DefaultTask() {
    @get:InputDirectory abstract val fonts: DirectoryProperty
    @get:InputDirectory abstract val licenses: DirectoryProperty
    @get:OutputDirectory abstract val resources: DirectoryProperty
    @get:OutputDirectory abstract val notices: DirectoryProperty

    @TaskAction fun stage() {
        val fontOutput = resources.get().dir("font").asFile.apply { mkdirs() }
        val faces = listOf("Regular", "Medium", "Bold", "Italic", "BoldItalic")
        val reading = listOf("Literata", "Spectral", "FiraSans").flatMap { family ->
            faces.map { face -> "$family-$face.ttf" to "${family.lowercase()}_${face.lowercase()}.ttf" }
        }.toMap()
        (reading + ("Geist-UI-560.ttf" to "geist_ui.ttf")).forEach { (source, target) ->
            fonts.get().file(source).asFile.copyTo(fontOutput.resolve(target), overwrite = true)
        }
        val noticeOutput = notices.get().dir("licenses").asFile.apply { mkdirs() }
        listOf("Geist-OFL.txt", "Literata-OFL.txt", "Spectral-OFL.txt", "FiraSans-OFL.txt", "Typeface-SOURCES.txt").forEach { name ->
            licenses.get().file(name).asFile.copyTo(noticeOutput.resolve(name), overwrite = true)
        }
    }
}
val stageThemeAssets = tasks.register<StageThemeAssets>("stageThemeAssets") {
    fonts.set(layout.projectDirectory.dir("../../assets/fonts"))
    licenses.set(layout.projectDirectory.dir("../../assets/licenses"))
    resources.set(layout.buildDirectory.dir("generated/theme/res"))
    notices.set(layout.buildDirectory.dir("generated/theme/assets"))
}

android {
    namespace = "io.github.tikkaaa3.simpl"
    compileSdk = 37
    ndkVersion = "29.0.14206865"

    defaultConfig {
        applicationId = "io.github.tikkaaa3.simpl"
        minSdk = 26
        targetSdk = 37
        versionCode = releaseVersionCode
        versionName = releaseVersion
        ndk { abiFilters += appAbis }
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    signingConfigs {
        if (signingValues.all { !it.isNullOrBlank() }) create("distribution") {
            storeFile = file(signingValues[0]!!)
            require(storeFile!!.isFile) { "SIMPL_ANDROID_KEYSTORE must name an existing keystore" }
            storePassword = signingValues[1]
            keyAlias = signingValues[2]
            keyPassword = signingValues[3]
        }
    }
    buildTypes {
        release {
            signingConfig = signingConfigs.findByName("distribution")
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }
    sourceSets.getByName("androidTest").assets.srcDir("../../crates/reader-ffi/tests/fixtures")
    splits {
        abi {
            isEnable = providers.gradleProperty("simplSplitApks").orElse("false").get().toBooleanStrict()
            reset()
            include(*appAbis.toTypedArray())
            isUniversalApk = true
        }
    }
}

baselineProfile {
    mergeIntoMain = true
    automaticGenerationDuringBuild = false
    saveInSrc = true
}

// Resolve the exact shipped JVM graph, not the version catalog's direct entries.
val stageAndroidNotices = tasks.register<StageAndroidNotices>("stageAndroidNotices") {
    dependsOn("stagePdfium", stageThemeAssets)
    workspace.set(rootProject.layout.projectDirectory.dir(".."))
    gradleCache.set(File(gradle.gradleUserHomeDir, "caches/modules-2/files-2.1"))
    offline.set(gradle.startParameter.isOffline)
    assets.set(layout.buildDirectory.dir("generated/notices/assets"))
    sources.from(file("../../scripts/collect-licenses.ps1"), file("../../scripts/android-licenses.py"),
        file("../../Cargo.lock"), file("../../rust-toolchain.toml"), file("../../LICENSE.md"), file("../../LICENSE-BINARY.txt"),
        fileTree("../../crates"), fileTree("../../patches"), fileTree("../../assets/licenses"),
        layout.buildDirectory.dir("pdfium/third-party"))
}

androidComponents.onVariants { variant ->
    if (variant.name == "release") stageAndroidNotices.configure {
        val runtime = variant.runtimeConfiguration
        sources.from(runtime)
        coordinates.set(providers.provider { runtime.incoming.resolutionResult.allComponents.mapNotNull {
            (it.id as? ModuleComponentIdentifier)?.let { id -> "${id.group}:${id.module}:${id.version}" }
        }.sorted() })
    }
    variant.sources.res?.addGeneratedSourceDirectory(stageThemeAssets) { it.resources }
    variant.sources.assets?.addGeneratedSourceDirectory(stageThemeAssets) { it.notices }
    variant.sources.assets?.addGeneratedSourceDirectory(stageAndroidNotices) { it.assets }
}

rustAndroid {
    crate = "reader-ffi"
    libraryName = "reader_ffi"
    abis = appAbis
    pdfium = true
}

dependencies {
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.splashscreen)
    implementation(libs.androidx.profileinstaller)
    baselineProfile(project(":baselineprofile"))
    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.lifecycle.runtime.compose)
    implementation(libs.androidx.lifecycle.viewmodel.compose)
    implementation(libs.androidx.lifecycle.viewmodel.savedstate)
    implementation(libs.androidx.navigation.compose)
    implementation(libs.kotlinx.coroutines.android)
    implementation(platform(libs.compose.bom))
    implementation(libs.compose.ui)
    implementation(libs.compose.material3)
    implementation(libs.compose.ui.tooling.preview)
    debugImplementation(libs.compose.ui.tooling)
    // JNA's Android AAR bundles its native dispatcher for every ABI.
    implementation(variantOf(libs.jna) { artifactType("aar") })

    androidTestImplementation(libs.junit)
    androidTestImplementation(platform(libs.compose.bom))
    androidTestImplementation(libs.androidx.test.runner)
    androidTestImplementation(libs.androidx.test.ext.junit)
    androidTestImplementation(libs.compose.ui.test.junit4)
    debugImplementation(libs.compose.ui.test.manifest)
}

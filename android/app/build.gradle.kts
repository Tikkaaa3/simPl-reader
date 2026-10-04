plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
    alias(libs.plugins.simpl.rust.android)
}

// Phones run arm64; x86_64 is for the emulator on this Windows machine.
val appAbis = listOf("arm64-v8a", "x86_64")

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
        versionCode = 1
        versionName = "0.1.0-dev"
        ndk { abiFilters += appAbis }
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    buildTypes {
        release {
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
    }
    sourceSets.getByName("androidTest").assets.srcDir("../../crates/reader-ffi/tests/fixtures")
}

androidComponents.onVariants { variant ->
    variant.sources.res?.addGeneratedSourceDirectory(stageThemeAssets) { it.resources }
    variant.sources.assets?.addGeneratedSourceDirectory(stageThemeAssets) { it.notices }
}

rustAndroid {
    crate = "reader-ffi"
    libraryName = "reader_ffi"
    abis = appAbis
    pdfium = true
}

dependencies {
    implementation(libs.androidx.core.ktx)
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

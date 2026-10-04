plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
    alias(libs.plugins.simpl.rust.android)
}

// Phones run arm64; x86_64 is for the emulator on this Windows machine.
val appAbis = listOf("arm64-v8a", "x86_64")

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
}

rustAndroid {
    crate = "reader-ffi"
    libraryName = "reader_ffi"
    abis = appAbis
}

dependencies {
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.activity.compose)
    implementation(platform(libs.compose.bom))
    implementation(libs.compose.ui)
    implementation(libs.compose.material3)
    implementation(libs.compose.ui.tooling.preview)
    debugImplementation(libs.compose.ui.tooling)
    // JNA's Android AAR bundles its native dispatcher for every ABI.
    implementation(variantOf(libs.jna) { artifactType("aar") })
}

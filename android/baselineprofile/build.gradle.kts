plugins {
    alias(libs.plugins.android.test)
    alias(libs.plugins.baselineprofile)
}

android {
    namespace = "io.github.tikkaaa3.simpl.baselineprofile"
    compileSdk = 37
    targetProjectPath = ":app"
    defaultConfig {
        minSdk = 28
        targetSdk = 37
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        // Emulator results are diagnostic, not physical-device performance claims.
        testInstrumentationRunnerArguments["androidx.benchmark.suppressErrors"] = "EMULATOR"
    }
    buildTypes { create("release") { isDebuggable = false } }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

baselineProfile { useConnectedDevices = true }

dependencies {
    implementation(libs.androidx.benchmark)
    implementation(libs.androidx.test.runner)
    implementation(libs.androidx.test.ext.junit)
}

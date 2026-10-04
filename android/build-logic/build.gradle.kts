plugins {
    `kotlin-dsl`
}

dependencies {
    compileOnly(libs.android.gradle.api)
}

gradlePlugin {
    plugins {
        register("rustAndroid") {
            id = "simpl.rust-android"
            implementationClass = "RustAndroidPlugin"
        }
    }
}

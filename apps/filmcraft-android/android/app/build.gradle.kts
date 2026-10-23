plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "ai.storyteller.filmcraft"
    compileSdk = 35

    defaultConfig {
        applicationId = "ai.storyteller.filmcraft"
        minSdk = 29          // Vulkan 1.1 for wgpu
        targetSdk = 34       // 35+ forces edge-to-edge, which the egui surface does not handle yet
        versionCode = 1
        versionName = "0.9.0"
        ndk { abiFilters += listOf("arm64-v8a") }
    }
    buildTypes {
        release { isMinifyEnabled = false }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions { jvmTarget = "17" }
}

dependencies {
    implementation("androidx.appcompat:appcompat:1.7.0")
    implementation("androidx.core:core-ktx:1.13.1")
    // Must match the GameActivity version the android-activity crate documents (see its README).
    implementation("androidx.games:games-activity:2.0.2")
}

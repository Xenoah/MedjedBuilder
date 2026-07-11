plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "io.html2apk.shell"
    compileSdk = 35

    defaultConfig {
        applicationId = "io.html2apk.generated"
        minSdk = 26
        targetSdk = 35
        versionCode = 1
        versionName = "__H2A_VERSION_NAME__"
    }

    buildTypes {
        release {
            isMinifyEnabled = false
        }
    }
}

dependencies {
    implementation("androidx.documentfile:documentfile:1.0.1")
}


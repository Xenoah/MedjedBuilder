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

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = "17"
    }
}

dependencies {
    implementation("androidx.documentfile:documentfile:1.0.1")
}


// aapt2 optimizeによるリソースパス短縮を無効化する。
// 短縮されると res/drawable-*/icon_payload.png が res/xx.png のような名前になり、
// MedjedBuilderのアイコン差し替え（エントリ名で照合）が効かなくなる。
tasks.configureEach {
    if (name == "optimizeReleaseResources") {
        enabled = false
    }
}

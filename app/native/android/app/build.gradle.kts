import java.util.Properties

plugins { id("com.android.application") }

// Version properties are written by `leptoskit native prepare android`.
val leptoskitVersion =
    Properties().apply {
        val versionFile = file("../../.leptoskit/android/version.properties")
        if (versionFile.exists()) {
            versionFile.inputStream().use { load(it) }
        } else {
            setProperty("versionCode", "1")
            setProperty("versionName", "0.1.0")
        }
    }

android {
    namespace = "dev.monochange.app"
    val ndkPathFile = file("../../.leptoskit/android/ndk-path.txt")

    if (ndkPathFile.exists()) {
        ndkPath = ndkPathFile.readText().trim()
        ndkVersion =
            Properties()
                .apply {
                    file("$ndkPath/source.properties").inputStream().use { load(it) }
                }.getProperty("Pkg.Revision") ?: error("Selected Android NDK is missing Pkg.Revision")
    }

    compileSdk = 36

    defaultConfig {
        applicationId = "dev.monochange.app"
        minSdk = 26
        targetSdk = 36
        versionCode =
            (findProperty("leptoskit.versionCode") as String?
                ?: leptoskitVersion.getProperty("versionCode")).toInt()
        versionName = leptoskitVersion.getProperty("versionName")
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        val abiFile = file("../../.leptoskit/android/abi.txt")

        if (abiFile.exists()) {
            ndk { abiFilters += abiFile.readText().trim() }
        }
    }
    sourceSets["main"].apply {
        kotlin.directories += "../../.leptoskit/android/sources"
        assets.directories += "../../.leptoskit/android/mobile"
        res.directories += "../../.leptoskit/android/res"
        val generatedManifest = file("../../.leptoskit/android/AndroidManifest.xml")

        if (generatedManifest.exists()) {
            manifest.srcFile(generatedManifest)
        }
    }
    val cmakeLists = file("../../.leptoskit/android/CMakeLists.txt")

    if (cmakeLists.exists()) {
        externalNativeBuild { cmake { path = cmakeLists } }
    }

    signingConfigs {
        val keystore = providers.environmentVariable("LEPTOSKIT_ANDROID_KEYSTORE").orNull

        if (keystore != null) {
            create("release") {
                storeFile = file(keystore)
                storePassword = providers.environmentVariable("LEPTOSKIT_ANDROID_KEYSTORE_PASSWORD").orNull
                keyAlias = providers.environmentVariable("LEPTOSKIT_ANDROID_KEY_ALIAS").orNull
                keyPassword = providers.environmentVariable("LEPTOSKIT_ANDROID_KEY_PASSWORD").orNull
            }
        }
    }
    buildTypes {
        release {
            signingConfig = signingConfigs.findByName("release")
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "../../.leptoskit/android/proguard-rules.pro",
                "proguard-rules.pro",
            )
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

dependencies {
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.10.2")
    val pluginDeps = file("../../.leptoskit/android/plugin-dependencies.txt")

    if (pluginDeps.exists()) {
        pluginDeps.readLines().forEach { implementation(it) }
    }

    androidTestImplementation("androidx.test:runner:1.6.2")
    androidTestImplementation("androidx.test.ext:junit:1.2.1")
    androidTestImplementation("androidx.test.uiautomator:uiautomator:2.3.0")
}

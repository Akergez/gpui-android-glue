// The Java half of gpui-android-glue as an Android library: the classes
// gpui-mobile calls by name (from its example, with the IME of matterfast),
// what the Rust half calls, and the helper activities in the manifest, which
// is merged into the application's.
//
// The version is the release's: the tag CI builds, without the `v`; the
// Rust half (Cargo.toml) carries the same, and build-aux/check-version.sh
// holds the two together.

plugins {
    id("com.android.library") version "8.13.2"
    `maven-publish`
}

group = "com.akergez.gpui"
version = providers.gradleProperty("glueVersion")
    .orElse(
        providers.environmentVariable("GITHUB_REF_TYPE")
            .filter { it == "tag" }
            .flatMap { providers.environmentVariable("GITHUB_REF_NAME") }
            .map { it.removePrefix("v") }
    )
    .orElse("0.1.0-SNAPSHOT")
    .get()

android {
    namespace = "dev.gpui.glue"
    compileSdk = 36
    buildToolsVersion = "36.0.0"

    defaultConfig {
        minSdk = 26 // Vulkan 1.0 is mandatory from API 26
        // The classes are called from native code by name: an application
        // that shrinks must keep them.
        consumerProguardFiles("consumer-rules.pro")
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
    }

    publishing {
        singleVariant("release") {
            withSourcesJar()
        }
    }

    lint {
        abortOnError = false
    }
}

dependencies {
    // What the classes from gpui-mobile use; `api`, so that the application
    // needs no list of its own.
    api("androidx.core:core:1.12.0")
    api("androidx.core:core-splashscreen:1.0.1")
    api("androidx.biometric:biometric:1.1.0")
    api("androidx.media:media:1.7.1")
}

publishing {
    publications {
        create<MavenPublication>("release") {
            artifactId = "android-glue"
            afterEvaluate { from(components["release"]) }
            pom {
                name = "gpui-android-glue"
                description = "The Java half of gpui-android-glue: the Android side of gpui-kit applications"
                url = "https://github.com/Akergez/gpui-android-glue"
            }
        }
    }
    repositories {
        // GitHub Actions sets GITHUB_REPOSITORY ("owner/repo"); for a local publish pass
        // -PgithubRepository=owner/repo together with GITHUB_ACTOR and GITHUB_TOKEN.
        val githubRepository = providers.gradleProperty("githubRepository")
            .orElse(providers.environmentVariable("GITHUB_REPOSITORY"))
            .orNull
        if (githubRepository != null) {
            maven {
                name = "GitHubPackages"
                // GitHub Packages wants a lowercase owner in the Maven URL.
                url = uri("https://maven.pkg.github.com/${githubRepository.lowercase()}")
                credentials {
                    username = providers.environmentVariable("GITHUB_ACTOR").orNull
                    password = providers.environmentVariable("GITHUB_TOKEN").orNull
                }
            }
        }
    }
}

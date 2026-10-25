pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositories {
        google()
        mavenCentral()
    }
}

// The artifact is `com.akergez.gpui:android-glue`: with this name an
// application's `includeBuild` of this directory replaces the published
// artifact by this checkout.
rootProject.name = "android-glue"

pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
        // The Kotlin part of rustls-platform-verifier (group org.rustls). The crate keeps
        // it as a Maven repository in a branch on GitHub. Nothing else comes from here.
        maven("https://raw.githubusercontent.com/rustls/rustls-platform-verifier/maven-archive/android-release-support/maven/") {
            content { includeGroup("org.rustls") }
        }
    }
}

rootProject.name = "chord-android"
include(":app")

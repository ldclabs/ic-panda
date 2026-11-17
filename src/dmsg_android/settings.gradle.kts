pluginManagement {
    repositories { google(); mavenCentral(); gradlePluginPortal() }
}
dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
        maven("https://raw.githubusercontent.com/rustls/rustls-platform-verifier/maven-archive/android-release-support/maven") {
            content { includeGroup("org.rustls") }
        }
    }
}
rootProject.name = "dMsgAndroid"
include(":app")

pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

// TLS certificates are checked by the system, through a small Kotlin library
// that `rustls-platform-verifier` ships inside its own crate rather than on
// a Maven server. Cargo knows where that crate was unpacked.
fun rustlsPlatformVerifierMaven(): File {
    val metadata = providers.exec {
        workingDir = rootDir.parentFile
        commandLine(
            "cargo", "metadata", "--format-version", "1",
            "--filter-platform", "aarch64-linux-android",
        )
    }.standardOutput.asText.get()
    @Suppress("UNCHECKED_CAST")
    val packages = (groovy.json.JsonSlurper().parseText(metadata) as Map<String, Any>)["packages"]
        as List<Map<String, Any>>
    val manifest = packages.first { it["name"] == "rustls-platform-verifier-android" }["manifest_path"]
    return File(manifest as String).parentFile.resolve("maven")
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
        maven { url = uri(rustlsPlatformVerifierMaven()) }
    }
}

rootProject.name = "Matterfast"
include(":app")

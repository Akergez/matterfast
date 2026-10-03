# What CMake needs to know to build C dependencies (Opus) for the phone.
#
# Cargo's `cmake` crate describes the target as "Android, aarch64", which
# CMake's own Android support does not accept. The NDK ships a toolchain file
# that does the job, but it has to be told the ABI, and takes that as a CMake
# variable rather than from the environment — hence this wrapper. It is
# selected with `CMAKE_TOOLCHAIN_FILE_aarch64_linux_android`; see README.md.

set(ANDROID_ABI arm64-v8a)
set(ANDROID_PLATFORM android-26)
include("$ENV{ANDROID_NDK_HOME}/build/cmake/android.toolchain.cmake")

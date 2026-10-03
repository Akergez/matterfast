# Matterfast for Android

The application is the same Rust crate as on the desktop, built as a shared
object and loaded by a `NativeActivity`. The platform layer is
[gpui-mobile](https://github.com/longbridge/gpui-mobile); the Java classes in
`app/src/main/java/dev/gpui/mobile` and the Gradle project come from its
example, under the same licence terms.

## What is needed

- The Android SDK with a platform (`android-36`) and build tools, and an NDK
  (r27 is what this was built with).
- `cargo install cargo-ndk`, and `rustup target add aarch64-linux-android`.
- A JDK, 17 or later.
- CMake: Opus is built from source.

## Building

From the repository root, the library first:

```sh
export ANDROID_HOME=$HOME/Library/Android/sdk
export ANDROID_NDK_HOME=$ANDROID_HOME/ndk/27.0.12077973
# The Opus sources ask for a CMake older than current ones accept.
export CMAKE_POLICY_VERSION_MINIMUM=3.5
# ...and have to be told how to build for the phone at all.
export CMAKE_TOOLCHAIN_FILE_aarch64_linux_android=$PWD/android/ndk-toolchain.cmake

cargo ndk -t arm64-v8a --platform 26 -o android/app/src/main/jniLibs \
    build -p matterfast --lib --release
```

Build it `--release`: a debug library is over a gigabyte, and the renderer's
debug labels crash the emulator's Vulkan driver. The toolkit needs a recent
stable compiler (`cargo +stable` if that is not the default).

and then the package:

```sh
cd android
./gradlew assembleDebug
adb install -r app/build/outputs/apk/debug/app-debug.apk
```

The log goes to logcat under the tag `matterfast`.

## A package to hand out

`build-aux/build-android.sh` does both steps with the `dist` profile and
leaves a release package in `dist/`; it is what CI runs. The package's version
is the one in the workspace's `Cargo.toml`, or the one the script is given,
which also asks for the release key: see "Signing the Android package" in the
top-level README.

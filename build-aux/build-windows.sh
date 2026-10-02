#!/usr/bin/env bash
# Cross-compile Matterfast for Windows on Linux and pack it into a zip.
#
#   build-aux/build-windows.sh x86_64  [version]
#   build-aux/build-windows.sh aarch64 [version]     does not build yet
#
# aarch64 gets as far as aws-lc, whose NEON code (crypto/hrss/hrss.c) does
# not compile for aarch64-pc-windows-msvc with clang here, under either of
# its build systems. CI builds x86_64 only until that is solved.
#
# No Windows machine is involved. What stands in for one:
#
# * cargo-xwin, which downloads the MSVC runtime and the Windows SDK's
#   headers and libraries and points clang-cl and lld-link at them;
# * `fxc.exe`, Microsoft's shader compiler, run through wine. GPUI ships its
#   Direct3D shaders precompiled, and nothing else produces the same bytecode
#   (see vendor/README.md for the patch that lets its build script do this
#   off Windows). It is taken from Microsoft's own NuGet package at build
#   time and is not redistributed.
#
# Needs on PATH: cargo with the target installed, cargo-xwin, clang, lld,
# llvm, cmake, ninja, nasm, wine, curl, unzip, zip. CI uses the
# `messense/cargo-xwin` image and installs the rest.
#
# The result has no video: GStreamer is not bundled, so attachments do not
# play and a call has sound only (crates/matterfast/src/video_stub.rs).
set -Eeuo pipefail

root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd -- "$root"

arch=${1:?usage: ${0##*/} x86_64|aarch64 [version]}
case $arch in
  x86_64|aarch64) target=$arch-pc-windows-msvc ;;
  *) echo "unknown architecture: $arch" >&2; exit 2 ;;
esac
version=${2:-$(sed -n '/^\[workspace\.package\]/,/^\[/s/^version = "\(.*\)"/\1/p' Cargo.toml)}
profile=${MATTERFAST_PROFILE:-dist}
tools=${MATTERFAST_WINDOWS_TOOLS:-$root/target/windows-tools}

# The SDK package fxc.exe comes in. Pinned: the shaders are part of what
# ships, and their compiler should not change underneath a release.
sdk_version=10.0.26100.1742
sdk_bin=c/bin/10.0.26100.0

if [[ -z ${GPUI_FXC_PATH:-} ]]; then
  # wine runs the fxc.exe built for the machine it is on.
  case $(uname -m) in
    x86_64) fxc_arch=x64 ;;
    aarch64|arm64) fxc_arch=arm64 ;;
    *) echo "no fxc.exe for a $(uname -m) host" >&2; exit 1 ;;
  esac
  fxc_dir=$tools/fxc-$sdk_version-$fxc_arch
  if [[ ! -f $fxc_dir/fxc.exe ]]; then
    mkdir -p -- "$fxc_dir"
    curl --fail --silent --show-error --location --output "$tools/sdk.nupkg" \
      "https://www.nuget.org/api/v2/package/Microsoft.Windows.SDK.CPP/$sdk_version"
    # fxc.exe loads the compiler itself from the DLL beside it.
    unzip -q -j -o "$tools/sdk.nupkg" \
      "$sdk_bin/$fxc_arch/fxc.exe" "$sdk_bin/$fxc_arch/d3dcompiler_47.dll" -d "$fxc_dir"
    rm -f -- "$tools/sdk.nupkg"
  fi
  export GPUI_FXC_PATH=$fxc_dir/fxc.exe
fi
export WINEDEBUG=${WINEDEBUG:--all}
export WINEPREFIX=${WINEPREFIX:-$tools/wine}

# cargo-xwin's environment, taken as variables so it can be added to: the
# wrapper itself replaces CFLAGS and RUSTFLAGS instead of extending them.
eval "$(cargo xwin env --target "$target")"
# It also exports an empty RUSTFLAGS, and to cargo a RUSTFLAGS that is set at
# all, even to nothing, replaces the per-target flags below.
[[ -n ${RUSTFLAGS:-} ]] || unset RUSTFLAGS

if [[ $arch == x86_64 ]]; then
  # Opus keeps its SSE4.1 code in files it expects to be compiled with SSE4.1
  # switched on, and its CMake only adds the flag for compilers that are not
  # MSVC-like. clang-cl is, and unlike MSVC it refuses the intrinsics without
  # it. Every x86-64 processor Windows 10 still runs on has SSE4.1.
  export CFLAGS_x86_64_pc_windows_msvc+=" -msse4.1"
fi

if [[ $arch == aarch64 ]]; then
  # `ring` compiles its arm64 code with plain `clang` whatever compiler it is
  # told to use, and hands it the same flags. Plain clang takes clang-cl's
  # `/imsvc <dir>` for a file name. `-Xclang -isystem` says the same thing in
  # words both drivers accept. (cargo-xwin's own clang mode is no way out:
  # aws-lc does not compile for this target with anything but clang-cl.)
  for flags in CFLAGS_aarch64_pc_windows_msvc CXXFLAGS_aarch64_pc_windows_msvc; do
    export "$flags=$(sed -E 's#/imsvc ([^ ]+)#-Xclang -isystem -Xclang \1#g' <<<"${!flags}")"
  done
fi

# The C runtime linked in, so the .exe runs on a machine that has never seen
# the Visual C++ redistributable.
rustflags=CARGO_TARGET_$(tr 'a-z-' 'A-Z_' <<<"$target")_RUSTFLAGS
export "$rustflags=${!rustflags:-} -C target-feature=+crt-static"

cargo build --profile "$profile" --locked -p matterfast --target "$target"

name=matterfast-$version-windows-$arch
stage=$(mktemp -d)
trap 'rm -rf -- "$stage"' EXIT
mkdir -p -- "$stage/$name"
cp -- "${CARGO_TARGET_DIR:-target}/$target/$profile/matterfast.exe" "$stage/$name/"
cp -- LICENSE "$stage/$name/LICENSE.txt"
cp -- crates/matterfast/assets/fonts/Inter-LICENSE.txt \
  crates/matterfast/assets/fonts/Twemoji-LICENSE.md "$stage/$name/"

mkdir -p dist
rm -f -- "dist/$name.zip"
(cd -- "$stage" && zip -q -r -X "$root/dist/$name.zip" "$name")
(cd dist && sha256sum "$name.zip" > "$name.zip.sha256")

echo "dist/$name.zip"

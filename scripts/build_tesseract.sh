#!/usr/bin/env bash
# Build one self-contained Tesseract executable from pinned source tarballs.
#
# Leptonica, Tesseract, zlib, libpng, libjpeg-turbo, libtiff and libwebp are downloaded over
# HTTPS, checked against the SHA-256 pinned below, and built as static libraries into a
# private prefix. No libcurl, libarchive, OpenCL, ICU, OpenMP or training tools are used.
# The result is src-tauri/binaries/tesseract-<target-triple>[.exe], which depends only on
# system libraries.
#
# On Windows the exe embeds an application manifest that makes UTF-8 the process code page.
# Without it, narrow paths (C++ std::filesystem, and every char* Tesseract and Leptonica pass
# to the C library) are read in the ANSI code page, so a non-ASCII working directory or
# --tessdata-dir fails with "Cannot convert character sequence: Illegal byte sequence".
#
# Status: macOS arm64, Linux x64, and Windows x64 (MSYS2 MinGW64 shell) all build and run in
# .github/workflows/tesseract.yml. Set TESSERACT_TARGET_TRIPLE to name the output for a target
# other than the host, e.g. x86_64-pc-windows-msvc for the app's Windows target.
#
# Environment:
#   TESSERACT_BUILD_DIR  work directory (default: <repo>/tmp/tesseract-build)
#   TESSERACT_JOBS       parallel jobs (default: detected CPU count)
#
# To bump a version, change it below and replace its hash with the SHA-256 of a tarball you
# downloaded and checked yourself.
#
# Usage:
#   bash scripts/build_tesseract.sh

set -euo pipefail

TESSERACT_VERSION="5.5.3"
LEPTONICA_VERSION="1.87.0"
ZLIB_VERSION="1.3.1"
LIBPNG_VERSION="1.6.47"
LIBJPEG_TURBO_VERSION="3.1.0"
LIBTIFF_VERSION="4.7.0"
LIBWEBP_VERSION="1.5.0"

SHA256_TESSERACT="9218e62793116d42a9f6d14cd9348518b27f382096eea3d0f2d1a24616bb5884"
SHA256_LEPTONICA="c73363397f96eb1295602bf44d708a994ad42046c791bf03ea0505d829bdb6a7"
SHA256_ZLIB="9a93b2b7dfdac77ceba5a558a580e74667dd6fede4585b91eefb60f03b72df23"
SHA256_LIBPNG="631a4c58ea6c10c81f160c4b21fa8495b715d251698ebc2552077e8450f30454"
SHA256_LIBJPEG_TURBO="9564c72b1dfd1d6fe6274c5f95a8d989b59854575d4bbee44ade7bc17aa9bc93"
SHA256_LIBTIFF="67160e3457365ab96c5b3286a0903aa6e78bdc44c4bc737d2e486bcecb6ba976"
SHA256_LIBWEBP="7d6fab70cf844bf6769077bd5d7a74893f8ffd4dfb42861745750c63c2a5c92c"

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUILD_DIR="${TESSERACT_BUILD_DIR:-${REPO_ROOT}/tmp/tesseract-build}"
PREFIX="${BUILD_DIR}/prefix"
OUT_DIR="${REPO_ROOT}/src-tauri/binaries"

# Sets TRIPLE, EXE_SUFFIX, EXE_LINKER_FLAGS and the extra CMake arguments for the host.
detect_platform() {
    local os arch
    os="$(uname -s)"
    arch="$(uname -m)"
    TRIPLE="${TESSERACT_TARGET_TRIPLE:-$(rustc -vV | sed -n 's/^host: //p')}"
    EXE_SUFFIX=""
    EXE_LINKER_FLAGS=""
    MINGW=0
    PLATFORM_ARGS=()
    case "${os}-${arch}" in
        Darwin-arm64)
            PLATFORM_ARGS+=("-DCMAKE_OSX_DEPLOYMENT_TARGET=12.0" "-DCMAKE_OSX_ARCHITECTURES=arm64"
                "-DCMAKE_IGNORE_PREFIX_PATH=/opt/homebrew;/usr/local" "-DCMAKE_FIND_FRAMEWORK=NEVER")
            ;;
        Linux-x86_64)
            # Only glibc, libstdc++ and libgcc_s remain dynamic.
            PLATFORM_ARGS+=("-DCMAKE_POSITION_INDEPENDENT_CODE=ON")
            ;;
        MINGW*-x86_64 | MSYS*-x86_64)
            # Run from an MSYS2 MINGW64 shell with mingw-w64-x86_64-{gcc,cmake,ninja,nasm,
            # binutils}. Static libgcc/libstdc++ keep the .exe free of MinGW runtime DLLs.
            EXE_SUFFIX=".exe"
            EXE_LINKER_FLAGS="-static -static-libgcc -static-libstdc++"
            MINGW=1
            PLATFORM_ARGS+=("-G" "Ninja")
            ;;
        *)
            echo "build_tesseract: unsupported host ${os} ${arch}" >&2
            exit 1
            ;;
    esac
}

# Prints the SHA-256 of a file.
sha256_of() {
    if command -v sha256sum > /dev/null 2>&1; then
        sha256sum "$1" | cut -d ' ' -f 1
    else
        shasum -a 256 "$1" | cut -d ' ' -f 1
    fi
}

# fetch_source NAME URL SHA256 : downloads, verifies and unpacks into src/NAME.
fetch_source() {
    local name="$1" url="$2" expected="$3"
    local archive="${BUILD_DIR}/dl/${name}.tar.gz" actual
    mkdir -p "${BUILD_DIR}/dl" "${BUILD_DIR}/src/${name}"
    if [[ ! -f "${archive}" ]]; then
        echo "build_tesseract: downloading ${url}"
        curl --proto "=https" --proto-redir "=https" --tlsv1.2 --fail --silent --show-error \
            --retry 3 --location --output "${archive}" "${url}"
    fi
    actual="$(sha256_of "${archive}")"
    if [[ "${actual}" != "${expected}" ]]; then
        echo "build_tesseract: SHA-256 mismatch for ${name}" >&2
        echo "  expected ${expected}" >&2
        echo "  actual   ${actual}" >&2
        rm -f "${archive}"
        exit 1
    fi
    tar -xzf "${archive}" -C "${BUILD_DIR}/src/${name}" --strip-components=1
}

# cmake_build NAME [CMAKE ARGS...] : configures, builds and installs src/NAME into the prefix.
cmake_build() {
    local name="$1"
    shift
    echo "build_tesseract: building ${name}"
    cmake -S "${BUILD_DIR}/src/${name}" -B "${BUILD_DIR}/build/${name}" \
        -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="${PREFIX}" \
        -DCMAKE_PREFIX_PATH="${PREFIX}" -DCMAKE_INSTALL_LIBDIR=lib -DBUILD_SHARED_LIBS=OFF \
        -DCMAKE_POLICY_VERSION_MINIMUM=3.5 "${PLATFORM_ARGS[@]}" "$@" > "${BUILD_DIR}/${name}.log" 2>&1 \
        || { tail -30 "${BUILD_DIR}/${name}.log" >&2; exit 1; }
    cmake --build "${BUILD_DIR}/build/${name}" --parallel "${JOBS}" >> "${BUILD_DIR}/${name}.log" 2>&1 \
        || { tail -30 "${BUILD_DIR}/${name}.log" >&2; exit 1; }
    cmake --install "${BUILD_DIR}/build/${name}" >> "${BUILD_DIR}/${name}.log" 2>&1
}

# Removes shared libraries that zlib and libpng install, so only static ones can be linked.
drop_shared_libs() {
    find "${PREFIX}/lib" \( -name '*.dylib' -o -name '*.so' -o -name '*.so.*' -o -name '*.dll.a' \) -exec rm -f {} +
}

# Prints the runtime dependencies of the built binary for the log.
show_dependencies() {
    case "$(uname -s)" in
        Darwin) otool -L "$1" ;;
        Linux) ldd "$1" || true ;;
        *) objdump -p "$1" | grep 'DLL Name' || true ;;
    esac
}

# build_utf8_manifest : writes the application manifest that makes UTF-8 the process code page,
# compiles it into a resource object with windres, and prints the object's path.
#
# Windows 10 1903 and later honour activeCodePage. longPathAware keeps what the manifest MinGW
# adds by default already gives, so replacing it does not change how long paths behave.
build_utf8_manifest() {
    local dir="${BUILD_DIR}/manifest"
    mkdir -p "${dir}"
    cat > "${dir}/tesseract.manifest" <<EOF
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <assemblyIdentity type="win32" name="scriptorium.tesseract" version="${TESSERACT_VERSION}.0" processorArchitecture="amd64"/>
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="asInvoker" uiAccess="false"/>
      </requestedPrivileges>
    </security>
  </trustInfo>
  <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1">
    <application>
      <supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"/>
    </application>
  </compatibility>
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <activeCodePage xmlns="http://schemas.microsoft.com/SMI/2019/WindowsSettings">UTF-8</activeCodePage>
      <longPathAware xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">true</longPathAware>
    </windowsSettings>
  </application>
</assembly>
EOF
    # RT_MANIFEST is type 24, and an executable's manifest must use resource id 1.
    printf '1 24 "tesseract.manifest"\n' > "${dir}/tesseract.rc"
    command -v windres > /dev/null 2>&1 \
        || { echo "build_tesseract: windres is missing; install mingw-w64-x86_64-binutils" >&2; exit 1; }
    # windres opens the manifest relative to the working directory, so run it in its own.
    # Named like the toolchain's own file, so gcc picks it up through -B (see below).
    ( cd "${dir}" && windres -i tesseract.rc -o default-manifest.o )
    echo "${dir}/default-manifest.o"
}

START="$(date +%s)"
detect_platform
if command -v nproc > /dev/null 2>&1; then
    JOBS="${TESSERACT_JOBS:-$(nproc)}"
else
    JOBS="${TESSERACT_JOBS:-$(sysctl -n hw.ncpu)}"
fi

rm -rf "${BUILD_DIR}/src" "${BUILD_DIR}/build" "${PREFIX}"
mkdir -p "${PREFIX}"

# The Windows exe needs the UTF-8 manifest. GCC's MinGW specs link a file named
# default-manifest.o into every exe, found on the startfile search path. A second manifest
# beside ours is refused by some ld versions ("multiple non-default manifests"), and the ones
# that merge it leave which one Windows reads undefined. So the Tesseract link gets -B with a
# directory holding our own default-manifest.o, which gcc searches first. The exe then carries
# exactly one manifest, ours, and the shared toolchain is never changed. The path is converted
# because the native compiler cannot open MSYS paths (/d/a/...), and quoted for CMake, which
# pastes these flags into the link command as written.
MANIFEST_OBJ=""
if [[ "${MINGW}" == 1 ]]; then
    MANIFEST_OBJ="$(build_utf8_manifest)"
    MANIFEST_DIR="$(dirname "${MANIFEST_OBJ}")"
    if command -v cygpath > /dev/null 2>&1; then
        MANIFEST_DIR="$(cygpath -m "${MANIFEST_DIR}")"
    fi
    EXE_LINKER_FLAGS="${EXE_LINKER_FLAGS} \"-B${MANIFEST_DIR}/\""
fi

fetch_source zlib "https://github.com/madler/zlib/releases/download/v${ZLIB_VERSION}/zlib-${ZLIB_VERSION}.tar.gz" "${SHA256_ZLIB}"
fetch_source libpng "https://github.com/pnggroup/libpng/archive/refs/tags/v${LIBPNG_VERSION}.tar.gz" "${SHA256_LIBPNG}"
fetch_source libjpeg-turbo "https://github.com/libjpeg-turbo/libjpeg-turbo/releases/download/${LIBJPEG_TURBO_VERSION}/libjpeg-turbo-${LIBJPEG_TURBO_VERSION}.tar.gz" "${SHA256_LIBJPEG_TURBO}"
fetch_source libtiff "https://download.osgeo.org/libtiff/tiff-${LIBTIFF_VERSION}.tar.gz" "${SHA256_LIBTIFF}"
fetch_source libwebp "https://storage.googleapis.com/downloads.webmproject.org/releases/webp/libwebp-${LIBWEBP_VERSION}.tar.gz" "${SHA256_LIBWEBP}"
fetch_source leptonica "https://github.com/DanBloomberg/leptonica/releases/download/${LEPTONICA_VERSION}/leptonica-${LEPTONICA_VERSION}.tar.gz" "${SHA256_LEPTONICA}"
fetch_source tesseract "https://github.com/tesseract-ocr/tesseract/archive/refs/tags/${TESSERACT_VERSION}.tar.gz" "${SHA256_TESSERACT}"

cmake_build zlib
drop_shared_libs
cmake_build libpng -DPNG_SHARED=OFF -DPNG_STATIC=ON -DPNG_TESTS=OFF -DPNG_TOOLS=OFF -DPNG_FRAMEWORK=OFF
cmake_build libjpeg-turbo -DENABLE_SHARED=OFF -DENABLE_STATIC=ON -DWITH_TURBOJPEG=OFF -DWITH_JAVA=OFF
# Every library must land in ${PREFIX}/lib, or a later find step can fall back to a system copy.
[[ -f "${PREFIX}/lib/libjpeg.a" ]] || { echo "build_tesseract: libjpeg.a is not in ${PREFIX}/lib" >&2; exit 1; }
# zlib's CMake names its static library libz.a on Unix and libzlibstatic.a on MinGW.
ZLIB_A=""
for lib in libz.a libzlibstatic.a; do
    [[ -f "${PREFIX}/lib/${lib}" ]] && { ZLIB_A="${PREFIX}/lib/${lib}"; break; }
done
[[ -n "${ZLIB_A}" ]] || { echo "build_tesseract: no static zlib in ${PREFIX}/lib" >&2; exit 1; }
cmake_build libtiff -Dtiff-tools=OFF -Dtiff-tests=OFF -Dtiff-contrib=OFF -Dtiff-docs=OFF \
    -Dlzma=OFF -Dzstd=OFF -Djbig=OFF -Dlerc=OFF -Dwebp=OFF -Dlibdeflate=OFF -Dcxx=OFF
cmake_build libwebp -DWEBP_BUILD_ANIM_UTILS=OFF -DWEBP_BUILD_CWEBP=OFF -DWEBP_BUILD_DWEBP=OFF \
    -DWEBP_BUILD_GIF2WEBP=OFF -DWEBP_BUILD_IMG2WEBP=OFF -DWEBP_BUILD_VWEBP=OFF \
    -DWEBP_BUILD_WEBPINFO=OFF -DWEBP_BUILD_WEBPMUX=OFF -DWEBP_BUILD_EXTRAS=OFF
drop_shared_libs
# libtiff's CMake package file requires a CMath::CMath target that Leptonica never defines.
# Without it CMake falls back to its own FindTIFF module, which finds the static library.
rm -rf "${PREFIX}/lib/cmake/tiff"
cmake_build leptonica -DSW_BUILD=OFF -DENABLE_GIF=OFF -DENABLE_OPENJPEG=OFF -DENABLE_ZLIB=ON \
    -DENABLE_PNG=ON -DENABLE_JPEG=ON -DENABLE_TIFF=ON -DENABLE_WEBP=ON
# Leptonica's generated package file leaves its WebP dependency condition empty, so the
# WebP::webp target it exports is never defined. Switch the condition on.
sed -i.bak '/^if ()$/{N;/WebP/s/^if ()/if (1)/;}' "${PREFIX}/lib/cmake/leptonica/LeptonicaConfig.cmake"
# GNU ld reads static archives once, left to right. Leptonica's link line puts libjpeg.a before
# libtiff.a, which needs it (libtiff's own CMake package, which carried that dependency, is
# deleted above). Repeating libjpeg and zlib at the end of the line resolves libtiff's references.
# libjpeg-turbo 3 makes libtiff build 12-bit JPEG support (tif_jpeg_12.c) unconditionally, so its
# jpeg12_* references must resolve too; libtiff's jpeg12 option has no effect in that mode.
# CMAKE_CXX_STANDARD_LIBRARIES is pasted into the link command as written, so each path is
# double-quoted to stay one argument even if the prefix contains spaces.
# Apple's linker does not need this, and an extra archive at the end is harmless there.
JPEG_A="${PREFIX}/lib/libjpeg.a"
# Under MSYS2 the native MinGW linker cannot open MSYS paths (/d/a/...), so pass Windows paths.
if command -v cygpath > /dev/null 2>&1; then
    JPEG_A="$(cygpath -m "${JPEG_A}")"
    ZLIB_A="$(cygpath -m "${ZLIB_A}")"
fi
TAIL_LIBS="\"${JPEG_A}\" \"${ZLIB_A}\""
cmake_build tesseract -DSW_BUILD=OFF -DDISABLE_CURL=ON -DDISABLE_ARCHIVE=ON -DOPENMP_BUILD=OFF \
    -DUSE_SYSTEM_ICU=OFF -DBUILD_TRAINING_TOOLS=OFF -DBUILD_TESTS=OFF -DGRAPHICS_DISABLED=ON \
    -DOPENCL=OFF -DDISABLE_TIFF=OFF \
    -DLeptonica_DIR="${PREFIX}/lib/cmake/leptonica" \
    -DCMAKE_CXX_STANDARD_LIBRARIES="${TAIL_LIBS}" \
    -DCMAKE_EXE_LINKER_FLAGS="${EXE_LINKER_FLAGS}"

mkdir -p "${OUT_DIR}"
OUT="${OUT_DIR}/tesseract-${TRIPLE}${EXE_SUFFIX}"
cp "${PREFIX}/bin/tesseract${EXE_SUFFIX}" "${OUT}"
echo "build_tesseract: unstripped size $(wc -c < "${OUT}") bytes"
if [[ "$(uname -s)" == "Darwin" ]]; then
    strip -x "${OUT}"
else
    strip "${OUT}"
fi
echo "build_tesseract: stripped size $(wc -c < "${OUT}") bytes"
show_dependencies "${OUT}"
# The binary must not load any of the libraries this script builds statically. A match means a
# system copy was linked instead of the private one.
# otool's first line names the binary itself, so lines ending in ':' are skipped.
if show_dependencies "${OUT}" | grep -v ':$' | grep -iE 'lept|tesseract|libpng|jpeg|tiff|webp|libz[.-]|zlib'; then
    echo "build_tesseract: ${OUT} depends on a shared copy of a bundled library" >&2
    exit 1
fi
# The UTF-8 manifest must have reached the shipped exe, after the strip. It is plain XML in the
# resource section, so strings finds it.
if [[ -n "${MANIFEST_OBJ}" ]]; then
    strings -n 8 "${OUT}" | grep -q 'activeCodePage' \
        || { echo "build_tesseract: ${OUT} has no activeCodePage manifest" >&2; exit 1; }
fi
echo "build_tesseract: wrote ${OUT} in $(($(date +%s) - START))s"

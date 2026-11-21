#!/usr/bin/env bash
# Download the pinned Tesseract binary and English language data for the host platform.
#
# The binary comes from this repo's `tesseract-<version>-r<n>` release, as a bare
# `tesseract-<target-triple>[.exe]` asset. It is installed as
# `src-tauri/binaries/scriptorium-tesseract-<target-triple>[.exe]`, so a distro
# `tesseract-ocr` package's `/usr/bin/tesseract` cannot clash with the bundle. The
# language data comes from tesseract-ocr/tessdata_fast at the pinned tag, and lands in
# src-tauri/tessdata/. Both SHA-256 values are checked against the values pinned below
# before anything is installed. The script exits at once when both files are already
# there for the pinned versions.
#
# Set TESSERACT_ASSET_CACHE to a directory to keep the downloaded files between runs.
# To bump Tesseract: change TESSERACT_RELEASE, then replace each binary hash with the
# SHA-256 of the matching asset from that release's SHA256SUMS. To bump the language
# data: change TESSDATA_RELEASE and SHA256_TESSDATA_ENG together.
#
# Usage:
#   bash scripts/fetch_tesseract.sh

set -euo pipefail

TESSERACT_RELEASE="tesseract-5.5.3-r2"
TESSDATA_RELEASE="4.1.0"

SHA256_MAC_ARM64="5d5c574bf64687b01df27d5ad8c19ee67fecc6de4983c31b412c51dac1d8e035"
SHA256_LINUX_X64="05c22fe23b8bb61e7e70b0e12fb96b953e8bb32f83f8e0603d60c43e7d2f6a3d"
SHA256_WIN_X64="ade58185dc75a73d6b8190247c4505a82b43852e906b28c26fe9363c4b482575"

SHA256_TESSDATA_ENG="7d4322bd2a7749724879683fc3912cb542f19906c83bcc1a52132556427170b2"

RELEASE_BASE="https://github.com/kgforais1/scriptorium/releases/download/${TESSERACT_RELEASE}"
TESSDATA_URL="https://github.com/tesseract-ocr/tessdata_fast/raw/${TESSDATA_RELEASE}/eng.traineddata"

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN_DEST="${REPO_ROOT}/src-tauri/binaries"
TESSDATA_DEST="${REPO_ROOT}/src-tauri/tessdata"

# Sets PLATFORM, ASSET and EXPECTED_SHA for the host.
detect_platform() {
    local os arch
    os="$(uname -s)"
    arch="$(uname -m)"
    case "${os}-${arch}" in
        Darwin-arm64)
            PLATFORM="mac-arm64"
            ASSET="tesseract-aarch64-apple-darwin"
            EXPECTED_SHA="${SHA256_MAC_ARM64}"
            ;;
        Darwin-x86_64)
            echo "fetch_tesseract: there is no pinned tesseract build for macOS x86_64." >&2
            echo "  The app targets Apple silicon only; build the sidecar with scripts/build_tesseract.sh." >&2
            exit 1
            ;;
        Linux-x86_64)
            PLATFORM="linux-x64"
            ASSET="tesseract-x86_64-unknown-linux-gnu"
            EXPECTED_SHA="${SHA256_LINUX_X64}"
            ;;
        MINGW*-x86_64 | MSYS*-x86_64 | CYGWIN*-x86_64)
            PLATFORM="win-x64"
            ASSET="tesseract-x86_64-pc-windows-msvc.exe"
            EXPECTED_SHA="${SHA256_WIN_X64}"
            ;;
        *)
            echo "fetch_tesseract: no pinned tesseract build for ${os} ${arch}" >&2
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

# Copies URL to DEST, from the cached copy when there is one, and checks its SHA-256.
# A file that fails the check is deleted, whether cached or just downloaded, so that a
# corrupt copy cannot fail every later run.
fetch_verified() {
    local url="$1" cached="$2" dest="$3" expected="$4" label="$5"
    local actual
    if [[ -n "${cached}" && -f "${cached}" ]]; then
        cp "${cached}" "${dest}"
    else
        echo "fetch_tesseract: downloading ${url}"
        curl --proto "=https" --proto-redir "=https" --tlsv1.2 --fail --silent --show-error \
            --retry 3 --location --output "${dest}" "${url}"
    fi
    actual="$(sha256_of "${dest}")"
    if [[ "${actual}" != "${expected}" ]]; then
        echo "fetch_tesseract: SHA-256 mismatch for ${label}" >&2
        echo "  expected ${expected}" >&2
        echo "  actual   ${actual}" >&2
        rm -f "${dest}"
        if [[ -n "${cached}" ]]; then
            rm -f "${cached}"
        fi
        exit 1
    fi
    if [[ -n "${cached}" && ! -f "${cached}" ]]; then
        cp "${dest}" "${cached}"
    fi
}

detect_platform
INSTALLED="scriptorium-${ASSET}"

if [[ -f "${BIN_DEST}/${INSTALLED}" && -f "${TESSDATA_DEST}/eng.traineddata" \
    && "$(cat "${BIN_DEST}/.version" 2> /dev/null || true)" == "${TESSERACT_RELEASE} ${PLATFORM}" \
    && "$(cat "${TESSDATA_DEST}/.version" 2> /dev/null || true)" == "${TESSDATA_RELEASE}" ]]; then
    echo "fetch_tesseract: ${INSTALLED} and eng.traineddata already present for ${TESSERACT_RELEASE}"
    exit 0
fi

WORK="$(mktemp -d)"

# Each file is staged beside its destination and renamed into place, so a reader never sees
# a partly written file. The staged names are cleaned up if this run fails partway.
BIN_STAGED="${BIN_DEST}/.${INSTALLED}.partial.$$"
TESSDATA_STAGED="${TESSDATA_DEST}/.eng.traineddata.partial.$$"
trap 'rm -rf "${WORK}"; rm -f "${BIN_STAGED}" "${TESSDATA_STAGED}"' EXIT

BIN_TMP="${WORK}/${ASSET}"
TESSDATA_TMP="${WORK}/eng.traineddata"

BIN_CACHED=""
TESSDATA_CACHED=""
if [[ -n "${TESSERACT_ASSET_CACHE:-}" ]]; then
    mkdir -p "${TESSERACT_ASSET_CACHE}"
    BIN_CACHED="${TESSERACT_ASSET_CACHE}/${TESSERACT_RELEASE}-${ASSET}"
    TESSDATA_CACHED="${TESSERACT_ASSET_CACHE}/tessdata_fast-${TESSDATA_RELEASE}-eng.traineddata"
fi

fetch_verified "${RELEASE_BASE}/${ASSET}" "${BIN_CACHED}" "${BIN_TMP}" "${EXPECTED_SHA}" "${ASSET}"
fetch_verified "${TESSDATA_URL}" "${TESSDATA_CACHED}" "${TESSDATA_TMP}" "${SHA256_TESSDATA_ENG}" "eng.traineddata"

mkdir -p "${BIN_DEST}" "${TESSDATA_DEST}"

# Drop the markers first: while they are gone the next run re-downloads, so it cannot skip
# an install this one did not finish.
rm -f "${BIN_DEST}/.version" "${TESSDATA_DEST}/.version"

cp "${BIN_TMP}" "${BIN_STAGED}"
chmod +x "${BIN_STAGED}"
cp "${TESSDATA_TMP}" "${TESSDATA_STAGED}"
mv -f "${BIN_STAGED}" "${BIN_DEST}/${INSTALLED}"
mv -f "${TESSDATA_STAGED}" "${TESSDATA_DEST}/eng.traineddata"

# Markers last, once both files are in place.
echo "${TESSERACT_RELEASE} ${PLATFORM}" > "${BIN_DEST}/.version"
echo "${TESSDATA_RELEASE}" > "${TESSDATA_DEST}/.version"
echo "fetch_tesseract: installed ${BIN_DEST}/${INSTALLED} and ${TESSDATA_DEST}/eng.traineddata"

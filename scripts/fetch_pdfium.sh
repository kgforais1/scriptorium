#!/usr/bin/env bash
# Download the pinned pdfium library for the host platform into src-tauri/pdfium/.
#
# The archive comes from bblanchon/pdfium-binaries (the build without V8 and XFA). Its
# SHA-256 is checked against the value pinned below before anything is unpacked. Only the
# shared library is kept, in one flat directory, with the pinned release recorded beside it.
# The script exits at once when that library is already there for the pinned release.
#
# Set PDFIUM_ARCHIVE_CACHE to a directory to keep the downloaded archive between runs.
# To bump pdfium: change PDFIUM_RELEASE, then replace each hash with the SHA-256 of the
# matching archive that you downloaded yourself.
#
# Usage:
#   bash scripts/fetch_pdfium.sh

set -euo pipefail

PDFIUM_RELEASE="chromium/8138"

SHA256_MAC_ARM64="21a1567b397b571f4f77f21ca91eac7b892774219c3c715a5127a20950ccb18b"
SHA256_MAC_X64="72662ac8eff7bf5381ef82ceebd0a0ad5bddb051b303981115114e6972fd201c"
SHA256_WIN_X64="8d48973e3eb472dda0f5f2435e595ac043f9f2219dd71883392f549445afa8a5"
SHA256_LINUX_X64="d6b1e28950a0392590d79bbdbb7eb19686872b688f8a36f170acb19e41473113"

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="${REPO_ROOT}/src-tauri/pdfium"

# Sets PLATFORM, EXPECTED_SHA, LIB_IN_ARCHIVE and LIB_NAME for the host.
detect_platform() {
    local os arch
    os="$(uname -s)"
    arch="$(uname -m)"
    case "${os}-${arch}" in
        Darwin-arm64)
            PLATFORM="mac-arm64"
            EXPECTED_SHA="${SHA256_MAC_ARM64}"
            LIB_IN_ARCHIVE="lib/libpdfium.dylib"
            ;;
        Darwin-x86_64)
            PLATFORM="mac-x64"
            EXPECTED_SHA="${SHA256_MAC_X64}"
            LIB_IN_ARCHIVE="lib/libpdfium.dylib"
            ;;
        Linux-x86_64)
            PLATFORM="linux-x64"
            EXPECTED_SHA="${SHA256_LINUX_X64}"
            LIB_IN_ARCHIVE="lib/libpdfium.so"
            ;;
        MINGW*-x86_64 | MSYS*-x86_64 | CYGWIN*-x86_64)
            PLATFORM="win-x64"
            EXPECTED_SHA="${SHA256_WIN_X64}"
            LIB_IN_ARCHIVE="bin/pdfium.dll"
            ;;
        *)
            echo "fetch_pdfium: no pinned pdfium build for ${os} ${arch}" >&2
            exit 1
            ;;
    esac
    LIB_NAME="$(basename "${LIB_IN_ARCHIVE}")"
}

# Prints the SHA-256 of a file.
sha256_of() {
    if command -v sha256sum > /dev/null 2>&1; then
        sha256sum "$1" | cut -d ' ' -f 1
    else
        shasum -a 256 "$1" | cut -d ' ' -f 1
    fi
}

detect_platform

if [[ -f "${DEST}/${LIB_NAME}" && "$(cat "${DEST}/.version" 2> /dev/null || true)" == "${PDFIUM_RELEASE} ${PLATFORM}" ]]; then
    echo "fetch_pdfium: ${LIB_NAME} already present for ${PDFIUM_RELEASE}"
    exit 0
fi

WORK="$(mktemp -d)"
trap 'rm -rf "${WORK}"' EXIT

ARCHIVE="${WORK}/pdfium-${PLATFORM}.tgz"
CACHED=""
if [[ -n "${PDFIUM_ARCHIVE_CACHE:-}" ]]; then
    mkdir -p "${PDFIUM_ARCHIVE_CACHE}"
    CACHED="${PDFIUM_ARCHIVE_CACHE}/${PDFIUM_RELEASE//\//-}-pdfium-${PLATFORM}.tgz"
fi

if [[ -n "${CACHED}" && -f "${CACHED}" ]]; then
    cp "${CACHED}" "${ARCHIVE}"
else
    URL="https://github.com/bblanchon/pdfium-binaries/releases/download/${PDFIUM_RELEASE//\//%2F}/pdfium-${PLATFORM}.tgz"
    echo "fetch_pdfium: downloading ${URL}"
    curl --proto "=https" --proto-redir "=https" --tlsv1.2 --fail --silent --show-error --location --output "${ARCHIVE}" "${URL}"
fi

ACTUAL_SHA="$(sha256_of "${ARCHIVE}")"
if [[ "${ACTUAL_SHA}" != "${EXPECTED_SHA}" ]]; then
    echo "fetch_pdfium: SHA-256 mismatch for pdfium-${PLATFORM}.tgz" >&2
    echo "  expected ${EXPECTED_SHA}" >&2
    echo "  actual   ${ACTUAL_SHA}" >&2
    exit 1
fi
[[ -n "${CACHED}" && ! -f "${CACHED}" ]] && cp "${ARCHIVE}" "${CACHED}"

tar -xzf "${ARCHIVE}" -C "${WORK}" "${LIB_IN_ARCHIVE}"
mkdir -p "${DEST}"
cp "${WORK}/${LIB_IN_ARCHIVE}" "${DEST}/${LIB_NAME}"
echo "${PDFIUM_RELEASE} ${PLATFORM}" > "${DEST}/.version"
echo "fetch_pdfium: installed ${DEST}/${LIB_NAME}"

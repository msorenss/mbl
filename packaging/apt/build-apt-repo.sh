#!/usr/bin/env bash
# Generate a signed Debian APT repository layout (`dists/stable/main/binary-{arm64,amd64}`) from built `.deb` artifacts (M5.12).
set -euo pipefail

DEB_INPUT_DIR="${1:-./dist-debs}"
REPO_OUT_DIR="${2:-./apt-repo}"
GPG_KEY_ID="${GPG_KEY_ID:-}"

for arch in arm64 amd64; do
    mkdir -p "${REPO_OUT_DIR}/pool/main/m/mbl"
    mkdir -p "${REPO_OUT_DIR}/dists/stable/main/binary-${arch}"
done

if compgen -G "${DEB_INPUT_DIR}/*.deb" > /dev/null; then
    cp -v "${DEB_INPUT_DIR}"/*.deb "${REPO_OUT_DIR}/pool/main/m/mbl/"
fi

pushd "${REPO_OUT_DIR}" > /dev/null
for arch in arm64 amd64; do
    dpkg-scanpackages --arch "${arch}" pool/ > "dists/stable/main/binary-${arch}/Packages"
    gzip -9c "dists/stable/main/binary-${arch}/Packages" > "dists/stable/main/binary-${arch}/Packages.gz"
done

cat > dists/stable/Release <<EOF
Origin: MBL Media Server
Label: MBL
Suite: stable
Codename: stable
Architectures: arm64 amd64
Components: main
Description: MBL Media Server official APT repository
Date: $(LC_ALL=C date -u +"%a, %d %b %Y %H:%M:%S UTC")
EOF

if command -v apt-ftparchive > /dev/null; then
    apt-ftparchive release dists/stable >> dists/stable/Release
fi

if [ -n "${GPG_KEY_ID}" ]; then
    gpg --default-key "${GPG_KEY_ID}" -abs -o dists/stable/Release.gpg dists/stable/Release
    gpg --default-key "${GPG_KEY_ID}" --clearsign -o dists/stable/InRelease dists/stable/Release
    gpg --export "${GPG_KEY_ID}" > mbl-archive-keyring.gpg
fi
popd > /dev/null

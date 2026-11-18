#!/usr/bin/env bash
# Bangun APK universal lalu pecah jadi tiga: 32-bit, 64-bit, universal.
#
# Kunci tanda tangan dicari di tiga tempat, berurutan:
#   1. env ANDROID_KEYSTORE + ANDROID_KEY_PASS
#   2. ~/.config/lan-drop/keystore.env (dibuat mesin ini)
#   3. tidak ada kunci -> APK tetap dihasilkan tapi tidak ditandatangani
#
# Hasil: dist/android/lan-drop-<versi>-{32bit,64bit,universal}.apk
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

# rustup/cargo tidak selalu ada di PATH shell non-login (cron, CI shell).
export PATH="$HOME/.cargo/bin:$PATH"

if [[ -f "$HOME/.config/lan-drop/keystore.env" ]]; then
  # shellcheck disable=SC1091
  source "$HOME/.config/lan-drop/keystore.env"
  export ANDROID_KEYSTORE="$KEYSTORE" ANDROID_KEY_PASS="$STORE_PASS" ANDROID_KEY_ALIAS="$KEY_ALIAS"
fi

: "${ANDROID_HOME:=${ANDROID_SDK_ROOT:-/opt/android-sdk}}"
export ANDROID_HOME ANDROID_SDK_ROOT="$ANDROID_HOME"
if [[ -z "${NDK_HOME:-}" && -d "$ANDROID_HOME/ndk" ]]; then
  export NDK_HOME="$ANDROID_HOME/ndk/$(ls "$ANDROID_HOME/ndk" | sort -V | tail -1)"
fi

version="$(node -p "require('./rust/app/tauri.conf.json').version")"
out="dist/android"

echo "== bangun APK universal"
( cd rust/app && npx tauri android build --apk )

apk="rust/app/gen/android/app/build/outputs/apk/universal/release/app-universal-release-unsigned.apk"
if [[ ! -f "$apk" ]]; then
  apk="$(find rust/app/gen/android/app/build/outputs/apk/universal/release -maxdepth 1 -name '*unsigned.apk' | head -1)"
fi
[[ -f "$apk" ]] || { echo "APK universal tidak ditemukan"; exit 1; }

echo "== pecah dan tandatangani"
python3 scripts/android-apk-split.py --apk "$apk" --version "$version" --out "$out"

ls -1 "$out"

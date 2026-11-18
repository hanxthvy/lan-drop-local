#!/usr/bin/env python3
"""Pecah APK universal jadi tiga: 32-bit, 64-bit, dan universal.

Gradle hanya bisa memecah per-ABI (satu berkas per arsitektur). Yang diminta di
sini adalah dua berkas per lebar bit: 32-bit berisi armeabi-v7a + x86, 64-bit
berisi arm64-v8a + x86_64. Karena isinya cuma direktori `lib/<abi>/`, APK
universal bisa dipangkas tanpa menyentuh manifest, dex, atau resource.

Aturan yang dijaga:
- kompresi tiap entri dipertahankan (resources.arsc dan .so harus STORED,
  kalau tidak Android menolak di targetSdk >= 30)
- setelah ditulis ulang, APK di-zipalign lalu ditandatangani; tanda tangan
  gradle yang lama gugur karena byte-nya berubah
"""

from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import sys
import zipfile

ABI_GROUPS = {
    "32bit": {"armeabi-v7a", "x86"},
    "64bit": {"arm64-v8a", "x86_64"},
}
ALL_ABIS = set().union(*ABI_GROUPS.values())


def build_variant(src: zipfile.ZipFile, dst_path: str, keep: set[str]) -> int:
    """Tulis ulang APK hanya dengan lib/<abi>/ yang ada di `keep`."""
    dropped = 0
    with zipfile.ZipFile(dst_path, "w") as out:
        for item in src.infolist():
            parts = item.filename.split("/")
            if parts[0] == "lib" and len(parts) > 2 and parts[1] not in keep:
                dropped += 1
                continue
            new = zipfile.ZipInfo(item.filename, date_time=item.date_time)
            new.compress_type = item.compress_type
            new.external_attr = item.external_attr
            new.create_system = item.create_system
            with src.open(item) as reader, out.open(new, "w") as writer:
                shutil.copyfileobj(reader, writer)
    return dropped


def tool(name: str) -> str:
    found = shutil.which(name)
    if found:
        return found
    for root in (os.environ.get("ANDROID_HOME"), os.environ.get("ANDROID_SDK_ROOT")):
        if not root:
            continue
        build_tools = os.path.join(root, "build-tools")
        if os.path.isdir(build_tools):
            for version in sorted(os.listdir(build_tools), reverse=True):
                candidate = os.path.join(build_tools, version, name)
                if os.path.isfile(candidate):
                    return candidate
    raise SystemExit(f"{name} tidak ditemukan (pasang build-tools Android atau tambahkan ke PATH)")


def align_and_sign(path: str, keystore: str | None, storepass: str | None, alias: str) -> str:
    aligned = path + ".aligned"
    subprocess.run([tool("zipalign"), "-f", "4", path, aligned], check=True)
    os.replace(aligned, path)
    if not keystore:
        return path
    subprocess.run(
        [
            tool("apksigner"), "sign",
            "--ks", keystore,
            "--ks-key-alias", alias,
            "--ks-pass", f"pass:{storepass}",
            "--key-pass", f"pass:{storepass}",
            "--out", path + ".signed", path,
        ],
        check=True,
    )
    os.replace(path + ".signed", path)
    # Berkas .idsig hanya untuk signature v4 (incremental install); bukan bagian APK.
    if os.path.exists(path + ".signed.idsig"):
        os.remove(path + ".signed.idsig")
    subprocess.run([tool("apksigner"), "verify", path], check=True)
    return path


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--apk", required=True, help="APK universal hasil gradle")
    ap.add_argument("--version", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--keystore", default=os.environ.get("ANDROID_KEYSTORE"))
    ap.add_argument("--storepass", default=os.environ.get("ANDROID_KEY_PASS"))
    ap.add_argument("--alias", default=os.environ.get("ANDROID_KEY_ALIAS", "landrop"))
    args = ap.parse_args()

    if not os.path.isfile(args.apk):
        raise SystemExit(f"APK tidak ada: {args.apk}")
    os.makedirs(args.out, exist_ok=True)

    results = []
    with zipfile.ZipFile(args.apk) as src:
        present = {n.split("/")[1] for n in src.namelist() if n.startswith("lib/") and n.count("/") >= 2}
        missing = ALL_ABIS - present
        if missing:
            raise SystemExit(f"APK universal tidak memuat ABI: {', '.join(sorted(missing))}")

        for label, keep in ABI_GROUPS.items():
            path = os.path.join(args.out, f"lan-drop-{args.version}-{label}.apk")
            build_variant(src, path, keep)
            align_and_sign(path, args.keystore, args.storepass, args.alias)
            results.append((label, path, sorted(keep)))

    universal = os.path.join(args.out, f"lan-drop-{args.version}-universal.apk")
    shutil.copyfile(args.apk, universal)
    align_and_sign(universal, args.keystore, args.storepass, args.alias)
    results.append(("universal", universal, sorted(ALL_ABIS)))

    for label, path, abis in results:
        size = os.path.getsize(path)
        print(f"{label:<9} {size/1024/1024:6.1f} MB  {os.path.basename(path)}  [{', '.join(abis)}]")
    return 0


if __name__ == "__main__":
    sys.exit(main())

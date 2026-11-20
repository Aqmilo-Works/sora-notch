"""Recreate Sora's embedded x64 converter bundles from pinned upstream releases.

Usage: python tools/prepare-converter.py [directory for cached upstream archives]
The generated data/converter files are included in source releases for offline builds.
"""
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import urllib.request
import zipfile

DEST = Path(__file__).resolve().parents[1] / "data" / "converter"
FF = "9.0.2"
IM = "7.1.2-35"
VC_NAME = "Microsoft.VC.14.44.17.14.CRT.Redist.X64.base.vsix"
VC_SHA = "4aaf54db0bfc9435f7c3660e1a00237a4b556042bfeea64bde44c2e0194e6ee5"
VC_URL = f"https://download.visualstudio.microsoft.com/download/pr/45d3b8dd-bced-4b37-9974-142f748d710c/{VC_SHA}/{VC_NAME}"
BUNDLES = [
    ("ffmpeg", FF, "ffmpeg.zip",
     f"https://github.com/GyanD/codexffmpeg/releases/download/{FF}/ffmpeg-{FF}-essentials_build.zip",
     "60f467265b1e312373dbcd92200c2618a74850f98d3d078e94296bb3fa2047ba",
     {"ffmpeg.exe": f"ffmpeg-{FF}-essentials_build/bin/ffmpeg.exe",
      "LICENSE.txt": f"ffmpeg-{FF}-essentials_build/LICENSE",
      "README.txt": f"ffmpeg-{FF}-essentials_build/README.txt"},
     f"https://ffmpeg.org/releases/ffmpeg-{FF}.tar.xz\nBuild provider: https://www.gyan.dev/ffmpeg/builds/"),
    ("magick", IM, "magick.7z",
     f"https://github.com/ImageMagick/ImageMagick/releases/download/{IM}/ImageMagick-{IM}-portable-Q16-x64.7z",
     "e7c629f3530ccb9d4918a15f0e928254feda28e84a7fa794b25873d152c075c1",
     {name: name for name in ["magick.exe", "LICENSE.txt", "NOTICE.txt"]},
     f"https://github.com/ImageMagick/ImageMagick/tree/{IM}\nBuild recipes: https://github.com/ImageMagick/ImageMagick-Windows"),
]

def add(archive, name, data):
    info = zipfile.ZipInfo(name)
    archive.writestr(info, data, compress_type=zipfile.ZIP_DEFLATED, compresslevel=9)

def verified_file(cache, filename, url, expected):
    path = cache / filename
    if not path.exists():
        urllib.request.urlretrieve(url, path)
    if hashlib.sha256(path.read_bytes()).hexdigest() != expected:
        raise ValueError(f"SHA-256 mismatch for {filename}")
    return path

def main():
    DEST.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory() as tmp:
        stage = Path(tmp)
        cache = Path(sys.argv[1]) if len(sys.argv) > 1 else stage
        for engine, version, filename, url, expected, files, sources in BUNDLES:
            src = verified_file(cache, filename, url, expected)
            unpack = stage / engine
            unpack.mkdir()
            subprocess.run(["tar.exe", "-xf", str(src.resolve()), "-C", str(unpack), *files.values()], check=True)
            path = stage / (engine + "-bundle.zip")
            entries = {}
            with zipfile.ZipFile(path, "w") as archive:
                for name, original in files.items():
                    data = (unpack / original).read_bytes()
                    entries[name] = hashlib.sha256(data).hexdigest()
                    add(archive, name, data)
                notice = f"Bundled unchanged executable: {engine} {version} (Windows x64)\nBinary distribution: {url}\nArchive SHA-256: {expected}\nSource: {sources}\n".encode()
                if engine == "magick":
                    # The portable x64 release imports Microsoft's OpenMP DLL at process load.
                    redist = verified_file(cache, VC_NAME, VC_URL, VC_SHA)
                    with zipfile.ZipFile(redist) as vc:
                        dll = vc.read("Contents/VC/Redist/MSVC/14.44.35112/x64/Microsoft.VC143.OpenMP/vcomp140.dll")
                    add(archive, "vcomp140.dll", dll)
                    entries["vcomp140.dll"] = hashlib.sha256(dll).hexdigest()
                    notice += f"Microsoft Visual C++ OpenMP runtime, Copyright Microsoft Corporation.\nRedistributable package: {VC_URL}\nTerms: https://visualstudio.microsoft.com/license-terms/vs2022-cruntime/\nRedistributable list: https://learn.microsoft.com/visualstudio/releases/2022/redistribution\n".encode()
                add(archive, "SOURCES.txt", notice)
                entries["SOURCES.txt"] = hashlib.sha256(notice).hexdigest()
                if engine == "magick":
                    policy = (Path(__file__).with_name("magick-policy.xml")).read_bytes()
                    add(archive, "policy.xml", policy)
                    entries["policy.xml"] = hashlib.sha256(policy).hexdigest()
            shutil.copyfile(path, DEST / (engine + ".zip"))
            (DEST / (engine + ".json")).write_text(json.dumps({"version":version,"files":entries},indent=2)+"\n", encoding="utf-8")
            print(engine, version, path.stat().st_size)

if __name__ == "__main__":
    main()

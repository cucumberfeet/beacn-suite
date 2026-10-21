#!/usr/bin/env python3
"""Extract BEACN.exe and a MixImages theme pack from YOUR OWN copy of the official
BEACN Windows installer ("BEACN Setup V1.4.2.12.exe"), without running it.

The installer is an Advanced Installer bootstrapper whose payload is an embedded 7-Zip
archive (FILES.7z) running to the end of the file, with its first 512 bytes XOR'd with 0xFF.
This script restores that archive and extracts only the files the kit needs.

Usage: extract-beacn-installer.py SETUP.exe OUT_DIR [theme]     (theme defaults to "default")

Needs one of: 7z / 7zz / 7za on PATH (Fedora: `sudo dnf install 7zip`), or the py7zr module.
Nothing extracted here may be redistributed; it stays on your machine.
"""
import os, shutil, subprocess, sys, tempfile

MAGIC = b"7z\xbc\xaf\x27\x1c"
XOR_LEN = 512


def restore_archive(setup, dst):
    data = open(setup, "rb").read()
    start = data.find(bytes(b ^ 0xFF for b in MAGIC))
    if start < 0:
        sys.exit(f"{setup}: no embedded FILES.7z found (not a BEACN installer, or a different format)")
    head = bytes(b ^ 0xFF for b in data[start:start + XOR_LEN])
    with open(dst, "wb") as f:
        f.write(head)
        f.write(data[start + XOR_LEN:])


def extract(archive, out, members):
    for tool in ("7z", "7zz", "7za"):
        exe = shutil.which(tool)
        if exe:
            subprocess.run([exe, "x", "-y", f"-o{out}", archive, *members], check=True,
                           stdout=subprocess.DEVNULL)
            return
    try:
        import py7zr
    except ImportError:
        sys.exit("Need 7z (Fedora: sudo dnf install 7zip) or the py7zr Python module")
    with py7zr.SevenZipFile(archive) as z:
        z.extract(path=out, targets=members)


def main():
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    setup, out = sys.argv[1], sys.argv[2]
    theme = sys.argv[3] if len(sys.argv) > 3 else "default"
    members = ["BEACN.exe", f"MixImages/{theme}.images"]
    os.makedirs(out, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=out) as tmp:
        arc = os.path.join(tmp, "FILES.7z")
        restore_archive(setup, arc)
        extract(arc, out, members)
    for m in members:
        p = os.path.join(out, m)
        if not os.path.isfile(p):
            sys.exit(f"{m} was not found in the installer payload")
        print(p)


if __name__ == "__main__":
    main()

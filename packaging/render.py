#!/usr/bin/env python3
"""Writes the Homebrew formula and Scoop manifest for a release's archives."""

import hashlib
import pathlib
import sys

HERE = pathlib.Path(__file__).parent
ARCHIVES = {
    "aarch64_sha256": "clch-aarch64-apple-darwin.tar.gz",
    "x86_64_sha256": "clch-x86_64-apple-darwin.tar.gz",
    "windows_sha256": "clch-x86_64-pc-windows-msvc.zip",
}


def main(version: str, folder: str) -> None:
    dist = pathlib.Path(folder)
    values = {"version": version}
    for key, name in ARCHIVES.items():
        values[key] = hashlib.sha256((dist / name).read_bytes()).hexdigest()
    for output in ("clch.rb", "clch.json"):
        text = (HERE / f"{output}.template").read_text()
        for key, value in values.items():
            text = text.replace("{" + key + "}", value)
        (dist / output).write_text(text)


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])

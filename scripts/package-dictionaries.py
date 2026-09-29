"""Produce deterministic, independently downloadable dictionary packages.

Run after build-dictionaries.py, or reuse the cached source words.zip snapshot.
The reader embeds only catalog.json; the ZIPs are release assets and test data.
"""
import hashlib
import io
import json
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DATA = ROOT / "assets" / "dictionaries"
VERSION = "2026-09-30"
BASE = f"https://github.com/Tikkaaa3/simPl-reader/releases/download/dictionaries-v1-{VERSION}/"
LANGUAGES = dict(en="english", tr="turkish", es="spanish", de="german",
                 fr="french", ja="japanese", ko="korean", zh="chinese")


def package(payloads):
    output = io.BytesIO()
    with zipfile.ZipFile(output, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for name, data in sorted(payloads.items()):
            info = zipfile.ZipInfo(name, (2026, 9, 30, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            archive.writestr(info, data, compresslevel=9)
    return output.getvalue()


def main():
    packs = DATA / "packs"
    packs.mkdir(exist_ok=True)
    manifest = json.loads((DATA / "manifest.json").read_text(encoding="utf-8"))
    catalog = dict(version=1, data_version=VERSION, base_url=BASE, packages=[])
    with zipfile.ZipFile(ROOT / "target" / "dictionary-research" / "words.zip") as archive:
        for pair in manifest["pairs"]:
            name = f"{pair['source']}-{pair['target']}"
            data = archive.read(name + ".tsv")
            assert hashlib.sha256(data).hexdigest() == pair["sha256"]
            metadata = dict(version=1, generated=VERSION, license="CC-BY-SA-4.0", pairs=[pair])
            payload = package({
                name + ".tsv": data,
                "manifest.json": (json.dumps(metadata, ensure_ascii=False, indent=2) + "\n").encode(),
                "LICENSE.txt": (DATA / "CC-BY-SA-4.0.txt").read_bytes(),
                "README.md": (DATA / "README.md").read_bytes(),
            })
            filename = f"{name}-{VERSION}.zip"
            (packs / filename).write_bytes(payload)
            catalog["packages"].append(dict(
                source=LANGUAGES[pair["source"]], target=LANGUAGES[pair["target"]],
                file=filename, bytes=len(payload), sha256=hashlib.sha256(payload).hexdigest(),
                data_bytes=len(data), data_sha256=pair["sha256"], provider=pair["provider"],
                entries=pair["entries"]))
            print(filename, len(payload))
    (DATA / "catalog.json").write_text(json.dumps(catalog, ensure_ascii=False, indent=2) + "\n",
                                       encoding="utf-8", newline="\n")


if __name__ == "__main__":
    main()

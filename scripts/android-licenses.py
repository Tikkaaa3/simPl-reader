"""Package exact Android runtime notices and index their full text for the app."""

import argparse
import json
from pathlib import Path
import shutil
import urllib.error
import urllib.request
import xml.etree.ElementTree as ET
import zipfile

ROOT = Path(__file__).resolve().parents[1]
NS = {"m": "http://maven.apache.org/POM/4.0.0"}


def pom_for(cache, coordinate, offline):
    group, name, version = coordinate.split(":")
    directory = cache / group / name / version
    candidates = sorted(directory.glob(f"*/{name}-{version}.pom"))
    if candidates:
        return ET.parse(candidates[0]).getroot()
    stored = ROOT / "target/native/android-poms" / f"{group}-{name}-{version}.pom"
    if not stored.exists():
        if offline:
            raise RuntimeError(f"POM not cached for {coordinate}")
        suffix = f"{group.replace('.', '/')}/{name}/{version}/{name}-{version}.pom"
        for repository in ("https://dl.google.com/dl/android/maven2/", "https://repo.maven.apache.org/maven2/"):
            try:
                with urllib.request.urlopen(repository + suffix, timeout=60) as response:
                    content = response.read()
                ET.fromstring(content)
                stored.parent.mkdir(parents=True, exist_ok=True)
                stored.write_bytes(content)
                break
            except urllib.error.HTTPError as error:
                if error.code != 404:
                    raise
        else:
            raise RuntimeError(f"No POM found for {coordinate}")
    return ET.parse(stored).getroot()


def declarations(cache, coordinate, offline, seen=None):
    seen = set() if seen is None else seen
    if coordinate in seen or len(seen) > 12:
        raise RuntimeError(f"Invalid POM parent chain: {coordinate}")
    seen.add(coordinate)
    pom = pom_for(cache, coordinate, offline)
    licenses = [(node.findtext("m:name", "", NS), node.findtext("m:url", "", NS))
                for node in pom.findall("m:licenses/m:license", NS)]
    if licenses:
        return licenses
    parent = pom.find("m:parent", NS)
    if parent is not None:
        parent_id = ":".join(parent.findtext(f"m:{field}", "", NS)
                             for field in ("groupId", "artifactId", "version"))
        return declarations(cache, parent_id, offline, seen)
    raise RuntimeError(f"No declared license for {coordinate}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--coordinates", type=Path, required=True)
    parser.add_argument("--gradle-cache", type=Path, required=True)
    parser.add_argument("--destination", type=Path, required=True)
    parser.add_argument("--offline", action="store_true")
    args = parser.parse_args()
    destination = args.destination.resolve()
    shutil.copytree(ROOT / "android/app/build/pdfium/third-party/pdfium", destination / "pdfium", dirs_exist_ok=True)
    fonts = ("Geist", "Literata", "Spectral", "FiraSans")
    for family in fonts:
        target = destination / "fonts" / f"{family}-OFL.txt"
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(ROOT / "assets/licenses" / target.name, target)
    shutil.copyfile(ROOT / "assets/licenses/Typeface-SOURCES.txt", destination / "fonts/Typeface-SOURCES.txt")
    shutil.copyfile(ROOT / "LICENSE.md", destination / "simPl-source-LICENSE.md")
    shutil.copyfile(ROOT / "LICENSE-BINARY.txt", destination / "simPl-binary-LICENSE.txt")
    jvm = destination / "android-runtime"
    if not jvm.resolve().is_relative_to(destination):
        raise RuntimeError("Notice output must stay inside the destination")
    # Rebuild this subtree to avoid leaving notices for removed dependencies.
    if jvm.exists():
        shutil.rmtree(jvm)
    jvm.mkdir(parents=True)
    for coordinate in args.coordinates.read_text(encoding="utf-8").splitlines():
        group, name, version = coordinate.split(":")
        licenses = declarations(args.gradle_cache, coordinate, args.offline)
        directory = jvm / f"{group}-{name}-{version}"
        directory.mkdir()
        (directory / "DECLARATION.txt").write_text(
            coordinate + "\n\n" + "\n".join(f"{title}\n{url}" for title, url in licenses) +
            "\n\nDeclarations come from this version's Maven POM (or its declared parent).\n", encoding="utf-8")
        for archive in sorted((args.gradle_cache / group / name / version).glob("*/*")):
            if archive.suffix not in (".jar", ".aar"):
                continue
            with zipfile.ZipFile(archive) as zipped:
                for member in zipped.namelist():
                    leaf = Path(member).name
                    if member.startswith("META-INF/") and any(word in leaf.upper() for word in ("LICENSE", "NOTICE", "COPYING")) and not member.endswith("/"):
                        (directory / member.replace("/", "_")).write_bytes(zipped.read(member))
        if any("apache" in (title + url).lower() for title, url in licenses):
            # Full unmodified Apache 2.0 text, already maintained in the repository.
            shutil.copyfile(ROOT / "assets/licenses/Material-Symbols-LICENSE.txt", directory / "Apache-2.0.txt")
        elif not any(file.name != "DECLARATION.txt" for file in directory.iterdir()):
            raise RuntimeError(f"Full legal text is missing for {coordinate}: {licenses}")
    entries = []
    for file in sorted(destination.rglob("*")):
        if not file.is_file() or file.name == "index.json":
            continue
        relative = file.relative_to(destination).as_posix()
        group = relative.split("/")[0]
        category = {"native": "Rust", "pdfium": "PDFium", "fonts": "Fonts", "android-runtime": "Android runtime"}.get(group, "simPl")
        title = relative.removeprefix("native/").replace("/", " · ")
        entries.append({"title": title, "category": category, "path": "licenses/" + relative})
    (destination / "index.json").write_text(json.dumps(entries, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"Packaged {len(entries)} full license and provenance documents.")


if __name__ == "__main__":
    main()

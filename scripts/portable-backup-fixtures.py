"""Generate deterministic v1 cross-platform backup fixtures (no copyrighted text)."""
import hashlib
import json
from pathlib import Path
import zipfile

ASSETS = Path(__file__).resolve().parents[1] / "android/app/src/androidTest/assets"
ASSETS.mkdir(parents=True, exist_ok=True)
TEXT = b"<title>Portable Harbour</title><p>A portable reading passage with a lighthouse.</p>"
KEY = hashlib.sha256(TEXT).hexdigest()

def encode(value):
    return json.dumps(value, ensure_ascii=False, separators=(",", ":")).encode()

for platform, root, source in [
    ("windows", "\\\\?\\C:\\Users\\Reader\\AppData\\Local\\simPl", "\\\\?\\C:\\Users\\Reader\\AppData\\Local\\simPl\\documents\\portable.html"),
    ("android", "/data/user/0/io.github.tikkaaa3.simpl/files/simPl", "/data/user/0/io.github.tikkaaa3.simpl/files/simPl/documents/portable.html"),
]:
    path_key = hashlib.sha256(source.encode("utf-16le" if platform == "windows" else "utf-8")).hexdigest()
    document = dict(path=source, title="Portable Harbour", fingerprint=KEY, kind="Html")
    files = {
        "documents/portable.html": TEXT,
        "library.json": encode(dict(version=1, entries=[dict(document=document, author="Reader", byte_len=len(TEXT), opened_at=1, progress=.5, current=1, total=2, cover=False, favourite=True)])),
        "recent.json": encode(dict(version=1, entries=[document])),
        "preferences.json": encode(dict(version=1, appearance="dark", theme="default", dictionary=dict(source="english", target="turkish", automatic=False))),
        "shelves.json": encode(dict(version=1, next_id=2, shelves=[dict(id=1, name="Portable shelf", books=[KEY])])),
        f"positions/{path_key}.json": encode(dict(version=1, position=dict(fingerprint=KEY, item_id="item-000001", within=.5, font_size=20.0))),
        f"annotations/{KEY}.json": encode(dict(version=1, fingerprint=KEY, next_id=2, bookmarks=[], highlights=[dict(id=1, created=1, color="yellow", place=dict(kind="reflow", chapter=None, **{"from":dict(item_id="item-000001", byte=0), "to":dict(item_id="item-000001", byte=10)}), page="1", quote="A portable", note="Travel note — İstanbul 😀")])),
    }
    for documents in [True, False]:
        included = {name: data for name, data in files.items() if documents or not name.startswith("documents/")}
        manifest = dict(version=1, source_root=root, options=dict(documents=documents, dictionaries=False), files=[dict(path=name, bytes=len(data), sha256=hashlib.sha256(data).hexdigest()) for name, data in sorted(included.items())])
        included["manifest.json"] = encode(manifest)
        with zipfile.ZipFile(ASSETS / f"p3-{platform}{'' if documents else '-light'}.zip", "w", compression=zipfile.ZIP_DEFLATED) as archive:
            for name, data in sorted(included.items()):
                info = zipfile.ZipInfo(name, (2026, 1, 1, 0, 0, 0))
                info.compress_type = zipfile.ZIP_DEFLATED
                archive.writestr(info, data)
ASSETS.joinpath("p3-portable.html").write_bytes(TEXT)

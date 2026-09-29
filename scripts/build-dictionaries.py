"""Build the bundled CC BY-SA word data. Python standard library only.

Network is used only by this explicit maintenance script; the reader uses the
checked-in archive offline. Source downloads are cached below target/.
"""
import hashlib
import gzip
import json
import re
import sqlite3
import unicodedata
import urllib.request
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CACHE = ROOT / "target" / "dictionary-research"
OUTPUT = ROOT / "assets" / "dictionaries"
PAIRS = [(source, target) for language in ("tr", "es", "de", "fr", "ja", "zh")
         for source, target in (("en", language), (language, "en"))]


def download(url, path):
    if not path.exists():
        temporary = path.with_suffix(path.suffix + ".part")
        try:
            urllib.request.urlretrieve(url, temporary)
            temporary.replace(path)
        finally:
            temporary.unlink(missing_ok=True)


def clean(text):
    return " ".join(text.replace("\x1f", " ").split())


def key(text, language):
    text = unicodedata.normalize("NFKC", clean(text)).replace("’", "'")
    if language == "tr":
        text = text.replace("I", "ı").replace("İ", "i")
    return text.lower().replace("ß", "ss")


def add(entries, word, meanings, language):
    normalized = key(word, language)
    if not normalized or len(normalized.encode()) > 256:
        return
    headword, stored = entries.setdefault(normalized, (clean(word), []))
    for meaning in meanings:
        meaning = clean(meaning)
        if meaning and len(meaning.encode()) <= 1024 and meaning not in stored and len(stored) < 20:
            stored.append(meaning)


def encode(entries):
    return "".join(f"{word}\t{head}\t{' | '.join(meanings)}\n"
                   for word, (head, meanings) in sorted(entries.items()) if meanings).encode()


def main():
    CACHE.mkdir(parents=True, exist_ok=True)
    OUTPUT.mkdir(parents=True, exist_ok=True)
    records, payloads = [], {}
    for source, target in PAIRS:
        pair = f"{source}-{target}"
        url = f"https://download.wikdict.com/dictionaries/sqlite/2_2026-06/{pair}.sqlite3"
        path = CACHE / f"{pair}-2026-06.sqlite3"
        download(url, path)
        entries = {}
        with sqlite3.connect(path.resolve().as_uri() + "?mode=ro", uri=True) as database:
            for word, meanings in database.execute("SELECT written_rep, trans_list FROM simple_translation"):
                if isinstance(word, str) and isinstance(meanings, str):
                    add(entries, word, meanings.split(" | "), source)
        extra = {}
        if pair == "zh-en":
            # Add explicit simplified/traditional forms instead of guessing conversions.
            cedict_url = "https://www.mdbg.net/chinese/export/cedict/cedict_1_0_ts_utf-8_mdbg.txt.gz"
            cedict = CACHE / "cedict-2026-09-30.txt.gz"
            download(cedict_url, cedict)
            with gzip.open(cedict, "rt", encoding="utf-8") as stream:
                for line in stream:
                    match = re.fullmatch(r"([^ ]+) ([^ ]+) \[[^\]]*\] /(.+)/\s*", line)
                    if match:
                        meanings = match[3].split("/")
                        add(entries, match[1], meanings, "zh")
                        add(entries, match[2], meanings, "zh")
            extra = dict(additional_sources=[dict(provider="CC-CEDICT / MDBG", url=cedict_url,
                source_sha256=hashlib.sha256(cedict.read_bytes()).hexdigest())])
        data = encode(entries)
        payloads[pair + ".tsv"] = data
        records.append(dict(source=source, target=target, provider="WikDict / CC-CEDICT" if extra else "WikDict", url=url,
                            source_sha256=hashlib.sha256(path.read_bytes()).hexdigest(),
                            sha256=hashlib.sha256(data).hexdigest(), entries=data.count(b"\n"), **extra))
        print(pair, records[-1]["entries"], len(data), flush=True)

    # English Wiktionary's Korean subset, not the Korean-language Wiktionary dump.
    url = "https://kaikki.org/dictionary/Korean/kaikki.org-dictionary-Korean.jsonl"
    path = CACHE / "korean.jsonl"
    download(url, path)
    entries = {}
    with path.open(encoding="utf-8") as stream:
        for line in stream:
            entry = json.loads(line)
            if entry.get("lang_code") != "ko":
                continue
            meanings = [gloss for sense in entry.get("senses", [])
                        for gloss in sense.get("glosses", [])]
            if meanings:
                add(entries, entry["word"], meanings, "ko")
    data = encode(entries)
    payloads["ko-en.tsv"] = data
    records.append(dict(source="ko", target="en", provider="Kaikki / Wiktionary", url=url,
                        source_sha256=hashlib.sha256(path.read_bytes()).hexdigest(),
                        sha256=hashlib.sha256(data).hexdigest(), entries=data.count(b"\n")))
    print("ko-en", records[-1]["entries"], len(data), flush=True)
    manifest = dict(version=1, license="CC-BY-SA-4.0", generated="2026-09-30", pairs=records)
    payloads["manifest.json"] = (json.dumps(manifest, ensure_ascii=False, indent=2) + "\n").encode()
    # Stable metadata makes repeated builds byte-for-byte reproducible.
    with zipfile.ZipFile(OUTPUT / "words.zip", "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for name, data in sorted(payloads.items()):
            info = zipfile.ZipInfo(name, (2026, 9, 30, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            archive.writestr(info, data, compresslevel=9)
    (OUTPUT / "manifest.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8", newline="\n")
    print("Archive bytes:", (OUTPUT / "words.zip").stat().st_size)


if __name__ == "__main__":
    main()

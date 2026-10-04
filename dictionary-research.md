# Local dictionary and translation options — 30 September 2026

This document is the initial research and design proposal. Local word
dictionaries were added later; their coverage and sources are in the
[data notice](assets/dictionaries/README.md) and the [README](README.md). No
Argos/model/plugin infrastructure was added. Once data or models are installed
from a file, use can be offline; obtaining packages from the internet is a
separate step. The comparison below is not a translation quality or Windows
performance benchmark.

## Lightweight dictionary data

| Source | Coverage and format | Assessment for simPl |
| --- | --- | --- |
| [WikDict](https://www.wikdict.com/page/download) | Wiktionary-based; SQLite, StarDict, TEI; CC BY-SA 4.0 | Strong candidate for first word lookups thanks to ready SQLite data. Part-of-speech/inflection data lives in separate language databases; the files for the two directions are not mirror images. |
| [FreeDict](https://freedict.org/downloads/) | TEI, dictd, StarDict, etc.; coverage/license vary per language pair | Alternative and complementary source. The English→Turkish catalog has 36,589 headwords. The [eng-tur source](https://github.com/freedict/fd-dictionaries/blob/master/eng-tur/eng-tur.tei) is GPL-2.0-or-later; freshness/quality must be checked per pair. |
| [JMdict/EDICT](https://www.edrdg.org/wiki/JMdict-EDICT_Dictionary_Project.html) | Japanese headwords, readings, senses; XML/text | Dedicated candidate for Japanese→English. The [EDRDG license](https://www.edrdg.org/edrdg/licence.html) is CC BY-SA 4.0; source and license attribution required. English→Japanese needs a separate reverse lookup index. |
| [CC-CEDICT](https://www.mdbg.net/chinese/dictionary?page=cedict) | Mandarin→English; traditional/simplified characters and pinyin; text | Dedicated candidate for Chinese. CC BY-SA 4.0. The reviewed page reports 125,139 entries. Reverse English lookup is possible but does not promise the coverage of two independent dictionaries. |
| [Kaikki / Wiktextract](https://kaikki.org/dictionary/) | JSONL from Wiktionary; hundreds of languages, senses and grammatical data; CC BY-SA/GFDL | General infrastructure, including Korean. Better filtered and indexed at build time than shipped as a ready end-user package. |
| [Korean-English Learners' Dictionary](https://krdict.korean.go.kr/eng/mainAction) | Learners' dictionary of the National Institute of Korean Language | Second candidate for Korean. The [text policy](https://krdict.korean.go.kr/eng/kboardPolicy/copyRightTermsInfo) is CC BY-SA; media licenses may differ. The scope and process for obtaining a complete English export must be verified separately. The online API alone is not a local solution. |

Approximate file sizes in WikDict's [current SQLite catalog](https://download.wikdict.com/dictionaries/sqlite/2/)
are below. These are the translation pair files only; adding the separate
language databases for inflection/part of speech increases the total. Size does
not measure sense coverage or quality.

| Direction | SQLite file | Reverse direction |
| --- | ---: | ---: |
| English→Turkish | 8 MB | Turkish→English: 4 MB |
| Spanish→English | 11 MB | English→Spanish: 15 MB |
| German→English | 25 MB | English→German: 20 MB |
| French→English | 22 MB | English→French: 23 MB |
| Japanese→English | 5 MB | English→Japanese: 9 MB |
| Chinese→English | 12 MB | English→Chinese: 5 MB |
| Korean→English | Not in the reviewed WikDict catalog | Evaluate Kaikki / the learners' dictionary |

During the research the English→Turkish SQLite file was downloaded and queried
read-only: 8,417,280 bytes, 47,626 rows in the `simple_translation` table.
`book` and `read` returned translations; `ran` was not found directly. `lead`
returned only the metal sense. This small sample shows that mapping inflected
words to their lemma and checking coverage of polysemous words are necessary; it
is not a full quality test. The file and sample queries are in
`target/dictionary-research/`, not part of the app.

The [Korean Kaikki page](https://kaikki.org/dictionary/Korean/index.html) reports
57,252 distinct forms and 195.4 MB of processed JSONL from the English
Wiktionary, including characters and proper names. This processed download is
being deprecated. The English edition of the [recommended raw source](https://kaikki.org/dictionary/rawdata.html)
is 2.8 GB compressed / 23.9 GB uncompressed; package builds must filter
`lang_code=ko`. The `ko-extract` on that page is the Korean Wiktionary edition;
it must not be picked by mistake instead of the Korean package with English
glosses. End users do not need to download the whole dump; the size of the
filtered package we would prepare has not been measured yet.

## Local sentence/paragraph translation

| Option | Coverage/license | Assessment |
| --- | --- | --- |
| [Argos Translate](https://github.com/argosopentech/argos-translate) | Local Python engine, separate `.argosmodel` packages; MIT/CC0 | First candidate for an optional translation engine. Missing direct pairs can pivot through an intermediate language, with possible extra quality loss. Windows packaging of the Python/native dependencies must be tried separately. |
| [OPUS-MT / Marian](https://github.com/Helsinki-NLP/Opus-MT) | Downloadable language models; project code MIT, the listed OPUS-MT models CC BY 4.0 | Pair-specific models can be chosen. Compared to Argos, we would own more of the tokenizer/model/runtime integration. Each checkpoint's card must be checked separately. |
| [Bergamot / translateLocally](https://github.com/XapaJIaMnu/translateLocally) | C++ local engine; Windows app and CLI; app MIT | CPU-focused alternative. Limited to the models in its catalog; availability of all target pairs not verified. Engine, UI and model licenses must be checked separately. |
| [Apertium](https://github.com/apertium/apertium) | Rule-based translation; core GPL-2.0 | [Language pairs](https://www.wiki.apertium.org/wiki/List_of_language_pairs) vary in maturity/direction. A candidate for some European languages; not recommended as a common starting point for the seven requested pairs. |
| [M2M100 418M](https://huggingface.co/facebook/m2m100_418M) | 100 languages, including the seven targets; MIT | Candidate for covering many pairs with one model. 418 million parameters; heavier than a dictionary package. Real Windows speed/RAM/quality not measured. |
| [NLLB-200 600M](https://huggingface.co/facebook/nllb-200-distilled-600M) | Broad language coverage; CC BY-NC 4.0 | An option for local research; not recommended as a default distribution candidate because the license restricts commercial use. The model card emphasizes research use. |
| [MADLAD-400 3B](https://huggingface.co/google/madlad400-3b-mt) | 400+ languages; Apache-2.0 | Broad-coverage optional large model candidate. Three billion parameters plus an extra runtime would noticeably grow the lightweight reader package. |
| [TranslateGemma](https://blog.google/innovation-and-ai/technology/developers-tools/translategemma/) | 55 languages; 4B, 12B, 27B | New large-model alternative. The [model card](https://huggingface.co/google/translategemma-4b-it) states the Gemma terms of use and license acceptance for download. Too large for a lightweight first release; could be a separate engine by user choice. |
| [LibreTranslate](https://github.com/LibreTranslate/LibreTranslate) | Locally installable HTTP service; Argos-based; AGPL-3.0 | Not a new translation model. An option to connect to users who run a local service; for simPl a direct engine is simpler than requiring an extra service at first. |

Argos's [official package index, read on 30 September](https://raw.githubusercontent.com/argosopentech/argospm-index/main/index.json),
contains direct packages in both directions with English for Turkish, Spanish,
German, French, Japanese, Korean and Chinese. Traditional Chinese additionally
has `zt` packages. The Turkish package version is 1.5, Japanese/Korean 1.1;
these are not the engine's current version or a quality score. The maintainer
[stated that the model files are MIT/CC0 as well](https://github.com/argosopentech/argos-translate/issues/533#issuecomment-5160080718).
HEAD requests to the package URLs returned 403; model download sizes and
download/run success were not verified in this session. No translation engine
was installed or run.

## File formats and existing apps

[GoldenDict](https://github.com/goldendict/goldendict) reads many local
dictionary formats; if the user wishes, it could serve as an external dictionary
app in a first stage. It does not fully replace a meaning panel inside simPl.
[Yomitan](https://github.com/yomidevs/yomitan) is a reference for a pop-up
dictionary/language-learning interface; it is a browser extension.
[PyGlossary](https://github.com/ilius/pyglossary) is a candidate for format
conversion when building packages. StarDict/MDict/Yomitan are file formats or
ecosystems; a file's format does not determine the license of the dictionary
data inside it.

## Recommendation for simPl

First, word/phrase lookup with data packages that can be added later. The first
packages could be WikDict English→Turkish, Spanish→English, German→English and
French→English. Japanese JMdict, Chinese CC-CEDICT and filtered Korean Kaikki
data could be added through separate data adapters. These were the initial
recommendations. Word lookup is now implemented with offline dictionaries
covering 13 directions. By the user's decision, all dictionaries were split into
optional downloadable packages; the main app carries no word data.

For the first release, instead of a general plugin system that runs DLLs, a
manifest with source/target language, version, source link, license and hash
plus a read-only index file may be enough. Users pick the language packages they
want; the main installation does not carry every language. Inflection-to-lemma
mapping, Japanese/Chinese word boundaries and Korean suffixes are separate
product work that exact text matching cannot solve.

Then optional sentence translation: a separate translation provider in the same
panel, Argos first, with OPUS-MT/Bergamot for comparison. Models should load
only when needed, work must not block the UI thread, and a new selection should
cancel the previous job. The larger M2M100/MADLAD/TranslateGemma models should
stay opt-in.

Before deciding, rare words, polysemy, inflections, idioms and a few short
paragraphs should be tried with a shared small evaluation set for each target
language. First load, repeated lookup, RAM and CPU should be measured on
Windows. What is verified today is catalog coverage; quality, dictionary
indexing performance and model performance remain open.

## Implemented downloadable dictionaries — 30 September 2026

All 13 language directions were split into separate, optional packages. The main
exe carries only a **5,175-byte catalog**; it contains no word data or
translation models. The source SQLite/JSONL size is not the size shipped to
users. The prepared ZIP packages, including license and source notices, total
**19,769,702 bytes (19.77 MB)**. MB values are decimal.

| Language package | Download total | Uncompressed TSV data |
| --- | ---: | ---: |
| English ↔ Turkish | 1.13 MB | 3.22 MB |
| English ↔ Spanish | 2.22 MB | 7.07 MB |
| English ↔ German | 3.65 MB | 11.30 MB |
| English ↔ French | 4.02 MB | 13.67 MB |
| English ↔ Japanese | 1.58 MB | 4.31 MB |
| English ↔ Chinese | 5.91 MB | 16.40 MB |
| Korean → English | 1.26 MB | 3.36 MB |

Directions are downloaded independently. For example, English→Turkish alone is
**0.63 MB** and Turkish→English **0.50 MB**. The largest direction is
Chinese→English at **5.52 MB**. Files are stored as compressed ZIPs under
`%LOCALAPPDATA%\simPl\dictionaries`; reopening needs no internet. Only the
active direction is opened and indexed in memory. The uncompressed TSV size does
not indicate RAM use; strings and the lookup index take space too.

Settings show the selected direction, package size and status. Manage
dictionaries opens the 13 directions, with Download/Remove, progress/Cancel,
retry after an error and Import ZIP. When a dictionary is missing, the word card
also offers the download; a successful installation refreshes the result of the
same open card. Automatic word lookup never starts a download automatically.
Selected text and document content are never sent to the internet.

Packages are fetched over HTTPS from a separate GitHub data release. Size and
SHA-256 are verified against the immutable catalog inside the exe; the TSV and
manifest are verified too. Adding from a file uses the same verification path. A
cancelled or corrupt download does not replace the current valid package. Files
of a data release must not be changed in place; a new release needs a different
name and an updated app catalog. General third-party dictionary formats and the
Argos engine/model packages are later work.

In the previous build with embedded data, the exe was **38,008,320 bytes
(38.01 MB)**, the portable folder including PDFium **47,591,520 bytes
(47.59 MB)** and the portable ZIP **31,489,574 bytes (31.49 MB)**. In the new
downloadable-package build, the exe is **18,444,288 bytes (18.44 MB)**, the
portable folder **28,033,234 bytes (28.03 MB)** and the portable ZIP
**11,862,130 bytes (11.86 MB)**. The release and portable exe SHA-256 matched;
the ZIP CRC check passed. Dictionary ZIPs were not copied into the regular
portable package. These are not installer sizes.

# Dictionary and promotional-media license review — 2026-09-30

## Verified scope

- WikDict's [official attribution](https://www.wikdict.com/page/about) links to
  CC BY-SA 4.0 and names Karl Bartel, Wiktionary and DBnary.
- The [English Wiktionary copyright page](https://en.wiktionary.org/wiki/Wiktionary:Copyrights)
  offers original entry text under CC BY-SA 4.0; the Korean subset uses this data
  through [Kaikki / Wiktextract](https://kaikki.org/dictionary/Korean/index.html).
- [MDBG's CC-CEDICT page](https://www.mdbg.net/chinese/dictionary?page=cedict)
  and the actual cached 2026-09-30 download header both specify CC BY-SA 4.0.
  The header also retains `CEDICT - Copyright (C) 1997, 1998 Paul Andrew Denisowski`.
- All 13 checked dictionary ZIPs match the application's catalog SHA-256 hashes
  and contain attribution, a source manifest and the full CC BY-SA 4.0 legal text.
  They contain plain-text indexes rather than upstream images or usage quotations.
- The actual published 0.1.5 portable ZIP includes dictionary attribution and
  legal text, font/icon notices, PDFium notices and Rust notices. The notice
  collector was rerun offline successfully for 190 shipped Rust dependencies,
  plus the pinned Rust standard-library notices. This verifies notice coverage;
  it is not an independent legal opinion on every dependency or upstream entry.
- Promotional documents, author names, text, covers and PDF art were created for
  the demonstration. The translation result is actual WikDict data; its image
  and contact-sheet preview require their own attribution when shared.

## Clarifications made

- The source license explicitly excludes CC-licensed dictionary content,
  including content reproduced in documentation and promotional images.
  Official binary terms also explicitly preserve independent dictionary rights.
- Dictionary credits retain the original CEDICT copyright notice verbatim.
  This supplemental notice applies to the unchanged published v1 dictionary
  packages as well. Anyone redistributing them should include the updated notice.
- The main README credits the translation image beside its display. The media
  kit contains [image attribution](screenshots/ATTRIBUTION.md) and an
  [HTML caption](screenshots/attribution.html) for website use, identifies changes
  and explicitly preserves CC BY-SA rights in dictionary content.
- Export metadata identifies the dictionary-bearing images. The exporter
  includes the notices in future kits. The dictionary packager rejects changed
  ZIPs under an existing version, including changes to notices.

## License implications and remaining boundary

[CC BY-SA 4.0, sections 3 and 4](https://creativecommons.org/licenses/by-sa/4.0/legalcode.en)
require attribution, license information, change indications and ShareAlike for
covered adaptations. The index adaptations are offered under CC BY-SA 4.0;
commercial reuse is allowed. The PolyForm Noncommercial source license and
binary sharing restrictions must not be applied to that data.

The application and separately licensed data are distributed as independent
components. Likewise, the promotional images identify the dictionary content
alongside independent interface/demo content.
[Creative Commons' collection guidance](https://creativecommons.org/faq/#if-i-create-a-collection-that-includes-a-work-offered-under-a-cc-license-which-licenses-may-i-choose-for-the-collection)
permits independent licensing of a collection while preserving each included
work's own license; merely displaying/loading dictionary data does not relicense
the independent application's source code.

The live promotional website was not part of this repository review. When it
publishes the translation image or contact sheet, it must also publish the linked
caption/full attribution and avoid conflicting site-wide restrictions on the
dictionary content. Attribution inside the downloaded media ZIP alone does not
verify compliance of a separate website.

This review establishes the licensing sources and concrete notice coverage.
It cannot certify every upstream contributor's rights or provide a legal
guarantee for all future uses. Original third-party content can have separate
terms, as Wiktionary's copyright page explains; the current builders extract
translations/glosses and do not package media or literary usage examples.

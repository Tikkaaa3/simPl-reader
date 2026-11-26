# simPl privacy policy

Effective date: 2026-11-26. Applies to the Windows simPl Reader application
(website, GitHub Releases and Microsoft Store) and the Android simPl Reader
application (`io.github.tikkaaa3.simpl`), along with their optional features.

simPl has no account system, advertising, analytics or telemetry. The application
does not upload your books, reading history, notes, searches or dictionary queries
to the project maintainers.

## Data on your device

Imported documents are copied into local application storage (or, on Windows, a
custom library folder you choose). Reading positions, bookmarks, highlights,
notes, shelves and preferences are stored locally. Disposable conversions and
dictionary downloads use local cache/storage. Removing a book removes its
managed copy while retaining saved annotations; it does not delete your original
file.

- **Windows (Microsoft Store):** Application data is stored in the package's
  Windows local app data container. Uninstalling the app removes package-private
  data while leaving original files and any external custom library folder
  untouched.
- **Windows (website installer and portable):** Application data is stored under
  `%LOCALAPPDATA%\simPl`. Uninstalling lets you choose whether to keep or delete
  that folder.
- **Android:** Application data is stored in app-private storage. Uninstalling
  the app or clearing its storage removes app-private data. Android's automatic
  cloud backup is disabled.

## Optional network and other applications

Dictionary packages are downloaded only when requested. The package host receives
ordinary connection information such as your IP address and requested package.
Definitions and lookup queries are processed locally. Package hosts are identified
in the dictionary catalog; hosting providers apply their own privacy policies.

Read aloud on Windows uses locally installed Windows speech voices offline. On
Android, read aloud passes book text to your chosen speech engine; simPl prefers
offline voices, and a network voice is used only after explicit consent and may
send text and connection information to the engine's provider. Its privacy
practices are independent of simPl. You can stop playback and choose another
voice.

Opening, importing, dragging and dropping, or using "Open with" gives simPl
access only to the files or folders you select. On Android, the system file
picker may also offer cloud document providers, which operate under their own
policies. Sharing selected text or exported notes gives the content you choose
to the receiving application.

## Backups and exports

Backups and exported notes are saved to destinations you choose. Portable ZIP
backups are not encrypted; full backups can contain complete books and notes.
Choose a trusted destination and protect or delete exported files yourself.
Exports are outside app-private storage and can remain after uninstallation.
Restoring a backup replaces the app's reading profile after your confirmation.

## Contact and changes

Project information is available at [simplreader.app](https://simplreader.app),
and the source and issue tracker are at
[Tikkaaa3/simPl-reader](https://github.com/Tikkaaa3/simPl-reader).
Use the [issue tracker](https://github.com/Tikkaaa3/simPl-reader/issues) for
questions; do not attach private books or notes to public reports. Policy changes
will update this page and its effective date.

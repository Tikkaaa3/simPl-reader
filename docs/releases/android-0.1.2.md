# simPl Android 0.1.2

This update redesigns the mobile reading view around the page.

- One compact bottom toolbar replaces the title bar and wrapping action rows.
  Previous/Next, the editable current page, Contents and Reader tools stay within
  one row. Narrow phones keep large touch targets by placing Contents in the menu.
  Import messages appear above the controls so they remain usable immediately.
- Enter a page number or EPUB printed label directly in the bottom toolbar and
  submit with Go. Invalid input stays editable for correction.
- Book mode opens with the complete page centered and fitted to the screen,
  including tall PDF Book pages. Fullscreen, rotation and typography changes
  update the fit; Fit width remains available in Reader tools.
- Zoom buttons preserve the current reading point at the viewport center.
  Pinch preserves the point between the fingers, with panning and slim scroll
  indicators when the enlarged page extends beyond the screen.
- Contents, search, annotations, typography, bookmarks, book switching, settings
  and read aloud open from the bottom controls. Panels open on request and have
  explicit close buttons. Active speech uses a small pause/stop row.
- PDF Document mode shares the same compact controls, direct page input and
  centered zoom.
- Reading themes now color the library, settings and reader controls using the
  shared desktop palettes. Settings includes theme previews and appearance
  controls alongside reading preferences.
- The refreshed library uses a compact cover grid, a Continue reading card,
  shelf filters and sorting by recent activity, title, author or format.

Android 8.0 (API 26) or later is required. Most phones should use the ARM64 APK;
the universal APK also contains x86_64 for supported devices and emulators.
The APKs use the existing distribution signing certificate and can update
Android 0.1.0 or 0.1.1 without uninstalling the app.

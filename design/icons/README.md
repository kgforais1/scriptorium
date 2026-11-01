# App icon

Last reviewed: 2026-11-01

`app-icon.svg` is the source of the Scriptorium app icon: an open book with a turning page and a
slim gold quill on its left page, on a dark wood plate with a soft lamplight disc. It uses the
Lamplit palette. `app-icon-1024.png` is its 1024 px render.

To change the icon, edit the SVG, render it to a 1024 px PNG, and regenerate the platform
sizes in `src-tauri/icons/`:

```sh
pnpm tauri icon design/icons/app-icon-1024.png
```

The command also writes Android and iOS folders. The app is desktop-only, so delete
`src-tauri/icons/android/`, `src-tauri/icons/ios/`, and `src-tauri/icons/64x64.png` before
committing.

## Alternates

Two runners-up from the same design round, kept for later:

- `alternates/quill-below-glow-disc.svg`: the quill lies flat under the book, on the same disc.
- `alternates/quill-on-page-diffuse-glow.svg`: the chosen layout with a diffuse glow that
  fades out instead of a disc.

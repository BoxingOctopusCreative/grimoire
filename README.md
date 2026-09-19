# Grimoire

Cross-platform desktop eBook library manager built with [Tauri 2](https://v2.tauri.app/) and [SvelteKit](https://svelte.dev/). Local SQLite catalog, Calibre-style on-disk library layout, and USB/MTP Kindle sync (including Kindle Colorsoft).

## Features (v0.1)

- Create or open a library folder with `metadata.db` and book files on disk
- Import EPUB, PDF, AZW3, MOBI, TXT, and comics (CBZ/CBR)
- Extract metadata on import: title, authors, tags, series, description, identifiers, and cover (EPUB, PDF, MOBI/AZW3)
- Fill missing metadata and cover art from Open Library (manual fetch, plus auto after sparse imports)
- Read EPUB, PDF, TXT, DRM-free MOBI, and comics (CBZ/CBR) in a separate window with saved progress (MOBI is converted to EPUB for display)
- AZW3: import and Kindle send (Amazon store AZW3 is usually DRM-protected and is not opened in-app)
- Convert formats in-app: EPUB → MOBI/AZW3, TXT → EPUB (no Calibre)
- Search (FTS5), edit metadata, delete books
- Detect connected Kindles and other eReaders over USB/MTP or mounted volumes
- Convert EPUB to MOBI or AZW3 natively for Kindle USB send (no Calibre required)
- Record transfer history per device

## Requirements

- Node.js 20+
- pnpm
- Rust toolchain (for Tauri)
- Platform Tauri dependencies ([guide](https://v2.tauri.app/start/prerequisites/))

## Develop

```bash
pnpm install
pnpm tauri dev
```

## Build

```bash
pnpm tauri build
```

Or use the Makefile for production platform builds:

```bash
make build          # current host
make build-mac      # macOS universal
make build-windows  # Windows x64
make build-linux    # Linux x64 (Linux host)
```

Run `make help` for the full target list. Artifacts are under `src-tauri/target/.../release/bundle/`.

## Test

```bash
make test           # Vitest + cargo test
make test-frontend  # frontend unit tests
make test-rust      # Rust unit/integration tests
```

Or: `pnpm test`, `pnpm test:rust`, `pnpm test:all`.

## Release pipeline (GitHub Actions)

Every pull request runs the test suite. When a PR is **merged**, or when you run the workflow manually (**Actions → CI and Release → Run workflow**), Actions also:

1. Runs tests again
2. Bumps the version from the PR title (or the manual bump choice):
   - Title starts with `MAJOR` → major bump (`1.2.3` → `2.0.0`)
   - Title starts with `MINOR` → minor bump (`1.2.3` → `1.3.0`)
   - Title starts with `PATCH`, or anything else → patch bump (`1.2.3` → `1.2.4`)
3. Creates a `vX.Y.Z` tag and a GitHub Release (notes from commit subjects since the previous tag)
4. Builds production bundles for macOS (arm64 + x64), Linux x64, and Windows x64 (NSIS setup.exe **and** MSI), then attaches them to that release

Example titles: `MINOR add dark mode default`, `MAJOR redesign library schema`, `add filter sidebar` (patch).

Manual runs: pick `patch`, `minor`, or `major` in the workflow form.

Repo Settings → Actions → General → Workflow permissions must allow **Read and write** so the workflow can push the version commit, tag, and release assets.

### Windows installers and library path

Windows releases include both:

- **NSIS** (`*-setup.exe`): interactive installs ask for the ebook library folder (default `Documents\Grimoire`). Silent/passive installs use that default, or `/LIBRARYPATH=D:\Books`.
- **MSI** (WiX): seeds the same default library path. Override with `msiexec /i Grimoire_*.msi LIBRARYPATH="D:\Books"`.

Both write `%APPDATA%\grimoire\config.json` only when that file does not already exist, so upgrades keep an existing library path.

## Library layout

When you choose a library folder, Grimoire writes:

```text
MyLibrary/
  metadata.db
  Author Name/
    Book Title/
      Book Title - Author Name.epub
      cover.jpg
```

App preferences (saved library path) live in the OS config directory under `grimoire/config.json`. Set the path in **Settings**; Grimoire opens that library automatically on startup.

## Open Library metadata

Grimoire can look up missing fields and cover art via [Open Library](https://openlibrary.org/).

- Prefer ISBN when present; otherwise title + author
- Only fills empty values (Unknown authors, blank tags/notes/ISBN, missing cover)
- Trigger from the book page (**Fetch from Open Library**), the library context menu (**Fetch metadata**), or automatically after import when cover/author looks incomplete
- Use **Choose cover** on the book page to pick among Open Library edition covers

No API key is required.

## Compatible eReaders

Grimoire detects these families over USB MTP or mounted volumes:

| Family | Preferred formats | USB send |
| --- | --- | --- |
| Kindle (Paperwhite, Colorsoft, Oasis, Scribe, and similar) | AZW3, MOBI, PDF | Yes (MTP) |
| Kobo | EPUB, KEPUB, PDF | Detection only |
| PocketBook | EPUB, PDF, FB2 | Detection only |
| reMarkable | PDF, EPUB | Detection only |
| BOOX / Onyx | EPUB, PDF | Detection only |
| Tolino | EPUB, PDF | Detection only |
| Nook | EPUB, PDF | Detection only |

Other USB devices that identify as an eReader or ebook device may appear as a generic eReader (EPUB, PDF). USB send is Kindle MTP only today. From Devices you can still browse and remove books on connected readers.

## Kindle USB / MTP notes

2024 Kindles (Paperwhite, Colorsoft, and similar) use MTP. Grimoire uses [`mtp-rs`](https://crates.io/crates/mtp-rs) for transfers.

### Device detection

The Devices page keeps watching while the app is open:

- **MTP hotplug:** connect/disconnect shows a toast; Devices and Send update quietly (no forced navigation)
- **Mounted volumes:** polled every few seconds for Kobo (`.kobo`), Kindle (`documents` + `system`), reMarkable markers, and volume names that match known brands

### Device library

From Devices, open **View books** on a connected reader. Grimoire scans the device for ebook files (EPUB, PDF, AZW3, MOBI, and similar) and shows them in a grid or list. You can multi-select and remove books from the device. The view preference is remembered.

### Format selection

1. Prefer an existing AZW3, then MOBI, then PDF
2. If the book is EPUB-only, convert natively to dual-format MOBI (via [`kindling-mobi`](https://crates.io/crates/kindling-mobi)) and store it in the library
3. Upload into the device `documents` folder

Raw EPUB copied over USB often does not appear in the Kindle library. Native conversion handles that path for USB send.

### macOS

macOS may lock the Kindle with `ptpcamerad` or Android File Transfer. If Grimoire cannot open the device:

1. Quit Android File Transfer
2. Run `pkill -9 ptpcamerad` (or disable it via `launchctl` if you accept Photos.app side effects)
3. Retry from Devices or the book detail page

### Linux

You may need udev rules so your user can access USB MTP devices without root. See the mtp-rs README for a sample rule.

### Windows

Kindles usually appear through the Windows Portable Devices stack with no extra drivers.

## Project layout

- `src/` - SvelteKit SPA UI
- `src-tauri/` - Rust backend (SQLite, import, conversion, MTP)
- `src-tauri/migrations/` - SQL schema migrations

## License

[Mozilla Public License 2.0](LICENSE)

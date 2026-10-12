# Grimoire

Cross-platform desktop eBook library manager. The window is a native [GPUI](https://gpui-kit.com/) app. The catalog is a local SQLite database with a Calibre-style layout on disk, plus USB/MTP sync for Kindles and other eReaders.

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

- Rust toolchain

## Develop

```bash
cargo run --manifest-path app/Cargo.toml --bin Grimoire
```

## Build

```bash
cargo build --manifest-path app/Cargo.toml --release --bin Grimoire
```

Or use the Makefile:

```bash
make build          # release binary for this machine
make build-mac      # macOS disk image (loads .env for Apple signing if present)
make build-windows  # Windows x64 (Windows host)
make build-linux    # Linux x64 (Linux host)
```

For a signed and notarized local macOS build, copy `.env.example` to `.env`, fill in the Apple values, then run `make build-mac`. See that file for variable notes.

Run `make help` for the full target list. Disk images land in `dist/`. The release binary is under `app/target/release/`.

## Test

```bash
make test
cargo test --manifest-path app/Cargo.toml
```

The tests cover the catalog, conversion, Kindle detection, and the GPUI theme and reader-font choices.

## Release pipeline (GitHub Actions)

Every pull request runs the test suite. When a PR is **merged**, or when you run the workflow manually (**Actions → CI and Release → Run workflow**), Actions also:

1. Runs tests again
2. Bumps the version from the PR title (or the manual bump choice):
   - Title starts with `MAJOR` → major bump (`1.2.3` → `2.0.0`)
   - Title starts with `MINOR` → minor bump (`1.2.3` → `1.3.0`)
   - Title starts with `PATCH`, or anything else → patch bump (`1.2.3` → `1.2.4`)
3. Creates a draft `vX.Y.Z` GitHub Release (notes from commit subjects since the previous tag)
4. Builds release binaries for macOS (arm64 + x64 disk images, signed and notarized), Linux x64, and Windows x64, then attaches them to that draft
5. Publishes the release (immutable releases lock assets only after publish)

Example titles: `MINOR add dark mode default`, `MAJOR redesign library schema`, `add filter sidebar` (patch).

Manual runs: pick `patch`, `minor`, or `major` in the workflow form.

### Release token (required)

`main` requires PRs, so the default `GITHUB_TOKEN` cannot push the version bump. Create a PAT for a **repo admin** and store it as the Actions secret `RELEASE_GITHUB_TOKEN`:

1. GitHub → Settings → Developer settings → Personal access tokens
2. Classic: enable the `repo` scope. Fine-grained: this repository, Contents **Read and write**
3. Repo → Settings → Secrets and variables → Actions → New repository secret → name `RELEASE_GITHUB_TOKEN`

Repo Settings → Actions → General → Workflow permissions should still allow **Read and write** for PR checks and other jobs.

### macOS signing and notarization (required for release builds)

macOS release jobs sign with a **Developer ID Application** certificate and notarize via the App Store Connect API. Add these Actions secrets:

| Secret | Value |
| --- | --- |
| `APPLE_CERTIFICATE` | Base64-encoded `.p12` (`openssl base64 -A -in cert.p12 -out cert-base64.txt`) |
| `APPLE_CERTIFICATE_PASSWORD` | Password used when exporting the `.p12` |
| `APPLE_API_KEY` | App Store Connect API Key ID |
| `APPLE_API_ISSUER` | App Store Connect Issuer ID |
| `APPLE_API_KEY_CONTENTS` | Full contents of the downloaded `.p8` private key |

CI reads the signing identity from `APPLE_CERTIFICATE`, so do not set `APPLE_SIGNING_IDENTITY` in Actions (a mismatch fails the build). For local Keychain signing, set `APPLE_SIGNING_IDENTITY` in `.env` instead.

Create the Developer ID certificate and the App Store Connect API key in [Apple Developer](https://developer.apple.com/account) and [App Store Connect](https://appstoreconnect.apple.com/access/integrations/api). Signed and notarized DMGs open normally; drag Grimoire into Applications.

### Windows

Windows releases are the `Grimoire.exe` binary. Choose the library folder in Settings. Grimoire writes `%APPDATA%\grimoire\config.json` and keeps that library path on later launches.

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

The Devices page checks for connected readers every few seconds. A notice appears when a reader connects or disconnects. Detection covers USB MTP devices and mounted volumes (Kobo, Kindle, reMarkable, and volume names that match known brands).

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

- `app/` - Rust app (GPUI window, SQLite catalog, import, conversion, MTP)
- `app/migrations/` - SQL schema migrations
- `static/` - in-app icon artwork

## License

[Mozilla Public License 2.0](LICENSE)

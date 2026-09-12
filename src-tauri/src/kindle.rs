use crate::error::{err, AppResult};
use bytes::Bytes;
use futures::stream;
use mtp_rs::mtp::{MtpDevice, NewObjectInfo, ObjectHandle};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use tokio::fs::File;
use tokio::io::AsyncReadExt;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReaderFamily {
    Kindle,
    Kobo,
    Pocketbook,
    Remarkable,
    Boox,
    Tolino,
    Nook,
    Other,
}

impl ReaderFamily {
    pub fn label(&self) -> &'static str {
        match self {
            ReaderFamily::Kindle => "Kindle",
            ReaderFamily::Kobo => "Kobo",
            ReaderFamily::Pocketbook => "PocketBook",
            ReaderFamily::Remarkable => "reMarkable",
            ReaderFamily::Boox => "BOOX",
            ReaderFamily::Tolino => "Tolino",
            ReaderFamily::Nook => "Nook",
            ReaderFamily::Other => "eReader",
        }
    }

    pub fn preferred_formats(&self) -> Vec<&'static str> {
        match self {
            ReaderFamily::Kindle => vec!["AZW3", "MOBI", "PDF"],
            ReaderFamily::Kobo => vec!["EPUB", "KEPUB", "PDF"],
            ReaderFamily::Pocketbook => vec!["EPUB", "PDF", "FB2"],
            ReaderFamily::Remarkable => vec!["PDF", "EPUB"],
            ReaderFamily::Boox => vec!["EPUB", "PDF"],
            ReaderFamily::Tolino => vec!["EPUB", "PDF"],
            ReaderFamily::Nook => vec!["EPUB", "PDF"],
            ReaderFamily::Other => vec!["EPUB", "PDF"],
        }
    }

    pub fn supports_usb_send(&self) -> bool {
        matches!(self, ReaderFamily::Kindle)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionKind {
    Mtp,
    Mount,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EbookReaderDevice {
    pub id: String,
    pub connection: ConnectionKind,
    pub location_id: Option<u64>,
    pub mount_path: Option<String>,
    pub manufacturer: String,
    pub product: String,
    pub serial_number: Option<String>,
    pub family: ReaderFamily,
    pub family_label: String,
    pub free_space_bytes: Option<u64>,
    pub display: String,
    pub supports_send: bool,
    pub preferred_formats: Vec<String>,
    pub notes: Option<String>,
}

/// Backward-compatible alias used by existing Kindle send paths.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KindleDeviceInfo {
    pub location_id: u64,
    pub manufacturer: String,
    pub product: String,
    pub serial_number: Option<String>,
    pub is_kindle: bool,
    pub free_space_bytes: Option<u64>,
    pub display: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferResult {
    pub remote_path: String,
    pub device_serial: String,
    pub format: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncRecord {
    pub book_id: i64,
    pub device_serial: String,
    pub last_sent_at: String,
    pub remote_path: String,
    pub format: String,
}

pub fn classify_reader(manufacturer: &str, product: &str) -> Option<ReaderFamily> {
    let m = manufacturer.to_ascii_lowercase();
    let p = product.to_ascii_lowercase();
    let blob = format!("{m} {p}");

    if m.contains("amazon")
        || p.contains("kindle")
        || p.contains("colorsoft")
        || p.contains("paperwhite")
        || p.contains("oasis")
        || p.contains("scribe")
    {
        return Some(ReaderFamily::Kindle);
    }
    if m.contains("kobo") || p.contains("kobo") || m.contains("rakuten") {
        return Some(ReaderFamily::Kobo);
    }
    if m.contains("pocketbook") || p.contains("pocketbook") {
        return Some(ReaderFamily::Pocketbook);
    }
    if m.contains("remarkable") || p.contains("remarkable") {
        return Some(ReaderFamily::Remarkable);
    }
    if m.contains("onyx") || p.contains("boox") || p.contains("onyx") {
        return Some(ReaderFamily::Boox);
    }
    if m.contains("tolino") || p.contains("tolino") {
        return Some(ReaderFamily::Tolino);
    }
    if m.contains("barnes") || p.contains("nook") || m.contains("nook") {
        return Some(ReaderFamily::Nook);
    }
    if blob.contains("ereader") || blob.contains("e-reader") || blob.contains("ebook") {
        return Some(ReaderFamily::Other);
    }
    None
}

fn reader_from_family(
    family: ReaderFamily,
    id: String,
    connection: ConnectionKind,
    location_id: Option<u64>,
    mount_path: Option<String>,
    manufacturer: String,
    product: String,
    serial_number: Option<String>,
    free_space_bytes: Option<u64>,
    display: String,
    notes: Option<String>,
) -> EbookReaderDevice {
    let preferred_formats = family
        .preferred_formats()
        .into_iter()
        .map(str::to_string)
        .collect();
    EbookReaderDevice {
        id,
        connection,
        location_id,
        mount_path,
        manufacturer,
        product,
        serial_number,
        family_label: family.label().to_string(),
        supports_send: family.supports_usb_send() && matches!(connection, ConnectionKind::Mtp),
        preferred_formats,
        family,
        free_space_bytes,
        display,
        notes,
    }
}

pub async fn list_ebook_readers() -> AppResult<Vec<EbookReaderDevice>> {
    let mut out = Vec::new();
    out.extend(list_mtp_readers().await?);
    out.extend(list_mounted_readers()?);

    out.sort_by(|a, b| {
        a.family_label
            .cmp(&b.family_label)
            .then_with(|| a.product.cmp(&b.product))
    });
    Ok(out)
}

pub async fn list_kindles() -> AppResult<Vec<KindleDeviceInfo>> {
    let readers = list_ebook_readers().await?;
    Ok(readers
        .into_iter()
        .filter(|d| d.family == ReaderFamily::Kindle)
        .filter_map(|d| {
            let location_id = d.location_id?;
            Some(KindleDeviceInfo {
                location_id,
                manufacturer: d.manufacturer,
                product: d.product,
                serial_number: d.serial_number,
                is_kindle: true,
                free_space_bytes: d.free_space_bytes,
                display: d.display,
            })
        })
        .collect())
}

async fn list_mtp_readers() -> AppResult<Vec<EbookReaderDevice>> {
    let devices = match MtpDevice::list_devices() {
        Ok(d) => d,
        Err(e) => {
            // No USB permission / empty bus should not hard-fail the whole scan.
            if e.to_string().to_ascii_lowercase().contains("no device") {
                return Ok(Vec::new());
            }
            return Err(map_mtp_error(e));
        }
    };

    let mut out = Vec::new();
    for d in devices {
        let manufacturer = d.manufacturer.clone().unwrap_or_else(|| "Unknown".into());
        let product = d.product.clone().unwrap_or_else(|| "Unknown".into());
        let Some(family) = classify_reader(&manufacturer, &product) else {
            continue;
        };

        let mut free_space = None;
        let mut notes = None;
        match MtpDevice::open_by_location(d.location_id).await {
            Ok(device) => {
                if let Ok(storages) = device.storages().await {
                    free_space = storages.first().map(|s| s.info().free_space);
                }
                let _ = device.close().await;
            }
            Err(e) => {
                if e.is_exclusive_access() {
                    notes = Some(
                        "Device is locked by another process (common on macOS with ptpcamerad or Android File Transfer)."
                            .to_string(),
                    );
                } else {
                    notes = Some(format!("Detected, but could not open: {e}"));
                }
            }
        }

        out.push(reader_from_family(
            family,
            mtp_device_id(d.location_id),
            ConnectionKind::Mtp,
            Some(d.location_id),
            None,
            manufacturer,
            product,
            d.serial_number.clone(),
            free_space,
            d.display(),
            notes,
        ));
    }

    Ok(out)
}

fn mtp_device_id(location_id: u64) -> String {
    // Full 64-bit hex. Truncating to 8 digits collides for FNV topology hashes.
    format!("mtp-{location_id:016x}")
}

/// Open an MTP session for a previously listed device.
///
/// Prefer serial when present. Never trust a `location_id` that crossed the
/// JS boundary: FNV topology hashes exceed `Number.MAX_SAFE_INTEGER` and lose
/// precision, which surfaces as "no device found" on send.
async fn open_mtp_for_reader(device: &EbookReaderDevice) -> AppResult<MtpDevice> {
    if let Some(serial) = device
        .serial_number
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        match MtpDevice::open_by_serial(serial).await {
            Ok(mtp) => return Ok(mtp),
            Err(e) if matches!(e, mtp_rs::Error::NoDevice) => {
                // Fall through to a fresh location_id from this list pass.
            }
            Err(e) => return Err(map_mtp_error(e)),
        }
    }

    let location_id = device
        .location_id
        .ok_or_else(|| err("MTP device is missing a location id"))?;
    MtpDevice::open_by_location(location_id)
        .await
        .map_err(map_mtp_error)
}

async fn resolve_mtp_reader(device_id: &str) -> AppResult<EbookReaderDevice> {
    let readers = list_ebook_readers().await?;
    let device = readers
        .into_iter()
        .find(|d| d.id == device_id)
        .ok_or_else(|| {
            err("That eReader is no longer connected. Unplug, replug, and try again.")
        })?;
    if device.connection != ConnectionKind::Mtp {
        return Err(err(
            "USB send is only available for Kindle devices connected over MTP.",
        ));
    }
    Ok(device)
}

pub(crate) fn list_mounted_readers() -> AppResult<Vec<EbookReaderDevice>> {
    let mut out = Vec::new();

    #[cfg(target_os = "windows")]
    {
        for letter in b'D'..=b'Z' {
            let path = PathBuf::from(format!("{}:\\", letter as char));
            if path.exists() {
                if let Some(device) = classify_mount(&path) {
                    out.push(device);
                }
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        for root in candidate_mount_roots() {
            let Ok(entries) = fs::read_dir(&root) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                if let Some(device) = classify_mount(&path) {
                    out.push(device);
                }
            }
        }
    }

    Ok(out)
}

fn candidate_mount_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    #[cfg(target_os = "macos")]
    {
        roots.push(PathBuf::from("/Volumes"));
    }
    #[cfg(target_os = "linux")]
    {
        roots.push(PathBuf::from("/media"));
        roots.push(PathBuf::from("/run/media"));
        roots.push(PathBuf::from("/mnt"));
        if let Ok(user) = std::env::var("USER") {
            roots.push(PathBuf::from(format!("/media/{user}")));
            roots.push(PathBuf::from(format!("/run/media/{user}")));
        }
    }
    roots
}

fn classify_mount(path: &Path) -> Option<EbookReaderDevice> {
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());

    let has_kobo = path.join(".kobo").is_dir() || path.join(".kobo-images").is_dir();
    let has_kindle = path.join("documents").is_dir()
        && (path.join("system").is_dir()
            || path.join("system").join("version.txt").is_file()
            || name.to_ascii_lowercase().contains("kindle"));
    let has_remarkable = path.join(".oxide").is_dir() || path.join("xochitl").is_dir();

    let (family, manufacturer, notes) = if has_kobo {
        (
            ReaderFamily::Kobo,
            "Kobo".to_string(),
            Some("Detected via USB mass-storage mount (.kobo).".to_string()),
        )
    } else if has_kindle {
        (
            ReaderFamily::Kindle,
            "Amazon".to_string(),
            Some("Detected via USB mass-storage mount.".to_string()),
        )
    } else if has_remarkable {
        (
            ReaderFamily::Remarkable,
            "reMarkable".to_string(),
            Some("Detected via USB mass-storage mount.".to_string()),
        )
    } else if let Some(family) = classify_reader("", &name) {
        (
            family,
            family.label().to_string(),
            Some("Detected from mounted volume name.".to_string()),
        )
    } else {
        return None;
    };

    let free = free_space_for_path(path);
    let mount = path.to_string_lossy().to_string();
    Some(reader_from_family(
        family,
        format!("mount-{}", mount),
        ConnectionKind::Mount,
        None,
        Some(mount.clone()),
        manufacturer,
        name.clone(),
        None,
        free,
        format!("{name} ({mount})"),
        notes,
    ))
}

fn free_space_for_path(path: &Path) -> Option<u64> {
    #[cfg(unix)]
    {
        use std::ffi::CString;
        use std::mem::MaybeUninit;
        let c_path = CString::new(path.to_string_lossy().as_bytes()).ok()?;
        let mut stat = MaybeUninit::<libc::statvfs>::uninit();
        let rc = unsafe { libc::statvfs(c_path.as_ptr(), stat.as_mut_ptr()) };
        if rc != 0 {
            return None;
        }
        let stat = unsafe { stat.assume_init() };
        Some(stat.f_bavail as u64 * stat.f_frsize as u64)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        None
    }
}

pub async fn send_file_to_kindle(
    device_id: &str,
    local_path: &Path,
    remote_filename: &str,
) -> AppResult<(String, String)> {
    let reader = resolve_mtp_reader(device_id).await?;
    let device = open_mtp_for_reader(&reader).await?;

    let serial = {
        let s = device.device_info().serial_number.clone();
        if s.is_empty() {
            reader
                .serial_number
                .clone()
                .unwrap_or_else(|| reader.id.clone())
        } else {
            s
        }
    };

    let storages = device.storages().await.map_err(map_mtp_error)?;
    let storage = storages
        .first()
        .ok_or_else(|| err("Kindle reported no storage volumes"))?;

    let documents = find_or_create_documents(storage).await?;

    let mut file = File::open(local_path)
        .await
        .map_err(|e| err(format!("Could not open file for transfer: {e}")))?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)
        .await
        .map_err(|e| err(format!("Could not read file for transfer: {e}")))?;

    let size = buf.len() as u64;
    let info = NewObjectInfo::file(remote_filename, size);
    let stream = stream::iter(vec![Ok::<_, std::io::Error>(Bytes::from(buf))]);

    storage
        .upload(Some(documents), info, Box::pin(stream))
        .await
        .map_err(|e| {
            err(format!(
                "Upload failed: {}. On macOS, quit Android File Transfer and stop ptpcamerad if the device is locked by another process.",
                e.source
            ))
        })?;

    let _ = device.close().await;
    let remote_path = format!("documents/{remote_filename}");
    Ok((remote_path, serial))
}

async fn find_or_create_documents(
    storage: &mtp_rs::mtp::Storage,
) -> AppResult<mtp_rs::mtp::ObjectHandle> {
    let objects = storage.list_objects(None).await.map_err(map_mtp_error)?;

    for obj in objects {
        let name = obj.filename.to_ascii_lowercase();
        if obj.is_folder() && (name == "documents" || name == "docs") {
            return Ok(obj.handle);
        }
    }

    storage
        .create_folder(None, "documents")
        .await
        .map_err(map_mtp_error)
}

pub fn record_sync(
    conn: &Connection,
    book_id: i64,
    device_serial: &str,
    remote_path: &str,
    format: &str,
) -> AppResult<()> {
    conn.execute(
        "INSERT INTO devices_sync (book, device_serial, last_sent_at, remote_path, format)
         VALUES (?1, ?2, datetime('now'), ?3, ?4)
         ON CONFLICT(book, device_serial) DO UPDATE SET
           last_sent_at = excluded.last_sent_at,
           remote_path = excluded.remote_path,
           format = excluded.format",
        params![book_id, device_serial, remote_path, format],
    )?;
    Ok(())
}

pub fn list_sync_records(conn: &Connection) -> AppResult<Vec<SyncRecord>> {
    let mut stmt = conn.prepare(
        "SELECT book, device_serial, last_sent_at, remote_path, format
         FROM devices_sync
         ORDER BY last_sent_at DESC
         LIMIT 100",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(SyncRecord {
            book_id: row.get(0)?,
            device_serial: row.get(1)?,
            last_sent_at: row.get(2)?,
            remote_path: row.get(3)?,
            format: row.get(4)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn map_mtp_error(e: mtp_rs::Error) -> crate::error::AppError {
    let msg = e.to_string();
    if e.is_exclusive_access() {
        err(format!(
            "{msg}. On macOS, another process (often ptpcamerad or Android File Transfer) holds the device. Quit those apps and retry."
        ))
    } else if matches!(e, mtp_rs::Error::NoDevice) {
        err(
            "No MTP device found. Keep the Kindle unlocked on the USB file-transfer screen, quit Android File Transfer if it is open, then unplug and replug.",
        )
    } else {
        err(msg)
    }
}

const MAX_DEVICE_SCAN_DEPTH: usize = 8;
const MAX_DEVICE_BOOKS: usize = 10_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceBook {
    pub title: String,
    pub format: String,
    pub filename: String,
    pub path: String,
    pub size: u64,
    /// MTP object handle (session-local).
    pub handle: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceLibrary {
    pub device_id: String,
    pub device_name: String,
    pub connection: ConnectionKind,
    pub books: Vec<DeviceBook>,
}

pub async fn scan_device_library(device_id: String) -> AppResult<DeviceLibrary> {
    let readers = list_ebook_readers().await?;
    let device = readers
        .into_iter()
        .find(|d| d.id == device_id)
        .ok_or_else(|| {
            err("That eReader is no longer connected. Refresh Devices and try again.")
        })?;

    let mut books = match device.connection {
        ConnectionKind::Mtp => scan_mtp_ebooks(&device).await?,
        ConnectionKind::Mount => {
            let mount = device
                .mount_path
                .as_ref()
                .ok_or_else(|| err("Mounted device is missing a mount path"))?;
            scan_mount_ebooks(mount)?
        }
    };

    books.sort_by(|a, b| {
        a.title
            .to_ascii_lowercase()
            .cmp(&b.title.to_ascii_lowercase())
            .then_with(|| a.format.cmp(&b.format))
    });

    Ok(DeviceLibrary {
        device_id: device.id.clone(),
        device_name: device.product.clone(),
        connection: device.connection,
        books,
    })
}

/// Book identity for device delete. Prefer `path` (stable across MTP sessions).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceBookTarget {
    pub path: String,
    pub filename: String,
    pub handle: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceBulkFailure {
    pub path: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceBulkResult {
    pub removed: i64,
    pub failed: Vec<DeviceBulkFailure>,
}

pub async fn delete_device_books(
    device_id: String,
    books: Vec<DeviceBookTarget>,
) -> AppResult<DeviceBulkResult> {
    if books.is_empty() {
        return Ok(DeviceBulkResult {
            removed: 0,
            failed: Vec::new(),
        });
    }

    let readers = list_ebook_readers().await?;
    let device = readers
        .into_iter()
        .find(|d| d.id == device_id)
        .ok_or_else(|| {
            err("That eReader is no longer connected. Refresh Devices and try again.")
        })?;

    match device.connection {
        ConnectionKind::Mtp => delete_mtp_ebooks(&device, &books).await,
        ConnectionKind::Mount => {
            let mount = device
                .mount_path
                .as_ref()
                .ok_or_else(|| err("Mounted device is missing a mount path"))?;
            delete_mount_ebooks(mount, &books)
        }
    }
}

async fn delete_mtp_ebooks(
    device: &EbookReaderDevice,
    targets: &[DeviceBookTarget],
) -> AppResult<DeviceBulkResult> {
    let wanted: HashSet<&str> = targets.iter().map(|t| t.path.as_str()).collect();
    let mtp = open_mtp_for_reader(device).await?;
    let storages = mtp.storages().await.map_err(map_mtp_error)?;
    let storage = storages
        .first()
        .ok_or_else(|| err("Device reported no storage volumes"))?;

    // Handles from an earlier scan belong to a closed session. Re-resolve by path.
    let mut found: HashMap<String, ObjectHandle> = HashMap::new();
    let mut stack: Vec<(Option<ObjectHandle>, String, usize)> = vec![(None, String::new(), 0)];

    while let Some((parent, path_prefix, depth)) = stack.pop() {
        if found.len() >= wanted.len() || depth > MAX_DEVICE_SCAN_DEPTH {
            continue;
        }

        let objects = storage.list_objects(parent).await.map_err(map_mtp_error)?;
        for obj in objects {
            let is_folder = obj.is_folder();
            let handle = obj.handle;
            let name = obj.filename;
            if name == "." || name == ".." {
                continue;
            }

            let child_path = if path_prefix.is_empty() {
                name.clone()
            } else {
                format!("{path_prefix}/{name}")
            };

            if is_folder {
                if should_skip_device_dir(&name) {
                    continue;
                }
                if depth < MAX_DEVICE_SCAN_DEPTH {
                    stack.push((Some(handle), child_path, depth + 1));
                }
                continue;
            }

            if wanted.contains(child_path.as_str()) {
                found.insert(child_path, handle);
                if found.len() >= wanted.len() {
                    break;
                }
            }
        }
    }

    let mut removed = 0i64;
    let mut failed = Vec::new();
    for target in targets {
        let Some(handle) = found.get(&target.path).copied() else {
            failed.push(DeviceBulkFailure {
                path: target.path.clone(),
                error: format!("Could not find \"{}\" on the device.", target.filename),
            });
            continue;
        };
        match storage.delete(handle).await {
            Ok(()) => removed += 1,
            Err(e) => failed.push(DeviceBulkFailure {
                path: target.path.clone(),
                error: map_mtp_error(e).to_string(),
            }),
        }
    }

    let _ = mtp.close().await;
    Ok(DeviceBulkResult { removed, failed })
}

fn delete_mount_ebooks(mount: &str, targets: &[DeviceBookTarget]) -> AppResult<DeviceBulkResult> {
    let canonical_mount = PathBuf::from(mount)
        .canonicalize()
        .map_err(|e| err(format!("Could not open device mount: {e}")))?;

    let mut removed = 0i64;
    let mut failed = Vec::new();
    for target in targets {
        if target.path.is_empty()
            || target.path.contains("..")
            || Path::new(&target.path).is_absolute()
        {
            failed.push(DeviceBulkFailure {
                path: target.path.clone(),
                error: "Invalid device path.".to_string(),
            });
            continue;
        }

        let candidate = canonical_mount.join(&target.path);
        let canonical = match candidate.canonicalize() {
            Ok(path) => path,
            Err(_) => {
                failed.push(DeviceBulkFailure {
                    path: target.path.clone(),
                    error: format!("Could not find \"{}\" on the device.", target.filename),
                });
                continue;
            }
        };

        if !canonical.starts_with(&canonical_mount) {
            failed.push(DeviceBulkFailure {
                path: target.path.clone(),
                error: "Invalid device path.".to_string(),
            });
            continue;
        }

        if !canonical.is_file() {
            failed.push(DeviceBulkFailure {
                path: target.path.clone(),
                error: format!("\"{}\" is not a removable file.", target.filename),
            });
            continue;
        }

        match fs::remove_file(&canonical) {
            Ok(()) => removed += 1,
            Err(e) => failed.push(DeviceBulkFailure {
                path: target.path.clone(),
                error: format!("Could not delete \"{}\": {e}", target.filename),
            }),
        }
    }

    Ok(DeviceBulkResult { removed, failed })
}

fn ebook_extension(name: &str) -> Option<&'static str> {
    let lower = name.to_ascii_lowercase();
    let ext = Path::new(&lower).extension()?.to_str()?;
    match ext {
        "epub" => Some("EPUB"),
        "kepub" => Some("KEPUB"),
        "pdf" => Some("PDF"),
        "azw3" => Some("AZW3"),
        "azw" => Some("AZW"),
        "mobi" => Some("MOBI"),
        "prc" => Some("PRC"),
        "fb2" => Some("FB2"),
        "txt" => Some("TXT"),
        "cbz" => Some("CBZ"),
        "cbr" => Some("CBR"),
        "djvu" => Some("DJVU"),
        _ => None,
    }
}

fn title_from_filename(filename: &str) -> String {
    let stem = Path::new(filename)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| filename.to_string());
    let cleaned = stem.replace('_', " ").replace("  ", " ");
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        filename.to_string()
    } else {
        cleaned.to_string()
    }
}

fn should_skip_device_dir(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if lower.starts_with('.') {
        return true;
    }
    matches!(
        lower.as_str(),
        "system"
            | "android"
            | "dcim"
            | "lost.dir"
            | "alarms"
            | "notifications"
            | "ringtones"
            | "podcasts"
            | "audiobooks"
            | "music"
            | "pictures"
            | "movies"
            | "itunes_control"
            | "sdr"
            | ".sdr"
            | "thumbnails"
            | "cache"
            | "tmp"
            | "temp"
            | "fonts"
            | "dictionaries"
            | "voice"
            | "sounds"
    )
}

fn device_book_from_file(filename: &str, path: String, size: u64, handle: Option<u64>) -> Option<DeviceBook> {
    let format = ebook_extension(filename)?.to_string();
    Some(DeviceBook {
        title: title_from_filename(filename),
        format,
        filename: filename.to_string(),
        path,
        size,
        handle,
    })
}

async fn scan_mtp_ebooks(device: &EbookReaderDevice) -> AppResult<Vec<DeviceBook>> {
    let mtp = open_mtp_for_reader(device).await?;
    let storages = mtp.storages().await.map_err(map_mtp_error)?;
    let storage = storages
        .first()
        .ok_or_else(|| err("Device reported no storage volumes"))?;

    let mut books = Vec::new();
    let mut stack: Vec<(Option<mtp_rs::mtp::ObjectHandle>, String, usize)> =
        vec![(None, String::new(), 0)];

    while let Some((parent, path_prefix, depth)) = stack.pop() {
        if books.len() >= MAX_DEVICE_BOOKS || depth > MAX_DEVICE_SCAN_DEPTH {
            continue;
        }

        let objects = storage.list_objects(parent).await.map_err(map_mtp_error)?;
        for obj in objects {
            let is_folder = obj.is_folder();
            let handle = obj.handle;
            let size = obj.size;
            let name = obj.filename;
            if name == "." || name == ".." {
                continue;
            }

            let child_path = if path_prefix.is_empty() {
                name.clone()
            } else {
                format!("{path_prefix}/{name}")
            };

            if is_folder {
                if should_skip_device_dir(&name) {
                    continue;
                }
                if depth < MAX_DEVICE_SCAN_DEPTH && books.len() < MAX_DEVICE_BOOKS {
                    stack.push((Some(handle), child_path, depth + 1));
                }
                continue;
            }

            if let Some(book) = device_book_from_file(&name, child_path, size, Some(handle.0)) {
                books.push(book);
                if books.len() >= MAX_DEVICE_BOOKS {
                    break;
                }
            }
        }
    }

    let _ = mtp.close().await;
    Ok(books)
}

fn scan_mount_ebooks(mount: &str) -> AppResult<Vec<DeviceBook>> {
    let mount_root = PathBuf::from(mount);
    let canonical_mount = mount_root
        .canonicalize()
        .map_err(|e| err(format!("Could not open device mount: {e}")))?;

    let mut books = Vec::new();
    let mut stack: Vec<(PathBuf, String, usize)> = vec![(canonical_mount.clone(), String::new(), 0)];

    while let Some((dir, path_prefix, depth)) = stack.pop() {
        if books.len() >= MAX_DEVICE_BOOKS || depth > MAX_DEVICE_SCAN_DEPTH {
            continue;
        }

        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name == "." || name == ".." {
                continue;
            }

            let child_path = if path_prefix.is_empty() {
                name.clone()
            } else {
                format!("{path_prefix}/{name}")
            };

            let meta = match entry.metadata() {
                Ok(meta) => meta,
                Err(_) => continue,
            };

            if meta.is_dir() {
                if should_skip_device_dir(&name) {
                    continue;
                }
                let child = entry.path();
                let Ok(canonical_child) = child.canonicalize() else {
                    continue;
                };
                if !canonical_child.starts_with(&canonical_mount) {
                    continue;
                }
                if depth < MAX_DEVICE_SCAN_DEPTH && books.len() < MAX_DEVICE_BOOKS {
                    stack.push((canonical_child, child_path, depth + 1));
                }
                continue;
            }

            if !meta.is_file() {
                continue;
            }

            if let Some(book) = device_book_from_file(&name, child_path, meta.len(), None) {
                books.push(book);
                if books.len() >= MAX_DEVICE_BOOKS {
                    break;
                }
            }
        }
    }

    Ok(books)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_common_readers() {
        assert_eq!(
            classify_reader("Amazon", "Kindle Colorsoft"),
            Some(ReaderFamily::Kindle)
        );
        assert_eq!(
            classify_reader("", "Kindle Paperwhite"),
            Some(ReaderFamily::Kindle)
        );
        assert_eq!(
            classify_reader("Kobo", "Clara Colour"),
            Some(ReaderFamily::Kobo)
        );
        assert_eq!(
            classify_reader("Rakuten", "Libra Colour"),
            Some(ReaderFamily::Kobo)
        );
        assert_eq!(
            classify_reader("PocketBook", "Era Color"),
            Some(ReaderFamily::Pocketbook)
        );
        assert_eq!(
            classify_reader("reMarkable", "reMarkable 2"),
            Some(ReaderFamily::Remarkable)
        );
        assert_eq!(
            classify_reader("Onyx", "BOOX Palma"),
            Some(ReaderFamily::Boox)
        );
        assert_eq!(
            classify_reader("tolino", "vision 6"),
            Some(ReaderFamily::Tolino)
        );
        assert_eq!(
            classify_reader("Barnes & Noble", "Nook GlowLight"),
            Some(ReaderFamily::Nook)
        );
        assert_eq!(
            classify_reader("Acme", "My eReader"),
            Some(ReaderFamily::Other)
        );
        assert_eq!(classify_reader("Apple", "iPhone"), None);
    }

    #[test]
    fn kindle_is_only_usb_send_family() {
        assert!(ReaderFamily::Kindle.supports_usb_send());
        assert!(!ReaderFamily::Kobo.supports_usb_send());
        assert!(!ReaderFamily::Pocketbook.supports_usb_send());
        assert!(!ReaderFamily::Remarkable.supports_usb_send());
        assert!(!ReaderFamily::Boox.supports_usb_send());
        assert!(!ReaderFamily::Tolino.supports_usb_send());
        assert!(!ReaderFamily::Nook.supports_usb_send());
        assert!(!ReaderFamily::Other.supports_usb_send());
    }

    #[test]
    fn preferred_formats_match_family() {
        assert_eq!(
            ReaderFamily::Kindle.preferred_formats(),
            vec!["AZW3", "MOBI", "PDF"]
        );
        assert_eq!(
            ReaderFamily::Kobo.preferred_formats(),
            vec!["EPUB", "KEPUB", "PDF"]
        );
        assert_eq!(
            ReaderFamily::Pocketbook.preferred_formats(),
            vec!["EPUB", "PDF", "FB2"]
        );
    }

    #[test]
    fn detects_ebook_extensions() {
        assert_eq!(ebook_extension("Book.epub"), Some("EPUB"));
        assert_eq!(ebook_extension("Book.AZW3"), Some("AZW3"));
        assert_eq!(ebook_extension("notes.txt"), Some("TXT"));
        assert_eq!(ebook_extension("cover.jpg"), None);
    }

    #[test]
    fn titles_from_filenames() {
        assert_eq!(title_from_filename("Dune_Messiah.mobi"), "Dune Messiah");
        assert_eq!(title_from_filename("chapter.pdf"), "chapter");
    }

    #[test]
    fn skips_system_device_dirs() {
        assert!(should_skip_device_dir(".hidden"));
        assert!(should_skip_device_dir("system"));
        assert!(should_skip_device_dir("SDR"));
        assert!(should_skip_device_dir("thumbnails"));
        assert!(!should_skip_device_dir("documents"));
        assert!(!should_skip_device_dir("Books"));
    }
}

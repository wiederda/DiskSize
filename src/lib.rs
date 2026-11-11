//! Gemeinsame Logik für die GUI und die CLI: Ordner scannen und nach Excel
//! exportieren.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use rayon::prelude::*;
use rust_xlsxwriter::{Color as XlsxColor, Format, FormatAlign, Workbook, Worksheet, XlsxError};

// ================== FOLDER ENTRY ==================

#[derive(Clone)]
pub struct FolderNode {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
    pub children: Vec<FolderNode>,
    pub is_scanned: bool,
    pub has_access: bool,
    pub expanded: bool,
}

impl FolderNode {
    pub fn placeholder(path: PathBuf) -> Self {
        Self {
            name: node_name(&path),
            path,
            size: 0,
            children: Vec::new(),
            is_scanned: false,
            has_access: true,
            expanded: false,
        }
    }
}

fn node_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

// ================== SCAN PROGRESS ==================

#[derive(Default)]
pub struct ScanProgress {
    pub directories: AtomicUsize,
    pub files: AtomicUsize,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ScanStats {
    pub files: usize,
    pub directories: usize,
}

// ================== SCAN ==================

pub fn list_top_level_folders(root: &str) -> Vec<FolderNode> {
    let mut folders = Vec::new();

    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            let is_dir = entry
                .file_type()
                .map(|file_type| file_type.is_dir() && !file_type.is_symlink())
                .unwrap_or(false);

            if is_dir {
                folders.push(FolderNode::placeholder(entry.path()));
            }
        }
    }

    folders.sort_by(|a, b| a.path.cmp(&b.path));

    folders
}

/// Summe der Dateien, die direkt im Verzeichnis liegen (nicht rekursiv).
pub fn direct_files_size(path: &Path) -> u64 {
    fs::read_dir(path)
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|entry| {
                    let file_type = entry.file_type().ok()?;
                    if file_type.is_file() {
                        entry.metadata().ok().map(|m| m.len())
                    } else {
                        None
                    }
                })
                .sum::<u64>()
        })
        .unwrap_or(0)
}

pub fn scan_directory(path: &Path) -> FolderNode {
    scan_directory_with_stats(path).0
}

/// Scannt einen Ordner rekursiv und liefert zusätzlich Datei- und Ordnerzahlen.
pub fn scan_directory_with_stats(path: &Path) -> (FolderNode, ScanStats) {
    let cancel = AtomicBool::new(false);
    let progress = ScanProgress::default();

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build()
        .unwrap();

    let node = pool.install(|| scan_directory_cancellable(path, &cancel, &progress));

    let stats = ScanStats {
        files: progress.files.load(Ordering::Relaxed),
        directories: progress.directories.load(Ordering::Relaxed),
    };

    (node, stats)
}

pub fn scan_directory_cancellable(
    path: &Path,
    cancel: &AtomicBool,
    progress: &ScanProgress,
) -> FolderNode {
    let mut children = Vec::new();

    if cancel.load(Ordering::Relaxed) {
        return FolderNode {
            path: path.to_path_buf(),
            name: node_name(path),
            size: 0,
            children,
            is_scanned: false,
            has_access: true,
            expanded: false,
        };
    }

    progress.directories.fetch_add(1, Ordering::Relaxed);

    let entries = match fs::read_dir(path) {
        Ok(entries) => entries.flatten().collect::<Vec<_>>(),
        Err(_) => {
            return FolderNode {
                path: path.to_path_buf(),
                name: node_name(path),
                size: 0,
                children,
                is_scanned: true,
                has_access: false,
                expanded: false,
            };
        }
    };

    let results: Vec<(u64, Option<FolderNode>)> = entries
        .into_par_iter()
        .filter_map(|entry| {
            if cancel.load(Ordering::Relaxed) {
                return None;
            }

            let file_type = entry.file_type().ok()?;

            if file_type.is_symlink() {
                return None;
            }

            if file_type.is_file() {
                progress.files.fetch_add(1, Ordering::Relaxed);

                let size = entry.metadata().ok()?.len();

                Some((size, None))
            } else if file_type.is_dir() {
                let child = scan_directory_cancellable(&entry.path(), cancel, progress);

                Some((child.size, Some(child)))
            } else {
                None
            }
        })
        .collect();

    let mut total_size = 0u64;

    for (size, child) in results {
        total_size = total_size.saturating_add(size);

        if let Some(child) = child {
            children.push(child);
        }
    }

    children.sort_by_key(|child| std::cmp::Reverse(child.size));

    FolderNode {
        path: path.to_path_buf(),
        name: node_name(path),
        size: total_size,
        children,
        is_scanned: !cancel.load(Ordering::Relaxed),
        has_access: true,
        expanded: false,
    }
}

// ================== UTILS ==================

pub fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes < KB {
        format!("{bytes} B")
    } else if bytes < MB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else if bytes < GB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    }
}

pub fn format_count(value: usize) -> String {
    let digits = value.to_string();
    let mut formatted = String::with_capacity(digits.len() + digits.len() / 3);

    for (index, character) in digits.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            formatted.push('.');
        }
        formatted.push(character);
    }

    formatted.chars().rev().collect()
}

pub fn count_folders(folders: &[FolderNode]) -> usize {
    folders
        .iter()
        .map(|folder| 1 + count_folders(&folder.children))
        .sum()
}

// ================== EXCEL EXPORT ==================

const EXPORT_HEADER_ROW: u32 = 5;
const EXPORT_FIRST_DATA_ROW: u32 = 6;
const EXCEL_MAX_ROWS: u32 = 1_048_576;

/// Ein Arbeitsblatt im Excel-Export. Jedes Blatt gehört zu einem Ordner.
pub struct ExportSheet<'a> {
    pub name: String,
    pub root_path: String,
    pub total_size: u64,
    pub folders: &'a [FolderNode],
}

struct ExportFormats {
    title: Format,
    label: Format,
    header: Format,
    bytes: Format,
    percent: Format,
    denied: Format,
}

pub fn default_export_filename(root: &str) -> String {
    let label: String = root
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '_'
            }
        })
        .collect();

    let label = label.trim_matches('_');
    let label = if label.is_empty() { "Export" } else { label };

    format!("DiskSize_{label}.xlsx")
}

pub fn with_xlsx_extension(mut path: PathBuf) -> PathBuf {
    let has_xlsx_extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.eq_ignore_ascii_case("xlsx"))
        .unwrap_or(false);

    if !has_xlsx_extension {
        path.set_extension("xlsx");
    }

    path
}

/// Macht aus einem Ordnernamen einen gültigen Excel-Blattnamen.
///
/// Excel erlaubt maximal 31 Zeichen und keine der Zeichen `: \ / ? * [ ]`.
pub fn sanitize_sheet_name(name: &str) -> String {
    let mut cleaned: String = name
        .chars()
        .map(|character| match character {
            ':' | '\\' | '/' | '?' | '*' | '[' | ']' => '_',
            character if character.is_control() => '_',
            character => character,
        })
        .collect();

    cleaned = cleaned.trim().trim_matches('\'').trim().to_string();

    if cleaned.is_empty() {
        cleaned = "Ordner".to_string();
    } else if cleaned.eq_ignore_ascii_case("history") {
        // "History" ist in Excel reserviert.
        cleaned.push('_');
    }

    cleaned.chars().take(31).collect()
}

/// Liefert einen eindeutigen Blattnamen und merkt ihn in `used` vor.
pub fn unique_sheet_name(base: &str, used: &mut HashSet<String>) -> String {
    let base = sanitize_sheet_name(base);
    let mut counter = 1;

    loop {
        let candidate = if counter == 1 {
            base.clone()
        } else {
            let suffix = format!(" ({counter})");
            let max = 31usize.saturating_sub(suffix.chars().count());
            let stem: String = base.chars().take(max).collect();
            format!("{stem}{suffix}")
        };

        if used.insert(candidate.to_lowercase()) {
            return candidate;
        }

        counter += 1;
    }
}

/// Schreibt eine Arbeitsmappe mit einem Blatt pro Eintrag und liefert die
/// Gesamtzahl der exportierten Ordner zurück.
pub fn write_excel_export(
    path: &Path,
    sheets: &[ExportSheet<'_>],
) -> Result<usize, Box<dyn std::error::Error>> {
    if sheets.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Keine Ordner für den Export angegeben.",
        )
        .into());
    }

    let mut workbook = Workbook::new();
    let formats = export_formats();
    let mut total_folders = 0;

    for spec in sheets {
        let sheet = workbook.add_worksheet();
        sheet.set_name(spec.name.as_str())?;
        total_folders += write_excel_sheet(sheet, spec, &formats)?;
    }

    workbook.save(path)?;

    Ok(total_folders)
}

fn export_formats() -> ExportFormats {
    ExportFormats {
        title: Format::new().set_bold().set_font_size(14.0),
        label: Format::new().set_bold(),
        header: Format::new()
            .set_bold()
            .set_font_color(XlsxColor::White)
            .set_background_color(XlsxColor::RGB(0x21_96_F3))
            .set_align(FormatAlign::Center)
            .set_text_wrap(),
        bytes: Format::new().set_num_format("#,##0"),
        percent: Format::new().set_num_format("0.0%"),
        denied: Format::new()
            .set_italic()
            .set_font_color(XlsxColor::RGB(0xB7_1C_1C)),
    }
}

fn write_excel_sheet(
    sheet: &mut Worksheet,
    spec: &ExportSheet<'_>,
    formats: &ExportFormats,
) -> Result<usize, XlsxError> {
    let folder_count = count_folders(spec.folders);

    sheet.merge_range(0, 0, 0, 6, "DiskSize – Ordnerübersicht", &formats.title)?;
    sheet.set_row_height(0, 22.0)?;

    sheet.write_string_with_format(1, 0, "Quelle:", &formats.label)?;
    sheet.write_string(1, 1, &spec.root_path)?;

    sheet.write_string_with_format(2, 0, "Gesamtgröße:", &formats.label)?;
    sheet.write_number_with_format(2, 1, spec.total_size as f64, &formats.bytes)?;
    sheet.write_string(2, 2, format_size(spec.total_size))?;

    sheet.write_string_with_format(3, 0, "Ordner:", &formats.label)?;
    sheet.write_number(3, 1, folder_count as f64)?;

    let headers = [
        "Ebene",
        "Ordner",
        "Pfad",
        "Größe (Bytes)",
        "Größe",
        "Anteil",
        "Status",
    ];

    for (column, header) in headers.iter().enumerate() {
        sheet.write_string_with_format(
            EXPORT_HEADER_ROW,
            column as u16,
            *header,
            &formats.header,
        )?;
    }

    let mut row = EXPORT_FIRST_DATA_ROW;

    // Stammzeile
    sheet.write_number(row, 0, 0.0)?;
    sheet.write_string(row, 1, &spec.root_path)?;
    sheet.write_string(row, 2, &spec.root_path)?;
    sheet.write_number_with_format(row, 3, spec.total_size as f64, &formats.bytes)?;
    sheet.write_string(row, 4, format_size(spec.total_size))?;
    sheet.write_number_with_format(row, 5, 1.0, &formats.percent)?;
    row += 1;

    write_excel_rows(sheet, &mut row, spec.folders, 1, spec.total_size, formats)?;

    if row > EXPORT_FIRST_DATA_ROW {
        sheet.autofilter(EXPORT_HEADER_ROW, 0, row - 1, 6)?;
    }

    sheet.set_freeze_panes(EXPORT_FIRST_DATA_ROW, 0)?;

    sheet.set_column_width(0, 7.0)?;
    sheet.set_column_width(1, 30.0)?;
    sheet.set_column_width(2, 70.0)?;
    sheet.set_column_width(3, 16.0)?;
    sheet.set_column_width(4, 12.0)?;
    sheet.set_column_width(5, 9.0)?;
    sheet.set_column_width(6, 20.0)?;

    Ok(folder_count)
}

fn write_excel_rows(
    sheet: &mut Worksheet,
    row: &mut u32,
    folders: &[FolderNode],
    level: u32,
    total_size: u64,
    formats: &ExportFormats,
) -> Result<(), XlsxError> {
    for folder in folders {
        // Excel erlaubt maximal 1.048.576 Zeilen.
        if *row >= EXCEL_MAX_ROWS {
            return Ok(());
        }

        let share = if total_size > 0 {
            folder.size as f64 / total_size as f64
        } else {
            0.0
        };

        sheet.write_number(*row, 0, level as f64)?;

        if folder.has_access {
            sheet.write_string(*row, 1, &folder.name)?;
            sheet.write_string(*row, 2, folder.path.to_string_lossy())?;
            sheet.write_string(*row, 6, "")?;
        } else {
            sheet.write_string_with_format(*row, 1, &folder.name, &formats.denied)?;
            sheet.write_string_with_format(
                *row,
                2,
                folder.path.to_string_lossy(),
                &formats.denied,
            )?;
            sheet.write_string_with_format(*row, 6, "Zugriff verweigert", &formats.denied)?;
        }

        sheet.write_number_with_format(*row, 3, folder.size as f64, &formats.bytes)?;
        sheet.write_string(*row, 4, format_size(folder.size))?;
        sheet.write_number_with_format(*row, 5, share, &formats.percent)?;

        *row += 1;

        write_excel_rows(sheet, row, &folder.children, level + 1, total_size, formats)?;
    }

    Ok(())
}

// ================== TESTS ==================

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_dir(prefix: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        std::env::temp_dir().join(format!("{prefix}-{}-{unique}", std::process::id()))
    }

    #[test]
    fn formats_sizes() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(1023), "1023 B");
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1024 * 1024), "1.0 MB");
        assert_eq!(format_size(1024 * 1024 * 1024), "1.0 GB");
    }

    #[test]
    fn scans_nested_directories() {
        let root = unique_dir("disksize-rust-test");

        fs::create_dir_all(root.join("a").join("b")).unwrap();

        fs::write(root.join("root.txt"), vec![0u8; 10]).unwrap();
        fs::write(root.join("a").join("x.txt"), vec![0u8; 20]).unwrap();
        fs::write(root.join("a").join("b").join("y.txt"), vec![0u8; 30]).unwrap();

        let result = scan_directory(&root);

        assert!(result.is_scanned);
        assert!(result.has_access);
        assert_eq!(result.size, 60);
        assert_eq!(result.children.len(), 1);
        assert_eq!(result.children[0].size, 50);
        assert_eq!(result.children[0].children.len(), 1);
        assert_eq!(result.children[0].children[0].size, 30);

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn cancellation_aborts_scan() {
        let cancel = AtomicBool::new(true);
        let progress = ScanProgress::default();

        let node = scan_directory_cancellable(Path::new("."), &cancel, &progress);

        assert!(!node.is_scanned);
        assert_eq!(node.size, 0);
    }

    #[test]
    fn lists_only_directories() {
        let root = unique_dir("disksize-rust-list-test");

        fs::create_dir_all(root.join("folder")).unwrap();
        fs::write(root.join("file.txt"), b"x").unwrap();

        let folders = list_top_level_folders(root.to_str().unwrap());

        assert_eq!(folders.len(), 1);
        assert_eq!(folders[0].name, "folder");

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn counts_direct_files_only() {
        let root = unique_dir("disksize-rust-direct-test");

        fs::create_dir_all(root.join("sub")).unwrap();
        fs::write(root.join("a.txt"), vec![0u8; 7]).unwrap();
        fs::write(root.join("sub").join("b.txt"), vec![0u8; 100]).unwrap();

        assert_eq!(direct_files_size(&root), 7);

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn exports_excel_workbook() {
        let root = unique_dir("disksize-rust-export-test");

        fs::create_dir_all(root.join("a")).unwrap();
        fs::write(root.join("a").join("x.txt"), vec![0u8; 42]).unwrap();

        let tree = scan_directory(&root);
        let total = tree.size;
        let file = root.join("export.xlsx");

        let count = write_excel_export(
            &file,
            &[ExportSheet {
                name: "Ordner".to_string(),
                root_path: root.to_string_lossy().into_owned(),
                total_size: total,
                folders: &tree.children,
            }],
        )
        .unwrap();

        assert_eq!(count, 1);
        assert!(file.exists());

        // xlsx-Dateien sind ZIP-Archive und beginnen mit "PK".
        let bytes = fs::read(&file).unwrap();
        assert!(bytes.len() > 2);
        assert_eq!(&bytes[0..2], b"PK");

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn sanitizes_sheet_names() {
        assert_eq!(sanitize_sheet_name("Projekte"), "Projekte");
        assert_eq!(sanitize_sheet_name("a:b\\c/d?e*f[g]"), "a_b_c_d_e_f_g_");
        assert_eq!(sanitize_sheet_name(""), "Ordner");
        assert_eq!(sanitize_sheet_name("History"), "History_");
        assert_eq!(sanitize_sheet_name(&"x".repeat(40)).chars().count(), 31);
    }

    #[test]
    fn deduplicates_sheet_names() {
        let mut used = HashSet::new();

        assert_eq!(unique_sheet_name("Downloads", &mut used), "Downloads");
        assert_eq!(unique_sheet_name("Downloads", &mut used), "Downloads (2)");
        assert_eq!(unique_sheet_name("downloads", &mut used), "downloads (3)");
    }
}

use std::collections::HashSet;
use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use disksize::{
    ExportSheet, FolderNode, default_export_filename, format_count, format_size,
    scan_directory_with_stats, unique_sheet_name, with_xlsx_extension, write_excel_export,
};

struct Options {
    folders: Vec<PathBuf>,
    output: Option<PathBuf>,
    quiet: bool,
}

fn main() -> ExitCode {
    let options = match parse_args() {
        Ok(options) => options,
        Err(message) => {
            eprintln!("Fehler: {message}");
            eprintln!();
            print_usage();
            return ExitCode::from(2);
        }
    };

    match run(options) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("Fehler: {message}");
            ExitCode::FAILURE
        }
    }
}

fn parse_args() -> Result<Options, String> {
    let mut folders = Vec::new();
    let mut output = None;
    let mut quiet = false;
    let mut parse_options = true;
    let mut args = env::args().skip(1);

    while let Some(argument) = args.next() {
        if !parse_options {
            folders.push(PathBuf::from(argument));
            continue;
        }

        match argument.as_str() {
            "--" => parse_options = false,
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            "-V" | "--version" => {
                println!("DiskSize CLI {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            "-q" | "--quiet" => quiet = true,
            "-o" | "--output" => {
                let value = args.next().ok_or("Nach -o/--output fehlt der Dateiname.")?;
                output = Some(PathBuf::from(value));
            }
            _ if argument.starts_with("--output=") => {
                let value = argument.trim_start_matches("--output=");

                if value.is_empty() {
                    return Err("Nach --output= fehlt der Dateiname.".to_string());
                }

                output = Some(PathBuf::from(value));
            }
            _ if argument.starts_with('-') => {
                return Err(format!("Unbekannte Option: {argument}"));
            }
            _ => folders.push(PathBuf::from(argument)),
        }
    }

    if folders.is_empty() {
        return Err("Bitte mindestens einen Ordner angeben.".to_string());
    }

    Ok(Options {
        folders,
        output,
        quiet,
    })
}

fn run(options: Options) -> Result<(), String> {
    let total = options.folders.len();
    let mut results = Vec::with_capacity(total);

    for (index, input) in options.folders.iter().enumerate() {
        let path =
            std::path::absolute(input).map_err(|error| format!("{}: {error}", input.display()))?;

        if !path.is_dir() {
            return Err(format!("Kein Ordner: {}", path.display()));
        }

        if !options.quiet {
            eprintln!("[{}/{}] Scanne {} ...", index + 1, total, path.display());
        }

        let (node, stats) = scan_directory_with_stats(&path);

        if !options.quiet {
            let datei = if stats.files == 1 { "Datei" } else { "Dateien" };
            eprintln!(
                "      {} {}, {} Ordner, {}",
                format_count(stats.files),
                datei,
                format_count(stats.directories),
                format_size(node.size)
            );
        }

        results.push((path, node));
    }

    let output = options
        .output
        .unwrap_or_else(|| default_output_path(&results));
    let output = with_xlsx_extension(output);

    let mut used_sheet_names = HashSet::new();
    let sheets: Vec<ExportSheet<'_>> = results
        .iter()
        .map(|(path, node)| ExportSheet {
            name: unique_sheet_name(&sheet_base_name(path), &mut used_sheet_names),
            root_path: path.to_string_lossy().into_owned(),
            total_size: node.size,
            folders: &node.children,
        })
        .collect();

    let folder_count = write_excel_export(&output, &sheets).map_err(|error| error.to_string())?;

    if !options.quiet {
        let blatt = if sheets.len() == 1 {
            "Blatt"
        } else {
            "Blätter"
        };
        eprintln!(
            "Excel-Export gespeichert: {} ({} {}, {} Ordner)",
            output.display(),
            sheets.len(),
            blatt,
            format_count(folder_count)
        );
    }

    Ok(())
}

fn default_output_path(results: &[(PathBuf, FolderNode)]) -> PathBuf {
    if results.len() == 1 {
        let label = results[0]
            .0
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Export".to_string());

        PathBuf::from(default_export_filename(&label))
    } else {
        PathBuf::from("DiskSize_Export.xlsx")
    }
}

/// Ermittelt einen lesbaren Blattnamen aus dem Ordnerpfad.
fn sheet_base_name(path: &Path) -> String {
    if let Some(name) = path.file_name() {
        return name.to_string_lossy().into_owned();
    }

    let text = path.to_string_lossy();
    let trimmed = text.trim_end_matches(['\\', '/']);

    if trimmed.len() == 2 && trimmed.ends_with(':') {
        trimmed[..1].to_string()
    } else if trimmed.is_empty() {
        "Ordner".to_string()
    } else {
        trimmed.to_string()
    }
}

fn print_usage() {
    eprintln!("Verwendung: disksize-cli [OPTIONEN] <ORDNER>...");
}

fn print_help() {
    println!(
        "DiskSize CLI – Ordner scannen und als Excel-Datei exportieren

Verwendung:
  disksize-cli [OPTIONEN] <ORDNER>...

Argumente:
  <ORDNER>...            Ein oder mehrere Ordner, die gescannt werden sollen

Optionen:
  -o, --output <DATEI>   Zieldatei (Standard: DiskSize_<Ordner>.xlsx)
  -q, --quiet            Keine Statusmeldungen ausgeben
  -h, --help             Diese Hilfe anzeigen
  -V, --version          Version anzeigen

Beispiele:
  disksize-cli C:\\Users
  disksize-cli \"C:\\Users\" \"D:\\Projekte\" -o Bericht.xlsx"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sheet_name_uses_folder_name() {
        let path = std::env::temp_dir().join("DiskSizeTestFolder");

        assert_eq!(sheet_base_name(&path), "DiskSizeTestFolder");
    }

    #[cfg(windows)]
    #[test]
    fn sheet_name_for_drive_root() {
        assert_eq!(sheet_base_name(Path::new("C:\\")), "C");
    }
}

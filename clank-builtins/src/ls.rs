use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use brush_core::{ExecutionResult, commands::ExecutionContext};
use clap::Parser;

use crate::color;

// ── Named types ───────────────────────────────────────────────────────────────

/// A single entry to be listed, carrying all display-relevant information.
struct ListEntry {
    /// The name shown in output (filename only, not full path).
    display_name: String,
    /// Full path used to read metadata.
    path: PathBuf,
    /// Whether this entry is a directory.
    is_dir: bool,
    /// Whether this entry is a symlink.
    is_symlink: bool,
    /// File size in bytes.
    size_bytes: u64,
    /// Last modification time.
    modified: Option<SystemTime>,
}

/// The resolved display options derived from CLI flags.
struct DisplayOptions {
    show_hidden: bool,
    long_format: bool,
    recursive: bool,
}

// ── CLI definition ────────────────────────────────────────────────────────────

/// clank's internal implementation of `ls`.
///
/// Lists directory contents using std::fs — no OS process is spawned.
/// Compiles to wasm32-wasip2.
#[derive(Debug, Parser)]
#[command(disable_help_flag = true)]
pub struct LsCommand {
    /// Show hidden entries (names starting with '.')
    #[arg(short = 'a')]
    show_hidden: bool,

    /// Long format: permissions, size, date, name
    #[arg(short = 'l')]
    long_format: bool,

    /// Recursive listing
    #[arg(short = 'R')]
    recursive: bool,

    /// Path(s) to list. Defaults to current directory.
    #[arg()]
    paths: Vec<String>,
}

impl brush_core::builtins::Command for LsCommand {
    type Error = brush_core::Error;

    async fn execute(
        &self,
        context: ExecutionContext<'_>,
    ) -> Result<ExecutionResult, Self::Error> {
        let opts = DisplayOptions {
            show_hidden: self.show_hidden,
            long_format: self.long_format,
            recursive: self.recursive,
        };

        let paths: Vec<PathBuf> = if self.paths.is_empty() {
            vec![PathBuf::from(".")]
        } else {
            self.paths.iter().map(PathBuf::from).collect()
        };

        let mut stdout = context.stdout();
        let mut stderr = context.stderr();
        let mut had_error = false;

        let show_header = paths.len() > 1;

        for path in &paths {
            if show_header {
                writeln!(stdout, "{}:", path.display()).ok();
            }
            if let Err(e) = list_path(path, &opts, &mut stdout) {
                writeln!(stderr, "{}ls:{} {}: {e}", color::CMD, color::RESET, path.display()).ok();
                had_error = true;
            }
            if show_header {
                writeln!(stdout).ok();
            }
        }

        if had_error {
            Ok(ExecutionResult::new(1))
        } else {
            Ok(ExecutionResult::success())
        }
    }
}

// ── Core listing logic ────────────────────────────────────────────────────────

fn list_path(
    path: &Path,
    opts: &DisplayOptions,
    stdout: &mut dyn Write,
) -> std::io::Result<()> {
    if opts.recursive {
        list_recursive(path, opts, stdout)
    } else {
        let entries = collect_entries(path, opts)?;
        write_entries(&entries, opts, stdout);
        Ok(())
    }
}

fn list_recursive(
    path: &Path,
    opts: &DisplayOptions,
    stdout: &mut dyn Write,
) -> std::io::Result<()> {
    use walkdir::WalkDir;

    // The root directory's contents are printed without a header — matching
    // OS ls -R behaviour: only subdirectories get a "path:" header.
    let root_entries = collect_entries(path, opts)?;
    write_entries(&root_entries, opts, stdout);

    // Walk all subdirectories and print each with a "path:" header.
    for entry in WalkDir::new(path)
        .min_depth(1)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_dir())
    {
        let dir_name = entry.file_name().to_string_lossy();
        // Respect -a: skip hidden subdirectories unless show_hidden.
        if !opts.show_hidden && dir_name.starts_with('.') {
            continue;
        }
        writeln!(stdout, "\n{}:", entry.path().display())?;
        let sub_entries = collect_entries(entry.path(), opts)?;
        write_entries(&sub_entries, opts, stdout);
    }

    Ok(())
}

/// Read directory entries at `path`, filter hidden if needed, sort alphabetically.
/// When `show_hidden` is true, `.` and `..` are prepended first (matching OS `ls -a`).
fn collect_entries(path: &Path, opts: &DisplayOptions) -> std::io::Result<Vec<ListEntry>> {
    let read = std::fs::read_dir(path)?;

    let mut entries: Vec<ListEntry> = read
        .filter_map(|e| e.ok())
        .filter_map(|dir_entry| {
            let name = dir_entry.file_name().to_string_lossy().into_owned();
            if !opts.show_hidden && name.starts_with('.') {
                return None;
            }
            let path = dir_entry.path();
            let meta = dir_entry.metadata().ok()?;
            Some(ListEntry {
                display_name: name,
                is_dir: meta.is_dir(),
                is_symlink: meta.file_type().is_symlink(),
                size_bytes: meta.len(),
                modified: meta.modified().ok(),
                path,
            })
        })
        .collect();

    entries.sort_by(|a, b| a.display_name.cmp(&b.display_name));

    // When -a is active, prepend "." and ".." — matching OS ls -a behaviour.
    if opts.show_hidden {
        let dot_meta = std::fs::metadata(path).ok();
        let dotdot_path = path.join("..");
        let dotdot_meta = std::fs::metadata(&dotdot_path).ok();

        let dot = ListEntry {
            display_name: ".".to_string(),
            is_dir: true,
            is_symlink: false,
            size_bytes: dot_meta.as_ref().map(|m| m.len()).unwrap_or(0),
            modified: dot_meta.and_then(|m| m.modified().ok()),
            path: path.to_path_buf(),
        };
        let dotdot = ListEntry {
            display_name: "..".to_string(),
            is_dir: true,
            is_symlink: false,
            size_bytes: dotdot_meta.as_ref().map(|m| m.len()).unwrap_or(0),
            modified: dotdot_meta.and_then(|m| m.modified().ok()),
            path: dotdot_path,
        };
        entries.insert(0, dotdot);
        entries.insert(0, dot);
    }

    Ok(entries)
}

// ── Output formatting ─────────────────────────────────────────────────────────

fn write_entries(entries: &[ListEntry], opts: &DisplayOptions, stdout: &mut dyn Write) {
    for entry in entries {
        if opts.long_format {
            write_long_entry(entry, stdout);
        } else {
            writeln!(stdout, "{}", entry.display_name).ok();
        }
    }
}

fn write_long_entry(entry: &ListEntry, stdout: &mut dyn Write) {
    let permissions = format_permissions(entry);
    let size = entry.size_bytes;
    let date = format_modified(entry.modified);
    let name = &entry.display_name;
    // Hard link count: we don't have a WASM-compatible way to get this accurately,
    // so we use 1 as a safe default.
    let link_count: u32 = 1;
    let owner = format_owner(entry);
    let group = format_group(entry);

    writeln!(
        stdout,
        "{permissions}  {link_count} {owner}  {group}  {size:>8} {date} {name}",
    )
    .ok();
}

/// Format the permission string (e.g. `-rw-r--r--`).
/// On non-Unix targets, returns a placeholder.
fn format_permissions(entry: &ListEntry) -> String {
    let type_char = if entry.is_dir {
        'd'
    } else if entry.is_symlink {
        'l'
    } else {
        '-'
    };

    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if let Ok(meta) = std::fs::symlink_metadata(&entry.path) {
            let mode = meta.mode();
            return format!(
                "{}{}{}{}{}{}{}{}{}{}",
                type_char,
                if mode & 0o400 != 0 { 'r' } else { '-' },
                if mode & 0o200 != 0 { 'w' } else { '-' },
                if mode & 0o100 != 0 { 'x' } else { '-' },
                if mode & 0o040 != 0 { 'r' } else { '-' },
                if mode & 0o020 != 0 { 'w' } else { '-' },
                if mode & 0o010 != 0 { 'x' } else { '-' },
                if mode & 0o004 != 0 { 'r' } else { '-' },
                if mode & 0o002 != 0 { 'w' } else { '-' },
                if mode & 0o001 != 0 { 'x' } else { '-' },
            );
        }
    }

    // WASM / non-Unix fallback
    format!("{type_char}---------")
}

/// Format the owner name.
/// On non-Unix targets, returns `-`.
fn format_owner(_entry: &ListEntry) -> String {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if let Ok(meta) = std::fs::symlink_metadata(&_entry.path) {
            let uid = meta.uid();
            // Try to resolve username; fall back to numeric uid.
            return resolve_username(uid);
        }
    }
    "-".to_string()
}

/// Format the group name.
/// On non-Unix targets, returns `-`.
fn format_group(_entry: &ListEntry) -> String {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if let Ok(meta) = std::fs::symlink_metadata(&_entry.path) {
            let gid = meta.gid();
            return resolve_groupname(gid);
        }
    }
    "-".to_string()
}

/// Resolve a Unix UID to a display string.
/// Returns the numeric UID — avoids libc dependency while remaining informative.
#[cfg(unix)]
fn resolve_username(uid: u32) -> String {
    uid.to_string()
}

/// Resolve a Unix GID to a display string.
/// Returns the numeric GID — avoids libc dependency while remaining informative.
#[cfg(unix)]
fn resolve_groupname(gid: u32) -> String {
    gid.to_string()
}

/// Format the modification time in `ls -l` style: `Mar  7 12:00`.
fn format_modified(modified: Option<SystemTime>) -> String {
    let Some(time) = modified else {
        return "            ".to_string();
    };

    let duration = time
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();

    // Manual time calculation — no chrono needed, WASM-compatible.
    let (year, month, day, hour, min) = secs_to_datetime(secs);

    let month_name = MONTH_NAMES[(month - 1) as usize];
    format!("{month_name} {day:>2} {year:>5} {:02}:{:02}", hour, min)
}

const MONTH_NAMES: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun",
    "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Convert Unix timestamp (seconds since epoch) to (year, month, day, hour, min).
/// Handles leap years. WASM-compatible — no OS time APIs needed.
fn secs_to_datetime(secs: u64) -> (u32, u32, u32, u32, u32) {
    let mins = secs / 60;
    let hours = mins / 60;
    let days = hours / 24;
    let min = (mins % 60) as u32;
    let hour = (hours % 24) as u32;

    // Calculate year and day-of-year from days since epoch (1970-01-01).
    let mut year = 1970u32;
    let mut remaining_days = days;
    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if remaining_days < days_in_year {
            break;
        }
        remaining_days -= days_in_year;
        year += 1;
    }

    // Calculate month and day from remaining days.
    let month_days: [u64; 12] = [
        31,
        if is_leap_year(year) { 29 } else { 28 },
        31, 30, 31, 30, 31, 31, 30, 31, 30, 31,
    ];
    let mut month = 1u32;
    for &days_in_month in &month_days {
        if remaining_days < days_in_month {
            break;
        }
        remaining_days -= days_in_month;
        month += 1;
    }
    let day = remaining_days as u32 + 1;

    (year, month, day, hour, min)
}

fn is_leap_year(year: u32) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

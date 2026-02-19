//! CLI linting mode for Avro schema validation
//!
//! This module provides a command-line interface for linting .avsc files
//! with beautiful error reporting using miette.

// Suppress false positive warnings from miette derive macros
#![allow(unused_assignments)]

use std::path::{Path, PathBuf};

use async_lsp::lsp_types::{DiagnosticSeverity, Url};
use miette::{Diagnostic, GraphicalReportHandler, GraphicalTheme, NamedSource, Report, SourceSpan};
use thiserror::Error;

use crate::handlers::diagnostics::parse_and_validate_with_workspace;
use crate::workspace::Workspace;

/// Error type for displaying Avro schema diagnostics with miette
#[derive(Error, Debug, Diagnostic)]
#[error("{message}")]
struct AvroLintError {
    message: String,
    #[source_code]
    src: NamedSource<String>,
    #[label("{label}")]
    span: SourceSpan,
    #[help]
    help: Option<String>,
    label: String,
}

/// Warning type for displaying Avro schema warnings with miette
#[derive(Error, Debug, Diagnostic)]
#[error("{message}")]
#[diagnostic(severity(Warning))]
struct AvroLintWarning {
    message: String,
    #[source_code]
    src: NamedSource<String>,
    #[label("{label}")]
    span: SourceSpan,
    #[help]
    help: Option<String>,
    label: String,
}

/// Discover all .avsc files in a path (file or directory)
fn discover_avsc_files(path: &Path) -> Result<Vec<PathBuf>, std::io::Error> {
    let mut files = Vec::new();

    // Canonicalize to get absolute path
    let abs_path = path.canonicalize()?;

    if abs_path.is_file() {
        if abs_path.extension().and_then(|s| s.to_str()) == Some("avsc") {
            files.push(abs_path);
        }
        return Ok(files);
    }

    if abs_path.is_dir() {
        visit_dirs(&abs_path, &mut files)?;
    }

    Ok(files)
}

/// Recursively visit directories to find .avsc files
fn visit_dirs(dir: &Path, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
    if dir.is_dir() {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                visit_dirs(&path, files)?;
            } else if path.extension().and_then(|s| s.to_str()) == Some("avsc") {
                files.push(path);
            }
        }
    }
    Ok(())
}

/// Find workspace root by looking for .git directory
fn find_workspace_root(start_path: &Path) -> Option<PathBuf> {
    let mut current = start_path;

    loop {
        let git_dir = current.join(".git");
        if git_dir.exists() {
            return Some(current.to_path_buf());
        }

        current = current.parent()?;
    }
}

/// Run the lint command
pub fn run_lint(paths: Vec<PathBuf>, workspace_mode: bool) -> i32 {
    // Discover all files to lint
    let mut all_files = Vec::new();

    for path in &paths {
        match discover_avsc_files(path) {
            Ok(mut files) => all_files.append(&mut files),
            Err(e) => {
                eprintln!("error: failed to scan path {}: {}", path.display(), e);
                return 2;
            }
        }
    }

    if all_files.is_empty() {
        eprintln!("error: no .avsc files found in specified paths");
        return 2;
    }

    // Initialize workspace if requested
    let workspace = if workspace_mode {
        // Try to find workspace root from current directory
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

        match find_workspace_root(&cwd) {
            Some(root) => {
                eprintln!("info: using workspace root: {}", root.display());
                let mut ws = Workspace::with_root(root.clone());

                // Scan and load all .avsc files in workspace
                match discover_avsc_files(&root) {
                    Ok(workspace_files) => {
                        for file_path in workspace_files {
                            if let Ok(content) = std::fs::read_to_string(&file_path)
                                && let Ok(file_url) = Url::from_file_path(&file_path)
                            {
                                // Ignore errors during workspace loading
                                let _ = ws.update_file(file_url, content);
                            }
                        }
                        Some(ws)
                    }
                    Err(e) => {
                        eprintln!("warning: failed to scan workspace: {}", e);
                        eprintln!("warning: continuing without workspace support");
                        None
                    }
                }
            }
            None => {
                eprintln!("warning: could not find workspace root (no .git directory found)");
                eprintln!("warning: continuing without workspace support");
                None
            }
        }
    } else {
        None
    };

    // Lint all files
    let mut total_errors = 0;
    let mut total_warnings = 0;
    let mut files_with_issues = 0;

    for file_path in &all_files {
        let content = match std::fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("error: failed to read {}: {}", file_path.display(), e);
                total_errors += 1;
                files_with_issues += 1;
                continue;
            }
        };

        // Convert to URL for workspace lookup
        let _file_url = match Url::from_file_path(file_path) {
            Ok(url) => url,
            Err(_) => {
                eprintln!("error: invalid file path: {}", file_path.display());
                total_errors += 1;
                files_with_issues += 1;
                continue;
            }
        };

        // Get diagnostics
        let diagnostics = parse_and_validate_with_workspace(&content, workspace.as_ref());

        if diagnostics.is_empty() {
            continue;
        }

        // Count errors and warnings
        let errors: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
            .collect();
        let warnings: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.severity == Some(DiagnosticSeverity::WARNING))
            .collect();

        if !errors.is_empty() || !warnings.is_empty() {
            files_with_issues += 1;

            // Create miette reporter with fancy theme
            let handler = GraphicalReportHandler::new_themed(GraphicalTheme::unicode());

            // Print errors with miette
            for diag in &errors {
                let line = diag.range.start.line as usize;
                let col = diag.range.start.character as usize;
                let end_line = diag.range.end.line as usize;
                let end_col = diag.range.end.character as usize;

                // Calculate byte offset for the span
                let lines: Vec<&str> = content.lines().collect();
                let mut offset = 0;
                for i in 0..line {
                    if i < lines.len() {
                        offset += lines[i].len() + 1; // +1 for newline
                    }
                }
                offset += col;

                // Calculate span length
                let length = if line == end_line {
                    end_col.saturating_sub(col).max(1)
                } else {
                    // Multi-line span - just highlight to end of line
                    lines.get(line).map(|l| l.len() - col).unwrap_or(1).max(1)
                };

                let error = AvroLintError {
                    message: diag.message.clone(),
                    src: NamedSource::new(file_path.display().to_string(), content.clone()),
                    span: SourceSpan::new(offset.into(), length),
                    help: None,
                    label: diag.message.clone(),
                };

                let report = Report::new(error);
                let mut output = String::new();
                handler.render_report(&mut output, report.as_ref()).ok();
                println!("{}", output);
            }

            // Print warnings with miette
            for diag in &warnings {
                let line = diag.range.start.line as usize;
                let col = diag.range.start.character as usize;
                let end_line = diag.range.end.line as usize;
                let end_col = diag.range.end.character as usize;

                // Calculate byte offset for the span
                let lines: Vec<&str> = content.lines().collect();
                let mut offset = 0;
                for i in 0..line {
                    if i < lines.len() {
                        offset += lines[i].len() + 1;
                    }
                }
                offset += col;

                // Calculate span length
                let length = if line == end_line {
                    end_col.saturating_sub(col).max(1)
                } else {
                    lines.get(line).map(|l| l.len() - col).unwrap_or(1).max(1)
                };

                let warning = AvroLintWarning {
                    message: diag.message.clone(),
                    src: NamedSource::new(file_path.display().to_string(), content.clone()),
                    span: SourceSpan::new(offset.into(), length),
                    help: None,
                    label: diag.message.clone(),
                };

                let report = Report::new(warning);
                let mut output = String::new();
                handler.render_report(&mut output, report.as_ref()).ok();
                println!("{}", output);
            }

            total_errors += errors.len();
            total_warnings += warnings.len();
        }
    }

    // Print summary
    println!();
    if total_errors > 0 || total_warnings > 0 {
        println!(
            "Found {} error(s) and {} warning(s) in {} file(s)",
            total_errors, total_warnings, files_with_issues
        );
    } else {
        println!("All {} file(s) validated successfully.", all_files.len());
    }

    // Return exit code
    if total_errors > 0 {
        1 // Errors found
    } else {
        0 // Success
    }
}

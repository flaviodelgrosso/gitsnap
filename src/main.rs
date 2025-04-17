use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use clap::Parser;
use git2::Repository;
use humansize::{format_size, BINARY};
use lazy_static::lazy_static;
use log::{debug, info, warn};
use rayon::prelude::*;
use regex::Regex;
use tempfile::tempdir;
use url::Url;
use walkdir::{DirEntry, WalkDir};

lazy_static! {
  static ref SSH_REGEX: Regex =
    Regex::new(r"^git@github\.com:([^/]+)/([^/]+?)(?:\.git)?$").unwrap();
  static ref SHORT_REGEX: Regex = Regex::new(r"^([^/]+)/([^/]+?)(?:\.git)?$").unwrap();
  static ref URL_REGEX: Regex = Regex::new(r"https://github\.com/[^/]+/([^/]+)").unwrap();
}

#[derive(Parser, Debug)]
#[clap(
  author,
  version,
  about = "Convert GitHub repositories to text files for LLMs"
)]
struct Args {
  /// GitHub repository URL or short format (username/repo)
  #[clap(value_parser)]
  repository: String,

  /// Specify output file path
  #[clap(short, long, value_parser)]
  output: Option<String>,

  /// Set file size threshold in MB
  #[clap(short, long, value_parser, default_value_t = 0.1)]
  threshold: f32,

  /// Include all files regardless of size or type
  #[clap(long, value_parser)]
  include_all: bool,

  /// Enable debug mode with verbose logging
  #[clap(long, value_parser)]
  debug: bool,
}

fn normalize_repository_url(repo_input: &str) -> Result<String> {
  // Check if it's already a valid URL
  if let Ok(url) = Url::parse(repo_input) {
    if url.scheme() == "https" || url.scheme() == "http" {
      let mut normalized = url.to_string();
      // Remove trailing .git if present
      if std::path::Path::new(&normalized)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("git"))
      {
        normalized.truncate(normalized.len() - 4);
      }
      // Remove trailing slash if present
      if normalized.ends_with('/') {
        normalized.pop();
      }
      return Ok(normalized);
    }
  }

  // Check if it's in SSH format: git@github.com:username/repository.git
  if let Some(captures) = SSH_REGEX.captures(repo_input) {
    return Ok(format!(
      "https://github.com/{}/{}",
      captures.get(1).unwrap().as_str(),
      captures.get(2).unwrap().as_str()
    ));
  }

  // Check if it's in short format: username/repository
  if let Some(captures) = SHORT_REGEX.captures(repo_input) {
    return Ok(format!(
      "https://github.com/{}/{}",
      captures.get(1).unwrap().as_str(),
      captures.get(2).unwrap().as_str()
    ));
  }

  anyhow::bail!(
    "Invalid repository format. Expected HTTPS URL, SSH URL, or 'username/repository' format."
  )
}

fn clone_repository(url: &str, temp_dir: &Path) -> Result<()> {
  info!("Cloning repository: {url}");
  let git_url = if std::path::Path::new(url)
    .extension()
    .is_some_and(|ext| ext.eq_ignore_ascii_case("git"))
  {
    url.to_string()
  } else {
    format!("{url}.git")
  };

  Repository::clone(&git_url, temp_dir).context("Failed to clone repository")?;

  info!("Repository cloned to: {}", temp_dir.display());
  Ok(())
}

fn is_binary_file(path: &Path) -> Result<bool> {
  let file = File::open(path)?;
  let mut reader = BufReader::with_capacity(8000, file);
  let mut buffer = [0; 8000]; // Read up to 8KB to detect binary content
  let bytes_read = reader.read(&mut buffer)?;

  // Check for null bytes in the first chunk, which likely indicates a binary file
  for &byte in &buffer[..bytes_read] {
    if byte == 0 {
      return Ok(true);
    }
  }

  Ok(false)
}

fn is_excluded_file(entry: &DirEntry, threshold_bytes: u64, include_all: bool) -> Result<bool> {
  let path = entry.path();

  // Skip .git directory
  if path.components().any(|comp| comp.as_os_str() == ".git") {
    return Ok(true);
  }

  // Skip node_modules directory
  if path
    .components()
    .any(|comp| comp.as_os_str() == "node_modules")
  {
    return Ok(true);
  }

  // Skip .gitignore files (using unwrap_or_else instead of map+unwrap_or)
  if path.file_name().is_some_and(|name| name == ".gitignore") {
    return Ok(true);
  }

  if include_all {
    return Ok(false);
  }

  // Check if file is a directory
  if entry.file_type().is_dir() {
    return Ok(false);
  }

  // Check file size
  let metadata = entry.metadata()?;
  if metadata.len() > threshold_bytes {
    debug!(
      "Skipping large file: {} ({})",
      path.display(),
      format_size(metadata.len(), BINARY)
    );
    return Ok(true);
  }

  // Check if binary
  if is_binary_file(path)? {
    debug!("Skipping binary file: {}", path.display());
    return Ok(true);
  }

  Ok(false)
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn process_repository(
  repo_dir: &Path,
  output_path: &Path,
  threshold_mb: f32,
  include_all: bool,
) -> Result<()> {
  let threshold_bytes = (threshold_mb * 1024.0 * 1024.0) as u64;
  info!(
    "Processing repository with threshold: {}",
    format_size(threshold_bytes, BINARY)
  );

  // Create output file
  let file = File::create(output_path)?;
  let output_file = Arc::new(Mutex::new(BufWriter::new(file)));

  // Collect all valid files first
  let mut valid_entries = vec![];
  for entry in WalkDir::new(repo_dir).into_iter().filter_map(Result::ok) {
    if let Ok(false) = is_excluded_file(&entry, threshold_bytes, include_all) {
      if entry.path().is_file() {
        valid_entries.push(entry);
      }
    }
  }

  info!("Found {} valid files to process", valid_entries.len());

  // Process files in parallel
  valid_entries.par_iter().for_each(|entry| {
    if let Err(err) = (|| -> Result<()> {
      let path = entry.path();
      let relative_path = path.strip_prefix(repo_dir)?;
      let metadata = fs::metadata(path)?;
      let file_size = format_size(metadata.len(), BINARY);

      // Get lock on output file only when writing
      let mut output_guard = output_file.lock().unwrap();
      writeln!(
        output_guard,
        "================================================================================"
      )?;
      writeln!(output_guard, "File: {}", relative_path.display())?;
      writeln!(output_guard, "Size: {file_size}")?;
      writeln!(
        output_guard,
        "================================================================================"
      )?;

      // Stream the file content directly
      {
        let file = File::open(path)?;
        let mut reader = BufReader::new(file);
        let mut buffer = [0; 8192];

        loop {
          let bytes_read = reader.read(&mut buffer)?;
          if bytes_read == 0 {
            break; // End of file
          }

          // Write the chunk directly to the output file
          output_guard.write_all(&buffer[..bytes_read])?;
        }
      }

      // Add a newline after the file content
      writeln!(output_guard)?;
      writeln!(output_guard)?;

      Ok(())
    })() {
      warn!("Error processing {}: {}", entry.path().display(), err);
    }
  });

  // Make sure to flush the buffer before finishing
  let mut output_guard = output_file.lock().unwrap();
  output_guard.flush()?;

  info!(
    "Repository converted and saved to: {}",
    output_path.display()
  );
  Ok(())
}

fn extract_repo_name(url: &str) -> Result<String> {
  if let Some(captures) = URL_REGEX.captures(url) {
    return Ok(captures.get(1).unwrap().as_str().to_owned());
  }
  anyhow::bail!("Could not extract repository name from URL")
}

fn main() -> Result<()> {
  let args = Args::parse();

  if args.debug {
    env_logger::Builder::new()
      .filter_level(log::LevelFilter::Debug)
      .init();
  } else {
    env_logger::Builder::new()
      .filter_level(log::LevelFilter::Info)
      .init();
  }

  info!("Starting gitsnap...");

  // Normalize repository URL
  let repo_url = normalize_repository_url(&args.repository)?;
  info!("Normalized repository URL: {repo_url}");

  // Create temporary directory for cloning
  let temp_dir = tempdir()?;
  info!("Created temporary directory: {}", temp_dir.path().display());

  // Clone repository
  clone_repository(&repo_url, temp_dir.path())?;

  // Determine output file path
  let repo_name = extract_repo_name(&repo_url)?;
  let output_path = match &args.output {
    Some(path) => PathBuf::from(path),
    None => PathBuf::from(format!("{repo_name}.txt")),
  };

  info!("Output will be saved to: {}", output_path.display());

  // Process repository and generate output file
  process_repository(
    temp_dir.path(),
    &output_path,
    args.threshold,
    args.include_all,
  )?;

  info!(
    "Done! Repository contents saved to: {}",
    output_path.display()
  );
  Ok(())
}

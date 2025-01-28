#![allow(clippy::module_name_repetitions)]

mod files;
mod repository;

use std::path::{Path, PathBuf};
use std::time::Instant;

use clap::{App, Arg};
use colored::Colorize;

use tempfile::TempDir;
use tokio::fs::{remove_dir_all, File};
use tokio::io::AsyncWriteExt;

use files::process_files;
use repository::{download_repository, normalize_github_url};

/// Parses command line arguments for the `gitsnap` CLI tool.
///
/// The following arguments are supported:
///
/// - `repository`: The GitHub repository URL or user/repo format (e.g., 'user/repo' or 'https://github.com/user/repo').
///   This argument is required and must be provided as the first positional argument.
///
/// - `output`: Specifies the output file path. This is an optional argument with a short form `-o` and a long form `--output`.
///   If not provided, the default output file name will be `repo_name.txt`.
///
/// - `threshold`: Sets the file size threshold in MB for text conversion. This is an optional argument with a short form `-t` and a long form `--threshold`.
///   Files larger than this threshold are skipped unless the `--include-all` flag is used. The default value is `0.1` MB.
///
/// - `include_all`: A flag to include all files, regardless of size or type. This flag overrides the `threshold` argument.
///
/// - `debug`: A flag to enable debug mode with verbose logging.
#[tokio::main]
async fn main() {
  let matches = App::new("gitsnap")
        .version("0.1.0")
        .about("A CLI tool to convert GitHub repositories into readable text files")
        .arg(
            Arg::with_name("repository")
                .help("GitHub repository URL or user/repo format (e.g., 'user/repo' or 'https://github.com/user/repo')")
                .required(true)
                .index(1),
        )
        .arg(
            Arg::with_name("output")
                .short("o")
                .long("output")
                .value_name("FILE")
                .help("Specify output file path (defaults to repo_name.txt)")
        )
        .arg(
            Arg::with_name("threshold")
                .short("t")
                .long("threshold")
                .value_name("MB")
                .help("Set file size threshold in MB for text conversion (default: 0.1 MB). Files larger than this are skipped unless --include-all is used.")
                .default_value("0.1"),
        )
        .arg(
            Arg::with_name("include_all")
                .long("include-all")
                .help("Include all files, regardless of size or type. Overrides the threshold.")
        )
        .arg(
            Arg::with_name("debug")
                .long("debug")
                .help("Enable debug mode with verbose logging")
        )
        .get_matches();

  let repo_url = matches.value_of("repository").unwrap();
  let output_path = matches.value_of("output");
  let threshold: f64 = matches
    .value_of("threshold")
    .unwrap()
    .parse()
    .expect("Invalid threshold value. Please provide a number.");

  let include_all = matches.is_present("include_all");
  let debug = matches.is_present("debug");

  if debug {
    println!("{}", "Debug Mode Enabled".blue());
  }

  if let Err(e) = run(repo_url, output_path, threshold, include_all, debug).await {
    eprintln!("{} {}", "Error:".red().bold(), e);
    std::process::exit(1);
  }
}

async fn run(
  repo_url: &str,
  output_path: Option<&str>,
  threshold: f64,
  include_all: bool,
  debug: bool,
) -> Result<(), Box<dyn std::error::Error>> {
  let normalized_url =
    normalize_github_url(repo_url).map_err(|e| format!("Invalid GitHub URL: {e}"))?;

  if debug {
    println!(
      "{}",
      format!("Debug: Normalized URL: {normalized_url}").blue()
    );
  }

  let clone_start_time = Instant::now();
  let (temp_dir, repo_name) = download_repository(&normalized_url, debug)?;

  if debug {
    println!(
      "{}",
      format!(
        "Debug: Repository download took: {:.2?}",
        clone_start_time.elapsed()
      )
      .blue()
    );
    println!(
      "{}",
      format!("Debug: Temp Dir created at: {temp_dir:?}").blue()
    );
  }

  let output_file =
    output_path.map_or_else(|| PathBuf::from(format!("{repo_name}.txt")), PathBuf::from);

  let process_start_time = Instant::now();
  let content = process_files(temp_dir.path(), threshold, include_all, debug).await?;

  if debug {
    println!(
      "{}",
      format!(
        "Debug: File Processing took: {:.2?}",
        process_start_time.elapsed()
      )
      .blue()
    );
  }

  let write_start_time = Instant::now();
  write_output(&content, &output_file).await?;

  if debug {
    println!(
      "{}",
      format!(
        "Debug: Output Writing took: {:.2?}",
        write_start_time.elapsed()
      )
      .blue()
    );
  }

  if debug {
    println!(
      "{}",
      format!("Debug: Cleaning up temp dir {temp_dir:?}").blue()
    );
  }

  cleanup(&temp_dir).await?;

  Ok(())
}

async fn write_output(content: &str, output_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
  let mut file = File::create(output_path).await?;
  file.write_all(content.as_bytes()).await?;
  file.flush().await?; // Ensure all data is written to disk
  println!("{} {}", "Output saved to:".green(), output_path.display());
  Ok(())
}

async fn cleanup(directory: &TempDir) -> Result<(), Box<dyn std::error::Error>> {
  remove_dir_all(directory.path()).await?;
  Ok(())
}

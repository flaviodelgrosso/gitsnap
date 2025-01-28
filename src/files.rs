use std::fs;
use std::path::Path;

use colored::Colorize;
use infer::Infer;
use tokio::fs::{read_dir, read_to_string, File};
use tokio::io::AsyncReadExt;

struct Config<'a> {
  threshold_bytes: f64,
  include_all: bool,
  debug: bool,
  root_dir: &'a Path,
}

struct ProcessState<'a> {
  output_buffer: &'a mut Vec<String>, // Buffer of strings
  processed_files: &'a mut usize,
  skipped_files: &'a mut usize,
}

pub async fn is_binary_file(path: &Path) -> Result<bool, std::io::Error> {
  let file = File::open(path).await?;
  let mut buffer = [0; 1024];
  let mut handle = file.take(1024);
  let bytes_read = handle.read(&mut buffer).await?;

  if bytes_read == 0 {
    return Ok(false);
  }

  let infer = Infer::new();
  let kind = infer.get(&buffer[..bytes_read]);

  match kind {
    Some(k) => Ok(!k.mime_type().starts_with("text/")),
    None => Ok(buffer[..bytes_read].iter().any(|&byte| byte == 0)),
  }
}

pub async fn process_files(
  directory: &Path,
  threshold_mb: f64,
  include_all: bool,
  debug: bool,
) -> Result<String, Box<dyn std::error::Error>> {
  let threshold_bytes = threshold_mb * 1024.0 * 1024.0;
  let mut output_buffer: Vec<String> = Vec::new();
  let mut processed_files = 0;
  let mut skipped_files = 0;

  process_directory(
    directory,
    Config {
      threshold_bytes,
      include_all,
      debug,
      root_dir: directory,
    },
    ProcessState {
      output_buffer: &mut output_buffer,
      processed_files: &mut processed_files,
      skipped_files: &mut skipped_files,
    },
  )
  .await?;

  if processed_files == 0 && debug {
    eprintln!("{}", "Warning: No files were processed".yellow());
  }

  if debug {
    println!(
      "{}",
      format!("Debug: Processed {processed_files} files, skipped {skipped_files} files").blue()
    );
  }

  Ok(output_buffer.join("")) // Join all strings at the end
}

async fn process_directory<'a>(
  dir: &'a Path,
  config: Config<'a>,
  state: ProcessState<'a>,
) -> Result<(), Box<dyn std::error::Error>> {
  let mut entries = read_dir(dir).await?;
  let output_buffer = state.output_buffer;
  let processed_files = state.processed_files;
  let skipped_files = state.skipped_files;

  while let Some(entry) = entries.next_entry().await? {
    let full_path = entry.path();
    let file_name = entry.file_name();
    let file_name_str = file_name.to_string_lossy();

    if entry.metadata().await?.is_dir()
      && file_name_str != "node_modules"
      && file_name_str != ".git"
    {
      Box::pin(process_directory(
        &full_path,
        Config { ..config },
        ProcessState {
          output_buffer,
          processed_files,
          skipped_files,
        },
      ))
      .await?;
      continue;
    }

    if !entry.metadata().await?.is_file() {
      continue;
    }

    let metadata = fs::metadata(&full_path)?;
    let file_size = metadata.len();
    let threshold_bytes_u64 = config.threshold_bytes as u64;

    if !config.include_all && file_size > threshold_bytes_u64 {
      if config.debug {
        println!(
          "{}",
          format!("Debug: Skipping large file: {file_name_str}").blue()
        );
      }
      *skipped_files += 1;
      continue;
    }

    if !config.include_all && is_binary_file(&full_path).await? {
      if config.debug {
        println!(
          "{}",
          format!("Debug: Skipping binary file: {file_name_str}").blue()
        );
      }
      *skipped_files += 1;
      continue;
    }

    let content = read_to_string(&full_path).await?;
    let relative_path = full_path
      .strip_prefix(config.root_dir)
      .unwrap_or(&full_path);
    let relative_path_str = relative_path.to_string_lossy();

    output_buffer.push(format!("\n{}\n", "=".repeat(80)));
    output_buffer.push(format!("File: {relative_path_str}\n"));
    output_buffer.push(format!("Size: {}\n", format_file_size(file_size)));
    output_buffer.push(format!("{}\n\n", "=".repeat(80)));
    output_buffer.push(content);

    *processed_files += 1;

    if config.debug {
      println!(
        "{}",
        format!("Debug: Processed file: {relative_path_str}").blue()
      );
    }
  }
  Ok(())
}

fn format_file_size(size: u64) -> String {
  const UNITS: [&str; 5] = ["bytes", "KB", "MB", "GB", "TB"];
  let mut size_f64 = size as f64;
  let mut unit_index = 0;

  while size_f64 >= 1024.0 && unit_index < UNITS.len() - 1 {
    size_f64 /= 1024.0;
    unit_index += 1;
  }

  format!("{:.2} {}", size_f64, UNITS[unit_index])
}

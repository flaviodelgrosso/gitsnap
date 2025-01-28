use std::fs;
use std::process::{Command, Stdio};

use colored::Colorize;
use lazy_static::lazy_static;
use regex::Regex;
use tempfile::TempDir;

lazy_static! {
  static ref GITHUB_SHORT_URL_REGEX: Regex = Regex::new(r"^[\w-]+/[\w-]+$").unwrap();
}

pub fn download_repository(
  url: &str,
  debug: bool,
) -> Result<(TempDir, String), Box<dyn std::error::Error>> {
  let temp_dir = TempDir::new()?;
  let temp_path = temp_dir.path();
  let repo_name = url
    .split('/')
    .last()
    .unwrap_or("repo")
    .trim_end_matches(".git")
    .to_string();
  let temp_path_str = temp_path.to_string_lossy();

  let clone_command = format!("git clone --depth 1 {url} {temp_path_str}");
  if debug {
    println!(
      "{}",
      format!("Debug: Executing command: {clone_command}").blue()
    );
  }

  let output = Command::new("git")
    .args(["clone", "--depth", "1", url, &temp_path_str])
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .output()?;

  if !output.status.success() {
    let err_msg = String::from_utf8_lossy(&output.stderr);
    eprintln!("{} Git clone failed. Please check:", "Error:".red().bold());
    eprintln!("{}", "  1. The repository exists and is public".yellow());
    eprintln!("{}", "  2. You have the correct repository URL".yellow());
    eprintln!("{}", "  3. GitHub is accessible from your network".yellow());
    eprintln!(
      "{}",
      "  4. Git is installed and accessible from command line".yellow()
    );
    return Err(format!("Failed to clone repository: {err_msg}").into());
  }

  let files_count = fs::read_dir(temp_path)
    .map(std::iter::Iterator::count)
    .unwrap_or(0); // Directly count entries

  if files_count == 0 {
    return Err("Repository appears to be empty".into());
  }

  if debug {
    println!(
      "{}",
      format!("Debug: Repository downloaded successfully to: {temp_path_str}").blue()
    );
  }
  Ok((temp_dir, repo_name))
}

pub fn normalize_github_url(url: &str) -> Result<String, String> {
  let url = url.trim_end_matches('/');
  if url.starts_with("git@github.com:") || url.starts_with("https://github.com/") {
    Ok(url.to_string())
  } else if GITHUB_SHORT_URL_REGEX.is_match(url) {
    Ok(format!("https://github.com/{url}"))
  } else {
    Err("Invalid GitHub repository URL format".to_string())
  }
}

# GitSnap

[![Latest Version]][crates.io] ![Crates.io Downloads](https://img.shields.io/crates/d/gitsnap) ![GitHub Repo stars](https://img.shields.io/github/stars/flaviodelgrosso/gitsnap?style=flat)

[Latest Version]: https://img.shields.io/crates/v/gitsnap.svg
[crates.io]: https://crates.io/crates/gitsnap

GitSnap is a CLI tool to take snapshots of GitHub repositories and convert them into readable text files.

## Description

gitsnap allows you to download a GitHub repository and convert its contents into a single text file. You can specify a file size threshold to skip large files or include all files regardless of size. The tool also supports debug mode for verbose logging.

## Installation

To install gitsnap, you need to have Rust and Cargo installed. You can install Rust and Cargo from [rustup.rs](https://rustup.rs/).

You can install gitsnap from [crates.io](https://crates.io/crates/gitsnap) using Cargo.

```sh
cargo install gitsnap
```

Alternatively, you can build the project from source.

```sh
git clone <https://github.com/flaviodelgrosso/GitSnap.git>
cd gitsnap
cargo build --release
```

## Usage

Run the `gitsnap` command with the required arguments:

```sh
gitsnap <repository> [OPTIONS]
```

### Arguments

- `repository`: The GitHub repository URL or user/repo format (e.g., 'user/repo' or '<https://github.com/user/repo>'). This argument is required.

### Options

- `-o, --output <FILE>`: Specify the output file path (defaults to `repo_name.txt`).
- `-t, --threshold <MB>`: Set file size threshold in MB for text conversion (default: 0.1 MB). Files larger than this are skipped unless `--include-all` is used.
- `--include-all`: Include all files, regardless of size or type. Overrides the threshold.
- `--debug`: Enable debug mode with verbose logging.

### Example

```sh
gitsnap user/repo -o output.txt -t 1.0 --debug
```

This command will download the `user/repo` repository, process files up to 1.0 MB, and save the output to `output.txt` with debug mode enabled.

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.

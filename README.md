# DOCX Cleaner

A Rust tool to remove special characters from DOCX files while preserving the original document structure and formatting.

## Features

- Removes invisible and uncommon Unicode characters from DOCX files
- Configurable character list via JSON cnfiguration
- Preserves original file (read-only operation)
- Cross-platform support (Windows, Linux)
- Processes text in paragraphs, tables, and other document elements

## Installation

### Download Pre-built Binaries

Download the latest release for your platform from the [Releases page](https://github.com/your-username/docx-cleaner/releases).

### Build from Source

```bash
git clone https://github.com/your-username/docx-cleaner.git
cd docx-cleaner
cargo build --release

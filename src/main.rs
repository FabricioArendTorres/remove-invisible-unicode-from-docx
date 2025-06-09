#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]
use clap::Parser;
use dashmap::DashMap; // Thread-safe HashMap
use docx_rs::*;
use once_cell::sync::Lazy;
use rfd::FileDialog;
use rfd::MessageDialog;
use serde_json;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fs::File;
use std::path::PathBuf;
use regex::Regex;

#[derive(Parser)]
#[command(name = "docx-cleaner")]
#[command(about = "Remove special characters from DOCX files")]
struct Cli {
    input: Option<PathBuf>,
}

use rfd::MessageLevel;
use std::panic::PanicHookInfo;

const RED_CROSS_MARK: char = '❌';
static CONFIG_STR: &str = include_str!("../src/config.json");

static CONFIG_DATA: Lazy<HashMap<char, (String, char)>> = Lazy::new(|| {
    let config_str = CONFIG_STR;
    let json_map: HashMap<String, Vec<String>> =
        serde_json::from_str(&config_str).expect("Failed to parse config.json");

    json_map
        .into_iter()
        .map(|(k, v)| {
            let key_char = k.chars().next().unwrap();
            let description = v.get(0).cloned().unwrap_or_else(|| "UNKNOWN".to_string());
            let replacement_char = v.get(1)
                .and_then(|s| s.chars().next())
                .unwrap_or('❌'); // fallback to red cross mark
            (key_char, (description, replacement_char))
        })
        .collect()
});


static UNICODE_CHARS_TO_REMOVE: Lazy<HashSet<char>> =
    Lazy::new(|| CONFIG_DATA.keys().copied().collect());

static CHAR_COUNTERS: Lazy<DashMap<char, usize>> = Lazy::new(|| {
    let map = DashMap::new();
    for &c in UNICODE_CHARS_TO_REMOVE.iter() {
        map.insert(c, 0);
    }
    map
});

static CHAR_NAMES: Lazy<HashMap<char, String>> = Lazy::new(|| {
    CONFIG_DATA.iter()
        .map(|(&k, (desc, _))| (k, desc.clone()))
        .collect()
});

// Add a new static for replacement characters
static REPLACEMENT_CHARS: Lazy<HashMap<char, char>> = Lazy::new(|| {
    CONFIG_DATA.iter()
        .map(|(&k, (_, replacement))| (k, *replacement))
        .collect()
});

pub fn show_error_dialog(title: &str, message: &str) {
    MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title(title)
        .set_description(message)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}


fn setup_panic_handler() {
    std::panic::set_hook(Box::new(|panic_info: &PanicHookInfo| {
        let message = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "Unknown panic".to_string()
        };

        let location = if let Some(loc) = panic_info.location() {
            format!("{}:{}:{}", loc.file(), loc.line(), loc.column())
        } else {
            "unknown location".to_string()
        };

        let full_message = format!("Panic at {}:\n\n{}", location, message);

        // Log to file
        eprintln!("{}", full_message); // only useful in console mode
        log_panic_to_file(&full_message); // log, just in case

        // Show in GUI
        MessageDialog::new()
            .set_title("Fatal Error")
            .set_description(&full_message)
            .set_level(MessageLevel::Error)
            .set_buttons(rfd::MessageButtons::Ok)
            .show();

        std::process::exit(1); // Ensure clean exit
    }));
}

fn log_panic_to_file(log: &str) {
    use std::fs::OpenOptions;
    use std::io::Write;

    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open("error.log")
    {
        let _ = writeln!(file, "{}", log);
    }
}

fn main() {
    let cli = Cli::parse();

    setup_panic_handler();

    let (input_path, is_gui_mode) = match cli.input {
        Some(path) => {
            if !path.exists() {
                eprintln!("Error: File '{}' does not exist.", path.display());
                std::process::exit(1)
            }
            (path, false)
        }
        None => {
            let file_path = FileDialog::new()
                .add_filter("Word Documents", &["docx"])
                .set_title("Select a DOCX file to process")
                .pick_file()
                .expect("No file selected");
            (file_path, true)
        }
    };

    let buf = std::fs::read(&input_path).expect("Failed to read DOCX input file.");
    let mut docx = read_docx(&buf).expect("Failed to parse DOCX.");

    for mut entry in CHAR_COUNTERS.iter_mut() {
        *entry.value_mut() = 0;
    }
    clean_document(&mut docx);

    let output_path = generate_output_path(&input_path);
    let mut output_file = File::create(&output_path).expect("Failed to create output file.");
    docx.build()
        .pack(&mut output_file)
        .expect("Failed to write DOCX.");

    if is_gui_mode {
        show_gui_statistics(&output_path);
    } else {
        print_console_statistics(&output_path);
    }
}

fn clean_document(docx: &mut Docx) {
    // Trivial transformation: remove all spaces
    for child in &mut docx.document.children {
        if let DocumentChild::Paragraph(paragraph) = child {
            clean_paragraph(paragraph);
        }
    }
}

fn clean_paragraph(paragraph: &mut Paragraph) {
    for child in &mut paragraph.children {
        if let ParagraphChild::Run(run) = child {
            clean_run(run);
        }
    }
}

fn clean_run(run: &mut Run) {
    for child in &mut run.children {
        if let RunChild::Text(text) = child {
            let cleaned: String = text
                .text
                .chars()
                .map(|c| {
                    if UNICODE_CHARS_TO_REMOVE.contains(&c) {
                        // Increment the counter for this specific character
                        if let Some(mut counter) = CHAR_COUNTERS.get_mut(&c) {
                            *counter += 1;
                        }
                        // Use the replacement character from the config instead of RED_CROSS_MARK
                        REPLACEMENT_CHARS.get(&c).copied().unwrap_or(RED_CROSS_MARK)
                    } else {
                        c
                    }
                })
                .collect();
            let space_collapse_re = Regex::new(r"[ ]{2,}").unwrap();

            let num_space_replacements = space_collapse_re.find_iter(&cleaned).count();

            if num_space_replacements > 0 {
                println!(
                    "Replaced {} stretch(es) of multiple spaces in '{}'",
                    num_space_replacements, cleaned
                );
            }

            let collapsed = space_collapse_re.replace_all(&cleaned, " ");
            text.text = collapsed.into_owned();
        }
    }
}


fn generate_output_path(input_path: &PathBuf) -> PathBuf {
    let stem = input_path.file_stem().unwrap().to_str().unwrap();
    let extension = input_path.extension().unwrap().to_str().unwrap();
    let parent = input_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));

    parent.join(format!("{}_cleaned.{}", stem, extension))
}
fn print_console_statistics(output_path: &PathBuf) {
    println!("\nCharacter Removal Statistics:");
    println!("============================");
    let mut total = 0;

    let mut results: Vec<(char, usize)> = CHAR_COUNTERS
        .iter()
        .map(|entry| (*entry.key(), *entry.value()))
        .collect();
    results.sort_by_key(|&(_, count)| std::cmp::Reverse(count));

    for (char, count) in results {
        if count > 0 {
            let name = CHAR_NAMES
                .get(&char)
                .map(|s| s.as_str())
                .unwrap_or("UNKNOWN");
            println!("{} (U+{:04X}) - {}: {}", name, char as u32, char, count);
            total += count;
        }
    }

    println!("\nTotal characters removed: {}", total);
    println!("Saved as: {}", output_path.display());
}

fn show_gui_statistics(output_path: &PathBuf) {
    let mut message = String::from("Character Removal Statistics:\n");
    message.push_str("============================\n\n");
    let mut total = 0;

    let mut results: Vec<(char, usize)> = CHAR_COUNTERS
        .iter()
        .map(|entry| (*entry.key(), *entry.value()))
        .collect();
    results.sort_by_key(|&(_, count)| std::cmp::Reverse(count));

    for (char, count) in results {
        if count > 0 {
            let name = CHAR_NAMES
                .get(&char)
                .map(|s| s.as_str())
                .unwrap_or("UNKNOWN");
            message.push_str(&format!(
                "{} (U+{:04X}) - {}: {}\n",
                name, char as u32, char, count
            ));
            total += count;
        }
    }

    message.push_str(&format!("\nTotal characters removed: {}\n", total));
    message.push_str(&format!("Saved as: {}", output_path.display()));

    let _ok = MessageDialog::new()
        .set_title("Processing Complete")
        .set_description(&message)
        .set_buttons(rfd::MessageButtons::Ok)
        .set_level(rfd::MessageLevel::Info)
        .show();
}

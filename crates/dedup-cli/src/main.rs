use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use dedup_core::Store;

#[derive(Parser)]
#[command(name = "dedup", about = "Content-addressed file deduplication tool")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Scan a source directory and store files in content-addressed format.
    Scan {
        /// Source directory to scan.
        #[arg(short, long)]
        source: PathBuf,

        /// Store directory (will be created if it doesn't exist).
        #[arg(short = 'o', long, default_value = ".store")]
        store: PathBuf,
    },

    /// List contents of a virtual directory in the store.
    Ls {
        /// Virtual path to list (e.g. "/" or "/docs").
        #[arg(default_value = "/")]
        path: String,

        /// Store directory to read from.
        #[arg(short = 'o', long, default_value = ".store")]
        store: PathBuf,
    },

    /// Show metadata for a virtual file path.
    Info {
        /// Virtual file path (e.g. "/docs/readme.txt").
        path: String,

        /// Store directory to read from.
        #[arg(short = 'o', long, default_value = ".store")]
        store: PathBuf,
    },

    /// Find all duplicate file groups in the store.
    Duplicates {
        /// Store directory to read from.
        #[arg(short = 'o', long, default_value = ".store")]
        store: PathBuf,
    },

    /// Extract a file from the store to stdout or a destination path.
    Cat {
        /// Virtual file path to read.
        path: String,

        /// Optional output file (defaults to stdout).
        #[arg(short = 'o', long)]
        output: Option<PathBuf>,

        /// Store directory to read from.
        #[arg(short = 's', long, default_value = ".store")]
        store: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Scan { source, store } => cmd_scan(&source, &store),
        Commands::Ls { path, store } => cmd_ls(&path, &store),
        Commands::Info { path, store } => cmd_info(&path, &store),
        Commands::Duplicates { store } => cmd_duplicates(&store),
        Commands::Cat {
            path,
            output,
            store,
        } => cmd_cat(&path, output.as_deref(), &store),
    }
}

fn cmd_scan(source: &PathBuf, store_path: &PathBuf) -> Result<()> {
    println!("Scanning: {}", source.display());
    println!("Store:    {}", store_path.display());
    println!();

    let store = Store::open(store_path).context("failed to open store")?;
    let stats = store.scan(source).context("scan failed")?;

    println!("Scan complete!");
    println!("  Files:           {}", stats.total_files);
    println!("  Directories:     {}", stats.total_dirs);
    println!("  Unique blobs:    {}", stats.unique_blobs);
    println!("  Duplicate files: {}", stats.duplicate_files);
    println!(
        "  Original size:   {} bytes ({:.2} MB)",
        stats.total_original_bytes,
        stats.total_original_bytes as f64 / 1_048_576.0
    );
    println!(
        "  Stored size:     {} bytes ({:.2} MB)",
        stats.total_stored_bytes,
        stats.total_stored_bytes as f64 / 1_048_576.0
    );

    if stats.total_original_bytes > 0 {
        let ratio = stats.total_stored_bytes as f64 / stats.total_original_bytes as f64;
        let saved = 1.0 - ratio;
        println!("  Space saved:     {:.1}%", saved * 100.0);
    }

    Ok(())
}

fn cmd_ls(path: &str, store_path: &PathBuf) -> Result<()> {
    let store = Store::open(store_path).context("failed to open store")?;
    let entries = store.list_dir(path).context("failed to list directory")?;

    if entries.is_empty() {
        println!("(empty directory or path not found)");
        return Ok(());
    }

    for entry in &entries {
        if entry.is_dir {
            println!("  {}  {}/", pad_size("DIR"), entry.name);
        } else {
            println!("  {}  {}", pad_size(&format_size(entry.size)), entry.name);
        }
    }

    Ok(())
}

fn cmd_info(path: &str, store_path: &PathBuf) -> Result<()> {
    let store = Store::open(store_path).context("failed to open store")?;

    match store.get_file(path)? {
        Some(meta) => {
            let cid = dedup_core::cid::cid_from_bytes(&meta.cid)?;
            let cid_str = dedup_core::cid::cid_to_string(&cid);

            println!("Path:            {path}");
            println!("CID:             {cid_str}");
            println!(
                "Original size:   {} ({})",
                meta.original_size,
                format_size(meta.original_size)
            );
            println!(
                "Compressed size: {} ({})",
                meta.compressed_size,
                format_size(meta.compressed_size)
            );
            println!("Modified:        {}", meta.modified);
            println!("Created:         {}", meta.created);
            println!("Permissions:     {:o}", meta.permissions);

            // Check for duplicates
            let dups = store.find_duplicates(path)?;
            if dups.len() > 1 {
                println!("\nDuplicate copies ({}):", dups.len());
                for dup in &dups {
                    let marker = if dup == path { " ← this file" } else { "" };
                    println!("  {dup}{marker}");
                }
            }
        }
        None => {
            println!("File not found: {path}");
        }
    }

    Ok(())
}

fn cmd_duplicates(store_path: &PathBuf) -> Result<()> {
    let store = Store::open(store_path).context("failed to open store")?;
    let groups = store.find_all_duplicates()?;

    if groups.is_empty() {
        println!("No duplicates found.");
        return Ok(());
    }

    println!("Found {} duplicate group(s):\n", groups.len());

    for (i, (cid_str, paths)) in groups.iter().enumerate() {
        println!(
            "Group {} (CID: {}…):",
            i + 1,
            &cid_str[..cid_str.len().min(20)]
        );
        for path in paths {
            println!("  {path}");
        }
        println!();
    }

    Ok(())
}

fn cmd_cat(path: &str, output: Option<&std::path::Path>, store_path: &PathBuf) -> Result<()> {
    let store = Store::open(store_path).context("failed to open store")?;
    let data = store.read_file(path)?;

    match output {
        Some(out_path) => {
            std::fs::write(out_path, &data)
                .with_context(|| format!("failed to write to {}", out_path.display()))?;
            println!("Extracted {} bytes to {}", data.len(), out_path.display());
        }
        None => {
            use std::io::Write;
            std::io::stdout()
                .write_all(&data)
                .context("failed to write to stdout")?;
        }
    }

    Ok(())
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1_048_576 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1_073_741_824 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else {
        format!("{:.1} GB", bytes as f64 / 1_073_741_824.0)
    }
}

fn pad_size(s: &str) -> String {
    format!("{:>10}", s)
}

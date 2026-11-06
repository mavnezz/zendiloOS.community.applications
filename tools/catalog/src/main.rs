//! `catalog`: checks the apps of the zendiloOS community catalog and builds
//! the index zendiloOS downloads.

mod app;
mod docker;
mod report;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use zendiloos_catalog::index::{Entry, Index, icon_path};

#[derive(Parser)]
#[command(version, about = "Checks the apps of the zendiloOS community catalog and builds its index")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Check apps against the format and the policy
    Validate {
        /// The folder that holds one folder per app
        #[arg(long, default_value = "apps")]
        apps: PathBuf,
        /// Skip the checks that ask the registries (images exist, architectures)
        #[arg(long)]
        offline: bool,
        /// Only these apps; all when none are named
        ids: Vec<String>,
    },
    /// Check every app (offline) and write index.json and the icons
    Index {
        /// The folder that holds one folder per app
        #[arg(long, default_value = "apps")]
        apps: PathBuf,
        /// Where to write the index
        #[arg(long)]
        out: PathBuf,
    },
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::from(2)
        }
    }
}

fn run(cli: Cli) -> Result<bool> {
    match cli.command {
        Command::Validate { apps, offline, ids } => {
            let folders = app_folders(&apps, &ids)?;
            let checked =
                folders.iter().map(|folder| app::check(folder, !offline)).collect::<Result<Vec<_>>>()?;
            Ok(report::print(&apps, &checked))
        }
        Command::Index { apps, out } => {
            let folders = app_folders(&apps, &[])?;
            let checked =
                folders.iter().map(|folder| app::check(folder, false)).collect::<Result<Vec<_>>>()?;
            if !report::print(&apps, &checked) {
                return Ok(false);
            }
            write_index(&out, checked.into_iter().filter_map(|checked| checked.app).collect())?;
            Ok(true)
        }
    }
}

/// The app folders below `apps`, sorted; only those named in `ids` if any.
fn app_folders(apps: &Path, ids: &[String]) -> Result<Vec<PathBuf>> {
    if !ids.is_empty() {
        return ids
            .iter()
            .map(|id| {
                let folder = apps.join(id);
                if !folder.is_dir() {
                    bail!("there is no app `{id}` in {}", apps.display());
                }
                Ok(folder)
            })
            .collect();
    }
    let mut folders = Vec::new();
    for entry in fs::read_dir(apps).with_context(|| format!("reading {}", apps.display()))? {
        let entry = entry?;
        let hidden = entry.file_name().to_string_lossy().starts_with('.');
        if entry.file_type()?.is_dir() && !hidden {
            folders.push(entry.path());
        }
    }
    folders.sort();
    Ok(folders)
}

fn write_index(out: &Path, apps: Vec<app::App>) -> Result<()> {
    fs::create_dir_all(out.join("icons")).with_context(|| format!("creating {}", out.display()))?;
    let mut entries = Vec::new();
    for app in apps {
        fs::write(out.join(icon_path(&app.manifest.id)), &app.icon)?;
        entries.push(Entry::new(app.manifest, &app.manifest_text, app.compose, &app.icon));
    }
    let count = entries.len();
    let json = serde_json::to_string_pretty(&Index::new(entries))?;
    fs::write(out.join("index.json"), json + "\n").context("writing index.json")?;
    let apps = if count == 1 { "app" } else { "apps" };
    println!("wrote {} with {count} {apps}", out.join("index.json").display());
    Ok(())
}

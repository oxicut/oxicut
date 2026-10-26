//! `oxicut models`: see, fetch and check the model files.

use std::io::Write;

use anyhow::{Result, bail};
use clap::Subcommand;
use oxicut_models::{ModelSpec, Progress, Status};

#[derive(Subcommand)]
pub enum ModelsCommand {
    /// Show which models are on disk and whether they pass their checksum.
    Status,
    /// Download the models (the MobileSAM pair unless ids are given).
    Download {
        /// Model ids, e.g. `mobilesam-encoder`. Default: the MobileSAM pair.
        ids: Vec<String>,
    },
    /// Check every present model against its checksum; fails if one is damaged.
    Verify,
    /// Print the folder the models live in.
    Path,
}

pub fn run(command: ModelsCommand) -> Result<()> {
    let dir = oxicut_models::models_dir()?;
    match command {
        ModelsCommand::Path => println!("{}", dir.display()),
        ModelsCommand::Status => {
            println!("models folder: {}", dir.display());
            for spec in oxicut_models::all() {
                println!("{:<18} {}", spec.id, describe(&dir, spec)?);
            }
        }
        ModelsCommand::Verify => {
            let mut damaged = 0;
            for spec in oxicut_models::all() {
                let state = oxicut_models::status(&dir, spec)?;
                if matches!(state, Status::Corrupt { .. }) {
                    damaged += 1;
                }
                println!("{:<18} {}", spec.id, describe(&dir, spec)?);
            }
            if damaged > 0 {
                bail!("{damaged} model file(s) failed their checksum");
            }
        }
        ModelsCommand::Download { ids } => {
            let specs: Vec<&ModelSpec> = if ids.is_empty() {
                oxicut_models::mobilesam().to_vec()
            } else {
                ids.iter()
                    .map(|id| {
                        oxicut_models::find(id).ok_or_else(|| anyhow::anyhow!("unknown model `{id}`"))
                    })
                    .collect::<Result<_>>()?
            };
            for spec in specs {
                let mut last_percent = u64::MAX;
                let path = oxicut_models::ensure_model(&dir, spec, |p: Progress| {
                    if let Some(total) = p.total.filter(|t| *t > 0) {
                        let percent = p.downloaded * 100 / total;
                        if percent != last_percent {
                            last_percent = percent;
                            eprint!("\r{:<18} {percent:>3}%", spec.id);
                            let _ = std::io::stderr().flush();
                        }
                    }
                })?;
                eprintln!("\r{:<18} ready: {}", spec.id, path.display());
            }
        }
    }
    Ok(())
}

fn describe(dir: &std::path::Path, spec: &ModelSpec) -> Result<String> {
    Ok(match oxicut_models::status(dir, spec)? {
        Status::Missing => "not downloaded".to_string(),
        Status::Ready => {
            let size = std::fs::metadata(oxicut_models::model_path(dir, spec))?.len();
            format!("ready ({size} bytes)")
        }
        Status::Corrupt { actual_sha256 } => {
            format!("DAMAGED (sha256 {actual_sha256}, expected {})", spec.sha256)
        }
    })
}

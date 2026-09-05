mod check;
mod dev;
mod hosts;
mod i18n;
mod imports;
mod manifest;
mod new;
mod pack;
mod panel;
mod scan;
mod schema;
mod sign_cmd;
mod wasm;

use clap::{Parser, Subcommand};
use modus_sign::SignatureStatus;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "modus", about = "Modus plugin SDK (ABI 2)")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    New {
        role: new::Role,
        #[arg(long)]
        id: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        author: Option<String>,
        #[arg(long)]
        dir: Option<PathBuf>,
        #[arg(long)]
        lang: Option<String>,
        #[arg(long, value_enum)]
        mode: Option<new::PanelMode>,
    },
    Check {
        path: Option<PathBuf>,
        #[arg(long)]
        trusted_keys: Option<PathBuf>,
    },
    Pack {
        path: Option<PathBuf>,
        #[arg(long)]
        sign: bool,
        #[arg(long)]
        key_file: Option<PathBuf>,
    },
    Keygen {
        #[arg(long)]
        out: PathBuf,
        #[arg(long, default_value = "modus-dev")]
        key_id: String,
        #[arg(long, default_value = "Modus")]
        issuer: String,
    },
    Dev {
        path: Option<PathBuf>,
        #[arg(long)]
        inject: Option<PathBuf>,
        #[arg(long)]
        token: Option<String>,
        #[arg(long)]
        token_file: Option<PathBuf>,
        #[arg(long)]
        account: Option<String>,
        #[arg(long)]
        replay: Option<PathBuf>,
        #[arg(long)]
        http_file: Option<PathBuf>,
        #[arg(long, help = "JSON payload(s) → Ready::Ui")]
        ui: Option<PathBuf>,
        #[arg(long, help = "JSON values overlay → Ready::Settings")]
        settings: Option<PathBuf>,
        #[arg(long, help = "JSON act-request → Ready::Act")]
        act: Option<PathBuf>,
    },
}

fn main() {
    if let Err(err) = run() {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let cli = Cli::parse();
    match cli.command {
        Command::New {
            role,
            id,
            name,
            author,
            dir,
            lang,
            mode,
        } => {
            let dest = new::scaffold(new::NewArgs {
                role,
                id,
                name,
                author,
                dir,
                lang,
                mode,
            })?;
            println!("created {}", dest.display());
        }
        Command::Check { path, trusted_keys } => {
            let target = resolve_path(path)?;
            if is_mplug(&target) {
                let manifest = check::check_mplug(&target)?;
                match sign_cmd::verify_mplug(&target, trusted_keys.as_deref()) {
                    Ok(SignatureStatus::Verified) => {
                        println!("ok {} {} (signed)", manifest.id, target.display());
                    }
                    Ok(SignatureStatus::Unsigned) => {
                        println!("ok {} {} (unsigned)", manifest.id, target.display());
                    }
                    Ok(status) => {
                        return Err(format!("подпись: {status:?}"));
                    }
                    Err(err) => return Err(err),
                }
            } else {
                let component = pack::compile_component(&target)?;
                let manifest = check::check_plugin_dir(&target, &component)?;
                println!("ok {} {}", manifest.id, target.display());
            }
        }
        Command::Pack {
            path,
            sign,
            key_file,
        } => {
            let target = resolve_path(path)?;
            let out = pack::pack(&target)?;
            if sign || sign_cmd::resolve_sign_key(key_file.as_deref())?.is_some() {
                let key_path = sign_cmd::resolve_sign_key(key_file.as_deref())?
                    .ok_or_else(|| "pack --sign: нужен --key-file или MODUS_SIGN_KEY".to_string())?;
                sign_cmd::sign_mplug(&out, &key_path)?;
                println!("signed {}", out.display());
            } else {
                println!("packed {}", out.display());
            }
        }
        Command::Keygen { out, key_id, issuer } => {
            sign_cmd::keygen(&out, &key_id, &issuer)?;
        }
        Command::Dev {
            path,
            inject,
            token,
            token_file,
            account,
            replay,
            http_file,
            ui,
            settings,
            act,
        } => {
            let target = resolve_path(path)?;
            if is_mplug(&target) {
                return Err("dev: нужен каталог crate, не .mplug".into());
            }
            dev::run(dev::DevArgs {
                path: Some(target),
                inject,
                token,
                token_file,
                account,
                replay,
                http_file,
                ui,
                settings,
                act,
            })?;
        }
    }
    Ok(())
}

fn resolve_path(path: Option<PathBuf>) -> Result<PathBuf, String> {
    match path {
        Some(path) => Ok(path),
        None => std::env::current_dir().map_err(|err| format!("cwd: {err}")),
    }
}

fn is_mplug(path: &Path) -> bool {
    path.is_file()
        && path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("mplug"))
}

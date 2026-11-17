//! `mychart config path|show|set|unset`.

use pk_cli_core::{output, CliError};

use super::Ctx;
use crate::cli::ConfigCmd;
use crate::config::Config;
use crate::session::ProfileSettings;

pub fn run(ctx: &Ctx, cmd: &ConfigCmd) -> Result<(), CliError> {
    match cmd {
        ConfigCmd::Path => {
            let path = ctx.store.path()?.display().to_string();
            output::emit(
                ctx.json(),
                "config-path",
                serde_json::json!({ "path": path }),
                |p| println!("{}", p["path"].as_str().unwrap_or_default()),
            );
            Ok(())
        }
        ConfigCmd::Show => {
            let cfg: Config = ctx.config()?;
            let mut v = super::to_json(&cfg)?;
            // A profile's patient ids and labels stay out of what is pasted
            // to debug a setting: `profiles list` and `patients list` show
            // them.
            if let Some(o) = v.as_object_mut().filter(|_| !cfg.profiles.is_empty()) {
                let mut profiles = Vec::new();
                for name in cfg.profiles.names() {
                    let settings: ProfileSettings = cfg.profiles.settings(name)?;
                    profiles.push(serde_json::json!({
                        "name": name.as_str(),
                        "patients": settings.contexts.len(),
                    }));
                }
                o.insert("profiles".into(), profiles.into());
            }
            if ctx.json() {
                output::json(&output::tagged("config", v));
            } else if v.as_object().is_some_and(|o| o.is_empty()) {
                eprintln!("no settings (`mychart config set client_id <ID>`)");
            } else {
                output::render(&v);
            }
            Ok(())
        }
        ConfigCmd::Set { key, value } => {
            let mut cfg: Config = ctx.config()?;
            cfg.set(key, value)?;
            // A port the published page does not forward to is refused
            // here, whichever of the two keys came second, and the file
            // keeps its old value.
            cfg.check_redirect()?;
            ctx.save_config(&cfg)?;
            ctx.note(format!("{key} set"));
            Ok(())
        }
        ConfigCmd::Unset { key } => {
            let mut cfg: Config = ctx.config()?;
            cfg.unset(key)?;
            ctx.save_config(&cfg)?;
            ctx.note(format!("{key} unset"));
            Ok(())
        }
    }
}

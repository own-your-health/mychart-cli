//! `mychart profiles list|add|remove|migrate`: the logins this install signs
//! in as. A profile is one MyChart account at one organization; its patient
//! contexts are in its config settings and its sign-ins in the vault, under
//! `<profile>/<organization>/<patient>`.

use pk_cli_config::profile::ProfileName;
use pk_cli_core::{confirm, output, CliError};
use pk_med_smart::{ProfileSessions, SessionStore};
use serde_json::{json, Value};

use super::{to_json, Ctx, BIN};
use crate::cli::ProfilesCmd;
use crate::session::{Context, ProfileSettings};

pub fn run(ctx: &Ctx, cmd: &ProfilesCmd) -> Result<(), CliError> {
    match cmd {
        ProfilesCmd::List => list(ctx),
        ProfilesCmd::Add { name } => add(ctx, name),
        ProfilesCmd::Remove { name, force } => remove(ctx, name, *force),
        ProfilesCmd::Migrate { name } => migrate(ctx, name),
    }
}

fn list(ctx: &Ctx) -> Result<(), CliError> {
    let cfg = ctx.config()?;
    let list = cfg.profiles.list();
    if ctx.json() {
        output::json(&to_json(&list)?);
        return Ok(());
    }
    if list.profiles.is_empty() {
        println!("No profiles: `{BIN} profiles add <NAME>`, one per MyChart account");
        return Ok(());
    }
    let mut rows = Vec::new();
    for name in cfg.profiles.names() {
        let settings: ProfileSettings = cfg.profiles.settings(name)?;
        let mut orgs: Vec<&str> = settings
            .contexts
            .iter()
            .map(|c| c.organization_name.as_str())
            .collect();
        orgs.dedup();
        rows.push(json!({
            "profile": name.as_str(),
            "patients": settings.contexts.len(),
            "organizations": orgs.join(", "),
        }));
    }
    output::table(&rows);
    Ok(())
}

fn add(ctx: &Ctx, name: &str) -> Result<(), CliError> {
    let name = ProfileName::parse(name)?;
    let mut cfg = ctx.config()?;
    let added = cfg.profiles.add(name.clone());
    if added {
        ctx.store.save(&cfg)?;
    }
    output::emit(
        ctx.json(),
        "profile-add",
        json!({ "profile": name.as_str(), "added": added }),
        |_| {
            if added {
                println!("Added profile {name}. Sign in: `{BIN} auth login --profile {name} --org <ORG>`");
            } else {
                println!("Profile {name} is already configured");
            }
        },
    );
    Ok(())
}

/// The profile's sign-ins, its browser profile, then the profile itself.
/// Asks first unless `--force`; exit 6 when it cannot ask.
fn remove(ctx: &Ctx, name: &str, force: bool) -> Result<(), CliError> {
    let name = ProfileName::parse(name)?;
    let cfg = ctx.config()?;
    if !cfg.profiles.contains(&name) {
        return Err(CliError::NotFound(format!("no profile `{name}`"))
            .with_remedy(format!("`{BIN} profiles list` shows the configured ones")));
    }
    confirm::gate(
        &ctx.common,
        force,
        &format!("Remove profile {name}, its sign-ins and its browser profile?"),
    )?;
    let contexts = ctx.contexts(&name)?;
    let deleted = ctx.lock.forget(|| match ctx.vault.shared() {
        Some(vault) => ProfileSessions::new(vault, name.clone()).delete_all(),
        None => {
            let sessions = ctx.sessions(&name);
            let mut n = 0;
            for c in &contexts {
                n += usize::from(sessions.delete(&c.account())?);
            }
            Ok(n)
        }
    })?;
    let browser = (ctx.forget_browser)(&name)?;
    let mut cfg = ctx.config()?;
    cfg.profiles.remove(&name);
    ctx.store.save(&cfg)?;
    output::emit(
        ctx.json(),
        "profile-remove",
        json!({
            "profile": name.as_str(),
            "sign_ins_deleted": deleted,
            "browser_profile_removed": browser,
        }),
        |p| {
            println!(
                "Removed profile {name}: {} sign-in{} deleted{}",
                deleted,
                if deleted == 1 { "" } else { "s" },
                if p["browser_profile_removed"] == Value::Bool(true) {
                    ", browser profile removed"
                } else {
                    ""
                }
            )
        },
    );
    Ok(())
}

/// mychart 0.4's sign-ins (a Keychain item each, listed in `contexts.json`)
/// into `name`, through pk-med-smart's `migrate_sessions`: no network, no
/// new approval in MyChart, and a failure leaves each old item in place
/// for the next run. Contexts whose sign-in reached the profile move with
/// it; the others are dropped, since nothing is left to sign in with.
fn migrate(ctx: &Ctx, name: &str) -> Result<(), CliError> {
    let name = ProfileName::parse(name)?;
    let index = ctx.legacy_index()?;
    let rows = index.load()?;
    let mut cfg = ctx.config()?;
    let added = cfg.profiles.add(name.clone());
    if added {
        ctx.store.save(&cfg)?;
    }
    let vault = ctx.vault.shared().ok_or_else(|| {
        CliError::Other(
            "moving sign-ins needs the CLI's vault, which this run does not have".into(),
        )
    })?;
    let accounts: Vec<String> = rows.iter().map(Context::account).collect();
    let done = pk_med_smart::migrate_sessions(
        &*ctx.legacy,
        &ProfileSessions::new(vault, name.clone()),
        &accounts,
        &ctx.lock,
    )?;
    let sessions = ctx.sessions(&name);
    let mut moved = Vec::new();
    for row in rows {
        if sessions.read(&row.account())?.is_some() {
            moved.push(row);
        }
    }
    let contexts = moved.len();
    ctx.update_profile(&name, |settings| {
        for row in moved {
            settings.upsert(row);
        }
    })?;
    match std::fs::remove_file(index.path()) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(CliError::Other(format!(
                "removing {}: {e}",
                index.path().display()
            )))
        }
    }
    output::emit(
        ctx.json(),
        "profile-migrate",
        json!({
            "profile": name.as_str(),
            "profile_added": added,
            "moved": done.moved,
            "kept": done.kept,
            "absent": done.absent,
            "contexts": contexts,
        }),
        |_| {
            println!(
                "Profile {name}: {} sign-in{} moved, {} already there, {} not found; {} patient \
                 context{} in the profile",
                done.moved,
                if done.moved == 1 { "" } else { "s" },
                done.kept,
                done.absent,
                contexts,
                if contexts == 1 { "" } else { "s" },
            )
        },
    );
    Ok(())
}

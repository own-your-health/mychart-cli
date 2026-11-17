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
        ProfilesCmd::Migrate { name, org, patient } => migrate(ctx, name, org.as_deref(), patient),
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
    ctx.note_vault();
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

/// One login's sign-ins from mychart 0.4 (a Keychain item each, listed in
/// `contexts.json`) into `name`, through pk-med-smart's `migrate_sessions`:
/// no network, no new approval in MyChart, and a failure leaves each old
/// item in place for the next run. `--org` and `--patient` choose the rows;
/// rows that span more than one login (organization and signed-in user)
/// are refused, since a profile is one login. The rows not chosen stay in
/// `contexts.json` for another profile's migration. One-way: mychart 0.4
/// finds neither the items nor the rows that moved.
fn migrate(ctx: &Ctx, name: &str, org: Option<&str>, patients: &[String]) -> Result<(), CliError> {
    let name = ProfileName::parse(name)?;
    let index = ctx.legacy_index()?;
    let rows = index.load()?;
    let wanted = |c: &Context| {
        org.is_none_or(|o| c.organization == o || c.organization_name.eq_ignore_ascii_case(o))
            && (patients.is_empty()
                || patients
                    .iter()
                    .any(|p| c.id == *p || c.label.as_deref() == Some(p.as_str())))
    };
    let (chosen, rest): (Vec<Context>, Vec<Context>) = rows.into_iter().partition(wanted);
    if let Some(p) = patients.iter().find(|p| {
        !chosen
            .iter()
            .any(|c| c.id == **p || c.label.as_deref() == Some(p.as_str()))
    }) {
        return Err(CliError::NotFound(format!(
            "no sign-in mychart 0.4 kept is patient `{p}`{}",
            org.map(|o| format!(" at {o}")).unwrap_or_default()
        )));
    }
    if chosen.is_empty() && org.is_some() {
        return Err(CliError::NotFound(format!(
            "no sign-in mychart 0.4 kept is at {}",
            org.unwrap_or_default()
        )));
    }
    let logins = logins(&chosen);
    if logins.len() > 1 {
        return Err(CliError::Usage(format!(
            "mychart 0.4 kept {} logins' sign-ins and a profile holds one: {}",
            logins.len(),
            logins.join("; ")
        ))
        .with_remedy(format!(
            "move one login per profile: `{BIN} profiles migrate <NAME> --org <ORG> [--patient \
             <WHO>]`"
        )));
    }
    ctx.note_vault();
    let vault = ctx.vault.shared().ok_or_else(|| {
        CliError::Other(
            "moving sign-ins needs the CLI's vault, which this run does not have".into(),
        )
    })?;
    let mut cfg = ctx.config()?;
    let added = cfg.profiles.add(name.clone());
    if added {
        ctx.store.save(&cfg)?;
    }
    let accounts: Vec<String> = chosen.iter().map(Context::account).collect();
    let done = pk_med_smart::migrate_sessions(
        &*ctx.legacy,
        &ProfileSessions::new(vault, name.clone()),
        &accounts,
        &ctx.lock,
    )?;
    // Every chosen item moved, was already here, or was never there (a
    // failure is the error above): a row whose sign-in is not here now has
    // nothing left to sign in with.
    let sessions = ctx.sessions(&name);
    let mut moved = Vec::new();
    for row in chosen {
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
    let left = rest.len();
    if rest.is_empty() {
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
    } else {
        index.save(&rest)?;
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
            "left": left,
        }),
        |_| {
            println!(
                "Profile {name}: {} sign-in{} moved, {} already there, {} not found; {} patient \
                 context{} in the profile; mychart 0.4 no longer finds them",
                done.moved,
                if done.moved == 1 { "" } else { "s" },
                done.kept,
                done.absent,
                contexts,
                if contexts == 1 { "" } else { "s" },
            );
            if left > 0 {
                println!(
                    "{left} of mychart 0.4's sign-ins are another login's and stay for its \
                     profile: `{BIN} profiles migrate <NAME> --org <ORG>`"
                );
            }
        },
    );
    Ok(())
}

/// Each login among `rows`, described: its organization and the patients
/// it signed in. A login is an organization and the user who signed in
/// (`fhirUser`), so a parent's own row and a child's row through proxy
/// access are one.
fn logins(rows: &[Context]) -> Vec<String> {
    let mut seen: Vec<(&str, Option<&str>, String, Vec<String>)> = Vec::new();
    for c in rows {
        let key = (c.organization.as_str(), c.fhir_user.as_deref());
        match seen.iter_mut().find(|(o, u, ..)| (*o, *u) == key) {
            Some((.., who)) => who.push(c.display()),
            None => seen.push((
                key.0,
                key.1,
                format!("{} ({})", c.organization_name, c.organization),
                vec![c.display()],
            )),
        }
    }
    seen.into_iter()
        .map(|(_, _, org, who)| format!("{org}: {}", who.join(", ")))
        .collect()
}

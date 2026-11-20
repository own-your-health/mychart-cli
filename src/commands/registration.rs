//! `mychart registration`: what to enter in Epic's developer portal.

use pk_cli_core::{output, CliError};
use serde_json::json;

use super::Ctx;
use crate::cli::RegistrationArgs;
use crate::registration::{
    client_line, client_status, pages, store_commands, APIS_KEPT, APIS_OUTSIDE, DOC_URL, PORTAL,
};

pub fn run(ctx: &Ctx, args: &RegistrationArgs) -> Result<(), CliError> {
    let cfg = ctx.config()?;
    let pages = pages(&cfg);
    if args.apis && !ctx.json() {
        for name in APIS_KEPT.iter().chain(APIS_OUTSIDE) {
            println!("{name}");
        }
        return Ok(());
    }
    let store: Vec<_> = store_commands()
        .into_iter()
        .map(|(when, command)| json!({ "when": when, "command": command }))
        .collect();
    let v = json!({
        "portal": PORTAL,
        "doc": DOC_URL,
        "launch": "standalone patient launch",
        "client_type": "public, with dynamic client registration (JWT bearer grant)",
        "pkce": "S256",
        "scope": cfg.scope(),
        "pages": pages,
        "apis": {
            "records_the_organization_keeps": APIS_KEPT,
            "copies_other_organizations_sent": APIS_OUTSIDE,
        },
        "store": store,
        "configured": client_status(&cfg),
    });
    output::emit(ctx.json(), "registration", v, |v| {
        println!("Register an app at {PORTAL} (free; about 20 minutes). Step by step: {DOC_URL}");
        println!(
            "Standalone patient launch, public client, PKCE S256, dynamic client registration; scope `{}`.",
            v["scope"].as_str().unwrap_or_default()
        );
        for page in v["pages"].as_array().into_iter().flatten() {
            println!("\n{}", page["page"].as_str().unwrap_or_default());
            for field in page["fields"].as_array().into_iter().flatten() {
                let value = field["value"].as_str().unwrap_or_default();
                let mut lines = value.lines();
                println!(
                    "  {}: {}",
                    field["field"].as_str().unwrap_or_default(),
                    lines.next().unwrap_or_default()
                );
                for more in lines {
                    println!("      {more}");
                }
                if let Some(note) = field["note"].as_str() {
                    println!("      ({note})");
                }
            }
        }
        println!("\nThen");
        for s in v["store"].as_array().into_iter().flatten() {
            println!(
                "  {}: `{}`",
                s["when"].as_str().unwrap_or_default(),
                s["command"].as_str().unwrap_or_default()
            );
        }
        println!("\nThis install: {}", client_line(&cfg));
    });
    Ok(())
}

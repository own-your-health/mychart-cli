//! `mychart registration`: what to enter in Epic's developer portal.

use pk_cli_core::{output, CliError};
use serde_json::json;

use super::Ctx;
use crate::cli::RegistrationArgs;
use crate::registration::{registration, APIS_KEPT, APIS_OUTSIDE};

pub fn run(ctx: &Ctx, args: &RegistrationArgs) -> Result<(), CliError> {
    let cfg = ctx.config()?;
    if args.apis {
        let v = json!({
            "records_the_organization_keeps": APIS_KEPT,
            "copies_other_organizations_sent": APIS_OUTSIDE,
        });
        output::emit(ctx.json(), "registration-apis", v, |_| {
            for name in APIS_KEPT.iter().chain(APIS_OUTSIDE) {
                println!("{name}");
            }
        });
        return Ok(());
    }
    let reg = registration(&cfg);
    if ctx.json() {
        let v = serde_json::to_value(&reg)
            .map_err(|e| CliError::Other(format!("serializing the registration: {e}")))?;
        output::json(&v);
    } else {
        print!("{}", reg.text());
    }
    Ok(())
}

//! `mychart registration`: what to enter in Epic's developer portal.

use pk_cli_core::{output, CliError};

use super::Ctx;
use crate::registration::{registration, Which};

pub fn run(ctx: &Ctx) -> Result<(), CliError> {
    let reg = registration(&ctx.config()?, Which::Production);
    if ctx.json() {
        let v = serde_json::to_value(&reg)
            .map_err(|e| CliError::Other(format!("serializing the registration: {e}")))?;
        output::json(&v);
    } else {
        print!("{}", reg.text());
    }
    Ok(())
}

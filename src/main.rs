use clap::{CommandFactory, Parser};
use pk_cli_core::info::{AuthInfo, CliInfo};
use pk_cli_core::{output, CliError};
use pk_cli_selfupdate::Updater;

use mychart::cli::{Cli, Command};
use mychart::commands::{self, Ctx, BIN, REPO};

/// The profiles `info` declares.
const PROFILES: &[&str] = &["health-records/v1", pk_cli_documents::PROFILE];

fn main() {
    let cli = Cli::parse();
    if let Err(e) = run(&cli) {
        std::process::exit(output::fail(&e, cli.common.json));
    }
}

fn run(cli: &Cli) -> Result<(), CliError> {
    let ctx = Ctx::for_binary(cli.common.clone(), cli.config.clone(), cli.profile.clone())?;
    match &cli.command {
        Command::Orgs(cmd) => commands::orgs::run(&ctx, cmd),
        Command::Auth(cmd) => commands::auth::run(&ctx, cmd),
        Command::Patients(cmd) => commands::patients::run(&ctx, cmd),
        Command::Records(cmd) => commands::records::run(&ctx, cmd),
        Command::Documents(cmd) => commands::documents::run(&ctx, cmd),
        Command::Api(args) => commands::api::run(&ctx, args),
        Command::Profiles(cmd) => commands::profiles::run(&ctx, cmd),
        Command::Config(cmd) => commands::config::run(&ctx, cmd),
        Command::Registration(args) => commands::registration::run(&ctx, args),
        Command::SelfUpdate(args) => Updater {
            repo: REPO.into(),
            binary: BIN.into(),
            target: env!("BUILD_TARGET").into(),
            current: env!("CARGO_PKG_VERSION").into(),
        }
        .run(args, cli.common.json, cli.common.quiet),
        Command::Completions { shell } => {
            clap_complete::generate(*shell, &mut Cli::command(), BIN, &mut std::io::stdout());
            Ok(())
        }
        Command::Info => {
            let info = CliInfo::new(
                BIN,
                env!("CARGO_PKG_VERSION"),
                &format!("https://github.com/{REPO}"),
                AuthInfo {
                    required: true,
                    method: "browser-session".into(),
                    login_hint: Some(format!(
                        "{BIN} profiles add <NAME>; {BIN} auth login --profile <NAME> --org <ORG> --label <WHO>"
                    )),
                },
                &[
                    "orgs",
                    "auth",
                    "patients",
                    "records",
                    "documents",
                    "api",
                    "profiles",
                    "config",
                    pk_med_records::REGISTRATION,
                    pk_med_records::LOGIN_PROFILES,
                ],
            )
            .with_profiles(PROFILES);
            // Whether this install has its client ids. A config that does
            // not read has none to sign in with: `info` still answers, and
            // says why on stderr.
            let cfg = ctx.config().unwrap_or_else(|e| {
                eprintln!("warning: the config does not read: {}", e.message());
                mychart::config::Config::default()
            });
            output::json(&mychart::registration::with_clients(&info, &cfg)?);
            Ok(())
        }
    }
}

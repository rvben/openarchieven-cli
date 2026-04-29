use std::process::ExitCode;

use clap::Parser;

use openarchieven::cli::{CacheCmd, Cli, Cmd};
use openarchieven::error::{Error, ErrorKind, emit_json};

fn main() -> ExitCode {
    let cli = Cli::parse();
    match dispatch(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            let _ = emit_json(&mut std::io::stderr().lock(), &err);
            ExitCode::from(err.kind().exit_code())
        }
    }
}

fn dispatch(cli: Cli) -> Result<(), Error> {
    match cli.command {
        Cmd::Version => {
            println!("openarchieven {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Cmd::Schema => {
            let schema = openarchieven::schema_cmd::build();
            let json = serde_json::to_string_pretty(&schema).expect("schema always serializes");
            println!("{json}");
            Ok(())
        }
        Cmd::Cache(CacheCmd::Info) => Err(Error::new(
            ErrorKind::Validation,
            "cache info: not yet implemented",
        )),
        Cmd::Cache(CacheCmd::Clear { .. }) => Err(Error::new(
            ErrorKind::Validation,
            "cache clear: not yet implemented",
        )),
        Cmd::Cache(CacheCmd::Prune) => Err(Error::new(
            ErrorKind::Validation,
            "cache prune: not yet implemented",
        )),
        _ => Err(Error::new(
            ErrorKind::Validation,
            "command not yet implemented",
        )),
    }
}

use clap::{Parser, Subcommand};
use hmi_core::{
    Result,
    cli::{self, CommandResult},
};
use std::path::PathBuf;
#[derive(Parser)]
#[command(name = "hmi-parse", version, about = "检查已有HMI并导出新版工程")]
struct Args {
    #[command(flatten)]
    reporting: cli::OutputOptions,
    #[command(subcommand)]
    command: Action,
}
#[derive(Subcommand)]
enum Action {
    Inspect {
        file: PathBuf,
    },
    Export {
        file: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
    },
}
fn run(args: &Args, session: &cli::Session) -> Result<CommandResult> {
    match &args.command {
        Action::Inspect { file } => {
            let task = session.report.task(
                "inspect",
                None,
                "validate",
                format!("HMI {}", file.display()),
            );
            let result = hmi_parser::inspect(file).map_err(|e| task.error(e))?;
            task.finish(false);
            Ok(cli::success(result, "HMI结构检查通过。"))
        }
        Action::Export { file, output } => Ok(cli::success(
            serde_json::json!(hmi_parser::export_with_report(
                file,
                output,
                &session.report
            )?),
            format!("新版工程已导出：{}", output.display()),
        )),
    }
}
fn main() -> std::process::ExitCode {
    let args: Args = match cli::parse() {
        Ok(v) => v,
        Err(c) => return c,
    };
    let session = cli::Session::new(
        "hmi-parse",
        env!("CARGO_PKG_VERSION"),
        &cli::command_name::<Args>(),
        &args.reporting,
    );
    let outcome = run(&args, &session);
    session.finish(outcome)
}

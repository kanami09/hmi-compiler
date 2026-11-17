use clap::{Parser, Subcommand};
use hmi_core::{
    Result,
    cli::{self, CommandResult},
};
use std::path::PathBuf;
#[derive(Parser)]
#[command(name = "hmi-parse", version, about = "检查已有HMI并导出新版工程")]
struct Args {
    #[arg(long, global = true)]
    json: bool,
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
fn run(args: &Args) -> Result<CommandResult> {
    match &args.command {
        Action::Inspect { file } => Ok(cli::success(
            hmi_parser::inspect(file)?,
            "HMI结构检查通过。",
        )),
        Action::Export { file, output } => Ok(cli::success(
            serde_json::json!(hmi_parser::export(file, output)?),
            format!("新版工程已导出：{}", output.display()),
        )),
    }
}
fn main() -> std::process::ExitCode {
    let args: Args = match cli::parse() {
        Ok(v) => v,
        Err(c) => return c,
    };
    cli::finish(run(&args), args.json)
}

use clap::Parser;
use hmi_core::{
    Result,
    cli::{self, CommandResult},
};
use std::{path::PathBuf, process::ExitCode};

#[derive(Parser)]
#[command(name = "hmi-link", version, about = "链接页面目标并重建 HMI")]
struct Args {
    source: PathBuf,
    #[arg(short, long, required_unless_present = "check")]
    output: Option<PathBuf>,
    #[arg(long, conflicts_with_all=["output","map"])]
    check: bool,
    #[arg(long)]
    map: Option<PathBuf>,
    #[command(flatten)]
    reporting: cli::OutputOptions,
    #[arg(long = "deny-warnings")]
    deny_warnings: bool,
    #[arg(short='W', value_parser=["error"], help="将警告作为错误：-Werror")]
    warnings: Option<String>,
}
fn run(args: &Args, session: &cli::Session) -> Result<CommandResult> {
    let _ = session;
    let linked =
        hmi_linker::from_manifest(&args.source, args.deny_warnings || args.warnings.is_some())?;
    let inputs: Vec<_> = linked.inputs.iter().map(|p| p.as_path()).collect();
    if let Some(output) = &args.output {
        hmi_linker::publish::Publication::prepare(&linked, output, args.map.as_deref(), &inputs)?
            .write()?;
    }
    Ok(CommandResult {
        result: serde_json::json!({"output":args.output,"output_written":args.output.is_some(),"size":linked.bytes.len(),"sha256":hmi_core::sha256(&linked.bytes),"map":linked.map,"external_files":linked.external_files.iter().map(|f| &f.path).collect::<Vec<_>>()}),
        message: if args.check {
            "链接检查通过。".into()
        } else {
            format!("重建完成：{}", args.output.as_ref().unwrap().display())
        },
        diagnostics: linked.diagnostics,
    })
}
fn main() -> ExitCode {
    let args: Args = match cli::parse() {
        Ok(args) => args,
        Err(code) => return code,
    };
    let session = cli::Session::new(
        "hmi-link",
        env!("CARGO_PKG_VERSION"),
        &cli::command_name::<Args>(),
        &args.reporting,
    );
    let outcome = run(&args, &session);
    session.finish(outcome)
}

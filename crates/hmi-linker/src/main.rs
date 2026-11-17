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
    if let Some(output) = &args.output {
        session.report.plan_output("hmi", output)?;
    }
    if let Some(map) = &args.map {
        session.report.plan_output("link_map", map)?;
    }
    let linked = hmi_linker::from_manifest_with_report(
        &args.source,
        args.deny_warnings || args.warnings.is_some(),
        &session.report,
    )?;
    let inputs: Vec<_> = linked.inputs.iter().map(|p| p.as_path()).collect();
    if let Some(output) = &args.output {
        session.report.stage(
            "publish",
            "publish",
            format!("{} 及配套文件", output.display()),
        );
        hmi_linker::publish::Publication::prepare(&linked, output, args.map.as_deref(), &inputs)?
            .write_with_report(&session.report)?;
    }
    Ok(CommandResult {
        result: serde_json::json!({"output":args.output,"output_written":args.output.is_some(),"size":linked.bytes.len(),"sha256":hmi_core::sha256(&linked.bytes),"map":linked.map,"external_files":linked.external_files.iter().map(|f| &f.path).collect::<Vec<_>>()}),
        message: if args.check {
            "HMI 链接结果，未写入文件".into()
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
        if args.check { "check" } else { "link" },
        &args.reporting,
    );
    let outcome = run(&args, &session);
    session.finish(outcome)
}

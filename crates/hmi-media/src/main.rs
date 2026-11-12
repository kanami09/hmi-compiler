use clap::{Parser, Subcommand};
use hmi_artifact::asset::AssetObject;
use hmi_core::{
    Result,
    cli::{self, CommandResult},
    file,
    target::Target,
};
use std::{path::PathBuf, process::ExitCode};

#[derive(Parser)]
#[command(
    name = "hmi-media",
    version,
    about = "编码 HMI 媒体资产，当前支持 K/T RGB565 图片"
)]
struct Args {
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// 解码并检查图片输入及处理参数，不写输出。
    Probe {
        source: PathBuf,
        #[arg(long)]
        target: PathBuf,
    },
    /// 编码图片，生成包含运行数据和编辑源的 .hma。
    Encode {
        source: PathBuf,
        #[arg(long)]
        target: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// 校验并查看资产目标的描述和成员。
    Inspect { file: PathBuf },
}
fn run(args: &Args) -> Result<CommandResult> {
    match &args.command {
        Command::Inspect { file } => {
            let asset = AssetObject::read(file)?;
            Ok(cli::success(
                serde_json::json!({"metadata":asset.metadata,"payload_size":asset.payload.len()}),
                "资产目标校验通过。",
            ))
        }
        Command::Probe { source, target } => {
            let target: Target = file::read_json(target)?;
            let probe = hmi_media::probe(source, &target)?;
            Ok(cli::success(
                serde_json::json!({"image":probe.info}),
                "图片检查通过。",
            ))
        }
        Command::Encode {
            source,
            target,
            output,
        } => {
            let config: Target = file::read_json(target)?;
            let encoded = hmi_media::encode(source, &config)?;
            let mut inputs = encoded
                .inputs
                .iter()
                .map(|p| p.as_path())
                .collect::<Vec<_>>();
            inputs.push(target);
            encoded.object.write(output, &inputs)?;
            Ok(cli::success(
                serde_json::json!({"output":output,"image":encoded.object.metadata.image}),
                format!("图片编码完成：{}", output.display()),
            ))
        }
    }
}
fn main() -> ExitCode {
    let args: Args = match cli::parse() {
        Ok(a) => a,
        Err(code) => return code,
    };
    cli::finish(run(&args), args.json)
}

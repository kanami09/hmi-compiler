"""根据已验证的 Windows x64 FFmpeg SDK 再生绑定；普通构建不需要运行。"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess


def main():
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--include", type=Path, required=True, help="已验证 SDK 的 include 目录")
    parser.add_argument("--offline", action="store_true", help="仅使用缓存的生成器依赖")
    args = parser.parse_args()
    if os.name != "nt" or platform.machine().lower() not in {"amd64", "x86_64"}:
        parser.error("当前绑定需在 Windows x64 环境生成")
    source = root / "crates/hmi-media/src/ffmpeg"
    include = args.include.resolve()
    for name, digest in json.loads((source / "headers.json").read_bytes()).items():
        path = include / name
        if not path.is_file() or hashlib.sha256(path.read_bytes()).hexdigest() != digest:
            parser.error(f"SDK 头文件与已验证版本不一致：{name}。升级时须一起审阅绑定和最低 ABI 版本。")
    helper = root / "target/ffmpeg-bindings-generator"
    (helper / "src").mkdir(parents=True, exist_ok=True)
    (helper / "Cargo.toml").write_text('[package]\nname="hmi-ffmpeg-bindings-generator"\nversion="0.0.0"\nedition="2024"\n[workspace]\n[dependencies]\nbindgen="=0.72.1"\n', encoding="utf-8")
    (helper / "src/main.rs").write_text('''fn main() {
    let args=std::env::args().skip(1).collect::<Vec<_>>();
    bindgen::Builder::default().header(&args[1]).clang_arg(format!("-I{}",args[0])).clang_arg("--target=x86_64-pc-windows-msvc")
        .allowlist_type("^(AVFormatContext|AVStream|AVCodecParameters|AVFrame|AVPacket|AVRational|AVChannelLayout|AVIOInterruptCB|AVPacketSideData|av_format_control_message)$")
        .allowlist_var("^HMI_.*").allowlist_recursively(false).generate_comments(false).derive_debug(false)
        .raw_line("// Generated from FFmpeg n9.0.2-49-gc179411bbb headers with bindgen 0.72.1.")
        .generate().expect("generate FFmpeg bindings").write_to_file(&args[2]).expect("write bindings");
}
''', encoding="utf-8")
    command = ["cargo", "run", "--manifest-path", str(helper / "Cargo.toml")]
    if args.offline:
        command.append("--offline")
    command.extend(["--", str(include), str(source / "wrapper.h"), str(source / "bindings.rs")])
    subprocess.run(command, check=True, env={**os.environ, "CARGO_INCREMENTAL": "0"})


if __name__ == "__main__":
    main()

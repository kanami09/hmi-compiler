"""用现有HMI验证冷构建、增量构建、全量重建及清理；不访问虚拟SD。"""

import argparse
from datetime import date
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time
import tomllib


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def tree(directory):
    return {str(p.relative_to(directory)): digest(p)
            for p in directory.rglob("*") if p.is_file()}


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--inputs", type=Path, nargs="+", required=True)
    parser.add_argument("--output", type=Path, default=root / "target/acceptance-incremental")
    parser.add_argument("--binaries", type=Path, default=root / "target/debug")
    args = parser.parse_args()
    output = args.output.resolve()
    if output.exists():
        parser.error("输出目录已存在，请使用 --output 指定新目录")
    inputs = [p.resolve() for p in args.inputs]
    if any(not p.is_file() for p in inputs):
        parser.error("输入HMI文件不存在")
    suffix = ".exe" if os.name == "nt" else ""
    binaries = args.binaries.resolve()
    tools = {name: binaries / (name + suffix) for name in ["hmi", "hmi-link"]}
    if any(not path.is_file() for path in tools.values()):
        parser.error("请先运行 cargo build --workspace --bins --locked")
    # 导入的是自包含的原生包，构建过程无需加载 FFmpeg。
    environment = dict(os.environ, HMI_FFMPEG_DIR=str(output / "unavailable-backend"))
    output.mkdir(parents=True)
    report = {"generated_on": str(date.today()), "stage": "incremental-build", "cases": []}

    def run(tool, *arguments):
        started = time.monotonic()
        command = [str(tools[tool]), "--json", *map(str, arguments)]
        result = subprocess.run(command, env=environment, capture_output=True, encoding="utf-8")
        parsed = json.loads(result.stdout)
        if result.returncode or not parsed["ok"]:
            raise RuntimeError(f"命令失败：{command}\n{result.stdout}\n{result.stderr}")
        return parsed["result"], round(time.monotonic() - started, 3)

    for index, source in enumerate(inputs):
        case = output / f"case{index}"
        project = case / "project"
        source_digest = digest(source)
        run("hmi", "import", source, "-o", project)
        run("hmi", "check", project)
        assert not (project / ".build").exists()
        manifest = tomllib.loads((project / "hmi.toml").read_text(encoding="utf-8"))
        units, assets = len(manifest["pages"]["sources"]) + 1, len(manifest["assets"]["items"])
        preserved = tree(project / ".preserve")
        cold, cold_time = run("hmi", "build", project)
        final = case / "published" / source.name
        warm, warm_time = run("hmi", "build", project, "-o", final)
        forced, forced_time = run("hmi", "rebuild", project, "-o", final)
        assert cold["build"] == {"compiled": units, "reused_objects": 0, "encoded": assets, "reused_assets": 0}
        assert warm["build"] == {"compiled": 0, "reused_objects": units, "encoded": 0, "reused_assets": assets}
        assert forced["build"] == cold["build"]
        assert cold["sha256"] == warm["sha256"] == forced["sha256"]
        standalone = case / "standalone" / source.name
        run("hmi-link", project / ".build/hmi-toolchain/link.toml", "-o", standalone)
        assert digest(standalone) == digest(final) == cold["sha256"]
        published = tree(case / "published")
        default_output = tree(project / "dist")
        cleaned, _ = run("hmi", "clean", project)
        assert cleaned["removed"] > 0
        assert tree(project / ".preserve") == preserved
        assert tree(case / "published") == published
        assert tree(project / "dist") == default_output
        assert run("hmi", "clean", project)[0]["removed"] == 0
        after_clean, _ = run("hmi", "build", project, "-o", final)
        assert after_clean["build"] == cold["build"]
        assert after_clean["sha256"] == cold["sha256"]
        assert digest(source) == source_digest
        report["cases"].append({
            "input": str(source), "input_sha256": source_digest,
            "sha256": cold["sha256"], "cold": cold["build"], "warm": warm["build"],
            "rebuild": forced["build"], "after_clean": after_clean["build"],
            "removed": cleaned["removed"], "standalone_equals_driver": True,
            "preserved_inputs_unchanged": True, "published_outputs_unchanged_by_clean": True,
            "without_backend": True,
            "elapsed_seconds": {"cold": cold_time, "warm": warm_time, "rebuild": forced_time},
        })
        (output / "verification.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        print(f"{source.name}：目标 {units} 项、媒体 {assets} 项；增量复用、全量一致、清理保护通过。", flush=True)
    report["automatic_checks_passed"] = True
    (output / "verification.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()

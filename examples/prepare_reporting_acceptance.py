"""保存新工具链的日志、JSON和JSONL实测输出，供审阅；不访问虚拟SD。"""
import argparse
from datetime import datetime, timedelta, timezone
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tomllib


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True, help="已支持的现有HMI工程")
    parser.add_argument("--output", type=Path, default=root / "target/acceptance-reporting")
    parser.add_argument("--binaries", type=Path, default=root / "target/debug")
    args = parser.parse_args()
    source, output, binaries = args.input.resolve(), args.output.resolve(), args.binaries.resolve()
    if output.exists():
        parser.error("输出目录已存在，请使用 --output 指定新目录")
    if not source.is_file():
        parser.error("输入HMI文件不存在")
    suffix = ".exe" if os.name == "nt" else ""
    tools = {name: binaries / (name + suffix) for name in ["hmi", "hmi-compile", "hmi-media", "hmi-link"]}
    if any(not path.is_file() for path in tools.values()):
        parser.error("请先执行 cargo build --workspace --bins --locked")
    output.mkdir(parents=True)
    environment = dict(os.environ, HMI_FFMPEG_DIR=str(output / "unavailable-backend"))
    records = []

    def run(label, tool, *arguments, expected=0):
        command = [str(tools[tool]), *map(str, arguments)]
        result = subprocess.run(command, env=environment, capture_output=True, encoding="utf-8")
        for stream, text in [("stdout", result.stdout), ("stderr", result.stderr)]:
            (output / f"{label}.{stream}.txt").write_text(text, encoding="utf-8")
        if result.returncode != expected:
            raise RuntimeError(f"{label}退出码{result.returncode}，预期{expected}\n{result.stdout}\n{result.stderr}")
        records.append({"case": label, "command": command, "exit_code": result.returncode})
        if "--json" in arguments:
            value = json.loads(result.stdout)
            assert value["schema_version"] == 2
            assert result.stderr == ""
            (output / f"{label}.pretty.json").write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        elif "jsonl" in arguments:
            events = [json.loads(line) for line in result.stdout.splitlines()]
            assert events[-1]["event"] == "result"
            assert result.stderr == ""
            value = events[-1]["data"]
        else:
            value = None
        if value:
            for counts in (value.get("result") or {}).get("summary", {}).values():
                assert counts["total"] == sum(counts[k] for k in ["processed", "reused", "failed", "not_run"])
            for artifact in (value.get("result") or {}).get("outputs", []):
                if artifact["state"] == "written" and artifact["sha256"] is not None:
                    assert digest(Path(artifact["path"])) == artifact["sha256"]
        print(f"已保存：{label}", flush=True)
        return value

    project = output / "project"
    original_digest = digest(source)
    run("01_import", "hmi", "--json", "import", source, "-o", project)
    manifest = tomllib.loads((project / "hmi.toml").read_text(encoding="utf-8"))
    run("02_cold_build", "hmi", "--color", "never", "build", project)
    run("03_cached_build", "hmi", "--color", "never", "build", project)
    run("04_verbose_build", "hmi", "--color", "never", "-v", "build", project)
    built = run("05_json_build", "hmi", "--json", "build", project)
    hmi = Path(built["result"]["output"])
    expected_digest = digest(hmi)
    streamed = run("06_jsonl_build", "hmi", "--message-format", "jsonl", "build", project)
    assert streamed["result"]["sha256"] == expected_digest
    run("07_inspect", "hmi", "--color", "never", "inspect", hmi)
    run("08_quiet_build", "hmi", "-q", "build", project)
    assert (output / "08_quiet_build.stdout.txt").read_text(encoding="utf-8") == ""
    if not built["diagnostics"]:
        assert (output / "08_quiet_build.stderr.txt").read_text(encoding="utf-8") == ""
    target = project / ".build/hmi-toolchain/target.json"
    page_arguments = ["--color", "never", "page", project / manifest["pages"]["sources"][0], "--target", target, "-o", output / "tools/page.hmo"]
    if manifest.get("original"):
        page_arguments += ["--original", project / manifest["original"]["bundle"]]
    run("09_compiler", "hmi-compile", *page_arguments)
    if manifest.get("assets", {}).get("items"):
        run("10_media", "hmi-media", "--color", "never", "-v", "encode", project / manifest["assets"]["items"][0]["source"], "--target", target, "-o", output / "tools/native.hma")
    linked = run("11_link", "hmi-link", "--json", project / ".build/hmi-toolchain/link.toml", "-o", output / "tools/linked.HMI", "--map", output / "tools/map.json")
    assert linked["result"]["sha256"] == expected_digest
    run("12_help_json", "hmi", "--json", "--help")
    run("13_arguments_json", "hmi", "--json", "build", "--bad-option", expected=2)
    failed_project = output / "project-error"
    shutil.copytree(project, failed_project, ignore=shutil.ignore_patterns(".build"))
    (failed_project / manifest["program"]["source"]).write_text("page 999999\npage 999998\n", encoding="utf-8")
    # 报告中的工程根目录与产物使用相同的规范路径，兼容 Windows 长路径前缀。
    failed_hmi = failed_project / hmi.relative_to(Path(built["project"]["root"]))
    failed = run("14_error_json", "hmi", "--json", "build", failed_project, expected=1)
    assert len([d for d in failed["diagnostics"] if d["code"] == "script_page"]) == 2
    assert all(a["state"] == "not_written" for a in failed["result"]["outputs"])
    run("15_error_human", "hmi", "--color", "never", "build", failed_project, expected=1)
    assert digest(failed_hmi) == expected_digest
    assert digest(source) == original_digest
    report = {"generated_on": datetime.now(timezone(timedelta(hours=8))).date().isoformat(), "input": str(source), "input_sha256": original_digest,
              "rebuilt_sha256": expected_digest, "input_bytes_equal": original_digest == expected_digest,
              "standalone_equals_driver": True, "failed_build_keeps_hmi": True, "without_backend": True,
              "automatic_checks_passed": True, "cases": records}
    (output / "verification.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    (output / "review.txt").write_text(
        "输出格式审阅\n\n"
        "02_cold_build.stderr.txt：首次构建，实际处理页面和媒体。\n"
        "03_cached_build.stderr.txt：默认模式只汇总缓存复用。\n"
        "04_verbose_build.stderr.txt：详细模式逐项显示缓存和耗时。\n"
        "05_json_build.pretty.json：实际JSON结果的缩进副本，原始单行输出另存stdout.txt。\n"
        "06_jsonl_build.stdout.txt：实时事件流，以result事件结束。\n"
        "07_inspect.stdout.txt：人类可读的工程摘要和页面、资源列表。\n"
        "09_compiler.stderr.txt、10_media.stderr.txt：独立编译器、媒体工具日志。\n"
        "14_error_json.pretty.json、15_error_human.stderr.txt：同一工程的两条错误。\n\n"
        "project-error保留了故意设置的错误，方便复现。输入HMI未修改；不需要上位机验收。\n",
        encoding="utf-8")


if __name__ == "__main__":
    main()

"""用已有HMI核对串并行构建；可选验证实际视频处理的计数事件。"""
import argparse
from datetime import datetime, timedelta, timezone
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--output", type=Path, default=root / "target/acceptance-parallel")
    parser.add_argument("--binaries", type=Path, default=root / "target/debug")
    parser.add_argument("--video-descriptor", type=Path, help="可选source视频资产描述，需要可用的FFmpeg后端")
    args = parser.parse_args()
    source, output, binaries = args.input.resolve(), args.output.resolve(), args.binaries.resolve()
    if output.exists():
        parser.error("输出目录已存在，请用 --output 指定新目录")
    if not source.is_file():
        parser.error("输入HMI不存在")
    suffix = ".exe" if os.name == "nt" else ""
    tools = {name: binaries / (name + suffix) for name in ["hmi", "hmi-media"]}
    if any(not path.is_file() for path in tools.values()):
        parser.error("请先执行 cargo build --workspace --bins --locked")
    output.mkdir(parents=True)

    def run(label, tool, *arguments, stream=False):
        result = subprocess.run([str(tools[tool]), *map(str, arguments)], capture_output=True, encoding="utf-8")
        (output / f"{label}.stdout.txt").write_text(result.stdout, encoding="utf-8")
        (output / f"{label}.stderr.txt").write_text(result.stderr, encoding="utf-8")
        if result.returncode:
            raise RuntimeError(f"{label}失败\n{result.stdout}\n{result.stderr}")
        assert result.stderr == ""
        events = [json.loads(line) for line in result.stdout.splitlines()] if stream else []
        value = events[-1]["data"] if stream else json.loads(result.stdout)
        assert value["ok"] and value["schema_version"] == 2
        for count in value["result"].get("summary", {}).values():
            assert count["total"] == sum(count[k] for k in ["processed", "reused", "failed", "not_run"])
        (output / f"{label}.pretty.json").write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        print(f"已核对：{label}", flush=True)
        return value, events

    project = output / "project"
    source_hash = sha(source)
    run("01_import", "hmi", "--json", "import", source, "-o", project)
    serial, _ = run("02_serial", "hmi", "--json", "rebuild", project, "-j", "1", "-o", output / "serial/Panel.HMI")
    parallel, events = run("03_parallel", "hmi", "--message-format", "jsonl", "rebuild", project, "-j", "4", "-o", output / "parallel/Panel.HMI", stream=True)
    assert serial["result"]["sha256"] == parallel["result"]["sha256"]
    assert (output / "serial/Panel.HMI").read_bytes() == (output / "parallel/Panel.HMI").read_bytes()
    assert json.loads((output / "serial/link-map.json").read_text(encoding="utf-8")) == json.loads((output / "parallel/link-map.json").read_text(encoding="utf-8"))
    active, media, maximum, media_maximum = set(), set(), 0, 0
    for event in events:
        task = event["data"].get("task", {})
        if task.get("group") not in ["program", "pages", "media"]:
            continue
        if event["event"] == "task_started":
            active.add(task["id"])
            if task["group"] == "media":
                media.add(task["id"])
            maximum, media_maximum = max(maximum, len(active)), max(media_maximum, len(media))
        elif event["event"] == "task_finished":
            active.remove(task["id"])
            media.discard(task["id"])
    assert not active and not media and maximum <= 4 and media_maximum <= 2
    warm, _ = run("04_cached", "hmi", "--json", "build", project, "-j", "4", "-o", output / "parallel/Panel.HMI")
    assert warm["result"]["sha256"] == serial["result"]["sha256"]
    for count in warm["result"]["summary"].values():
        assert count["reused"] == count["total"]
    report = {"generated_on": datetime.now(timezone(timedelta(hours=8))).date().isoformat(),
              "input_sha256": source_hash, "output_sha256": serial["result"]["sha256"],
              "input_bytes_equal": source_hash == serial["result"]["sha256"],
              "serial_equals_parallel": True, "link_maps_equal": True,
              "maximum_active_jobs": maximum, "maximum_active_media_jobs": media_maximum,
              "cached_summary": warm["result"]["summary"]}
    if args.video_descriptor:
        video, video_events = run("05_video_progress", "hmi-media", "--message-format", "jsonl", "encode", args.video_descriptor.resolve(),
                                  "--target", project / ".build/hmi-toolchain/target.json", "-o", output / "video.hma", stream=True)
        encoded = [event["data"]["task"]["progress"] for event in video_events
                   if event["event"] == "task_finished" and event["data"]["task"].get("progress", {}).get("phase") == "encode"]
        frames = video["result"]["asset"]["video"]["frame_count"]
        assert encoded and encoded[-1]["current"] == frames and encoded[-1]["total"] == frames
        report["video_progress"] = encoded[-1]
    assert sha(source) == source_hash
    report["automatic_checks_passed"] = True
    (output / "verification.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()

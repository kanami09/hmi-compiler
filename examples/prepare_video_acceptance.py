"""生成 X543 无声视频验收工程；用标准 AVI 素材和 FFmpeg DLL，不调用 FFmpeg EXE。"""
import argparse
from datetime import date
import hashlib
import json
import math
import os
from pathlib import Path
import shutil
import struct
import subprocess
import sys


def chunk(tag, body):
    return tag + struct.pack("<I", len(body)) + body + (b"\0" if len(body) % 2 else b"")


def avi(audio=False):
    width, height, fps, count = 160, 80, 4, 4
    stride = (width * 3 + 3) & ~3
    size = stride * height
    avih = struct.pack("<14I", 250000, size * fps, 0, 16, count, 0, 2 if audio else 1, size, width, height, 0, 0, 0, 0)
    strh = b"vids" + bytes(4) + struct.pack("<10I", 0, 0, 0, 1, fps, 0, count, size, 0xffffffff, 0) + struct.pack("<4h", 0, 0, width, height)
    strf = struct.pack("<IiiHH6I", 40, width, height, 1, 24, 0, size, 0, 0, 0, 0)
    hdrl = b"hdrl" + chunk(b"avih", avih) + chunk(b"LIST", b"strl" + chunk(b"strh", strh) + chunk(b"strf", strf))
    if audio:
        audio_header = b"auds" + bytes(4) + struct.pack("<10I", 0, 0, 0, 1, 8000, 0, 8000, 16000, 0xffffffff, 2) + bytes(8)
        audio_format = struct.pack("<HHIIHH", 1, 1, 8000, 16000, 2, 16)
        hdrl += chunk(b"LIST", b"strl" + chunk(b"strh", audio_header) + chunk(b"strf", audio_format))
    movi, index = bytearray(), bytearray()
    for frame, color in enumerate([(220, 40, 20), (30, 210, 60), (30, 70, 230), (220, 190, 20)]):
        pixels = bytearray()
        for y in range(height - 1, -1, -1):
            for x in range(width):
                rgb = (255, 255, 255) if x < 8 and y < 8 else color
                pixels.extend(rgb[::-1])
            pixels.extend(bytes(stride - width * 3))
        index.extend(b"00db" + struct.pack("<III", 16, 4 + len(movi), len(pixels)))
        movi.extend(chunk(b"00db", pixels))
        if audio:
            pcm = b"".join(struct.pack("<h", round(1500 * math.sin(2 * math.pi * 440 * i / 8000))) for i in range(frame * 2000, (frame + 1) * 2000))
            index.extend(b"01wb" + struct.pack("<III", 0, 4 + len(movi), len(pcm)))
            movi.extend(chunk(b"01wb", pcm))
    return chunk(b"RIFF", b"AVI " + chunk(b"LIST", hdrl) + chunk(b"LIST", b"movi" + movi) + chunk(b"idx1", index))


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=root / "target/acceptance-video")
    parser.add_argument("--binaries", type=Path, default=root / "target/debug")
    parser.add_argument("--ffmpeg-dir", type=Path, help="FFmpeg 9 shared 库的 bin 目录；省略时沿用后端设置")
    args = parser.parse_args()
    output, binaries = args.output.resolve(), args.binaries.resolve()
    if output.exists():
        parser.error("输出目录已存在，请使用 --output 指定新目录")
    environment = dict(os.environ)
    if args.ffmpeg_dir:
        environment["HMI_FFMPEG_DIR"] = str(args.ffmpeg_dir.resolve())
    suffix = ".exe" if os.name == "nt" else ""
    tools = {"hmi", "hmi-compile", "hmi-media", "hmi-link"}
    if any(not (binaries / (tool + suffix)).is_file() for tool in tools):
        parser.error("请先运行 cargo build --workspace --bins")

    def run(tool, *arguments):
        if tool not in tools:
            raise RuntimeError(f"执行计划包含未知工具：{tool}")
        r = subprocess.run([str(binaries / (tool + suffix)), *map(str, arguments), "--json"],
                           env=environment, capture_output=True, text=True, encoding="utf-8", check=False)
        if r.returncode:
            raise RuntimeError(r.stdout + r.stderr)
        value = json.loads(r.stdout)
        if not value["ok"] or value["diagnostics"]:
            raise RuntimeError(r.stdout)
        return value["result"]

    backend = run("hmi-media", "backend")["backend"]
    source = avi()
    output.mkdir(parents=True)
    cases = []
    for orientation, width, height, loop in [(0, 480, 272, 0), (90, 272, 480, 1)]:
        project = output / f"project-{orientation}"
        run("hmi", "init", project, "--target", "TJC4827X543_011", "--orientation", orientation)
        assets = project / "assets"
        assets.mkdir()
        (assets / "source.avi").write_bytes(source)
        for key, start, duration in [("full", 0, 1000), ("clip", 250, 500)]:
            (assets / f"{key}.asset.toml").write_text(f'schema="hmi.asset/1"\nkind="video"\nsource="source.avi"\n[encoding]\nquality=80\nfps=4\n[clip]\nstart_ms={start}\nduration_ms={duration}\n[audio]\nmode="if-present"\n', encoding="utf-8")
        order = ["full", "clip"] if orientation == 0 else ["clip", "full"]
        items = ",".join('{key="' + key + '",source="assets/' + key + '.asset.toml"}' for key in order)
        (project / "hmi.toml").write_text(f'''schema="hmi.project/1"
name="Video{orientation}"
[target]
profile="TJC4827X543_011"
format="ver21234"
encoding="utf-8"
orientation={orientation}
[program]
source="program.s"
[pages]
sources=["pages/full/page.toml","pages/clip/page.toml"]
[assets]
items=[{items}]
''', encoding="utf-8")
        for key in ["full", "clip"]:
            directory = project / "pages" / key
            directory.mkdir()
            (directory / "page.toml").write_text(f'schema="hmi.page/1"\nname="{key}"\n[[objects]]\nname="v0"\ntype="video"\nproperties={{x=20,y=20,w=160,h=80,vid={{ref="{key}"}},en=0,loop={loop},dis=100}}\nevents={{playend="v0.playend.s"}}\n', encoding="utf-8")
            (directory / "v0.playend.s").write_text("sys0=v0.qty\n", encoding="utf-8")
        run("hmi", "check", project, "-Werror")
        assert not (project / ".build").exists()
        hmi = output / f"Video{orientation}.HMI"
        result = run("hmi", "build", project, "-o", hmi, "-Werror")
        assert result["external_files"] == []
        driver_map = output / f"Video{orientation}.link-map.json"
        shutil.copyfile(output / "link-map.json", driver_map)
        plan = project / ".build/manual"
        run("hmi", "plan", project, "-o", plan)
        jobs = json.loads((plan / "jobs.json").read_bytes())["jobs"]
        manual_map = project / "standalone-map.json"
        for job in jobs:
            extra = ["--map", manual_map, "-Werror"] if job["command"] == "hmi-link" else []
            run(job["command"], *job["args"], *extra)
        assert hmi.read_bytes() == (project / f"dist/Video{orientation}/{hmi.name}").read_bytes()
        assert json.loads(driver_map.read_bytes()) == json.loads(manual_map.read_bytes())
        run("hmi-link", plan / "link.toml", "--check", "-Werror")
        for key, frames, duration in [("full", 4, 1000), ("clip", 2, 500)]:
            info = run("hmi-media", "inspect", plan / f"assets/a{order.index(key):05}.hma")["metadata"]["video"]
            assert info["frame_count"] == frames and info["duration_ms"] == duration
            assert info["source_frames"] == 4 and info["source_audio_streams"] == 0 and not info["audio_applied"]
        inspected = run("hmi-compile", "inspect", plan / "obj/p00001.hmo")
        assert inspected["objects"][1]["properties"]["vid"] == {"ref": "clip"}
        cases.append({"file": hmi.name, "sha256": hashlib.sha256(hmi.read_bytes()).hexdigest(), "profile": "TJC4827X543_011",
                      "orientation": orientation, "page_size": [width, height], "loop": loop, "en": 0,
                      "video_ids": {key: order.index(key) for key in order}, "fps": 4, "source_sha256": hashlib.sha256(source).hexdigest(),
                      "full": {"frames": 4, "duration_ms": 1000}, "clip": {"start_ms": 250, "frames": 2, "duration_ms": 500},
                      "standalone_jobs": len(jobs), "standalone_equals_driver": True, "opened": None, "compiled": None, "content_verified": None})
    (output / "verification.json").write_text(json.dumps({"generated_on": date.today().isoformat(), "stage": "silent-video-ffmpeg",
        "automatic_checks_passed": True, "backend": backend, "cases": cases}, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    (output / "README.md").write_text('''# 无声视频验收

请直接打开 Video0.HMI、Video90.HMI，逐页检查后分别编译。资源均已内嵌，无需复制到虚拟 SD；打开和编译这些 HMI 不需要安装 FFmpeg。需要保存时请另存副本，原始摘要见 verification.json。

两份均为 TJC4827X543_011，Video0 为 0°、480×272，Video90 为 90°、272×480。页面 full 为 ID 0、clip 为 ID 1；各页一个视频 v0，控件 ID 1，20/20、160×80，私有、from=0、en=0、dis=100。两份 loop 分别为 0／1，初始状态均为停止。playend 为 sys0=v0.qty，其他事件为空。

| 工程 | full 视频 ID | clip 视频 ID | loop |
|---|---|---|---|
| Video0.HMI | 0 | 1 | 0 |
| Video90.HMI | 1 | 0 | 1 |

输入是代码生成的 160×80、4fps 无声 AVI，四帧依次为红、绿、蓝、黄底，左上角有白色方块用于核对方向。full 资源完整保留四帧、1000ms；clip 从 250ms 起截取 500ms，保留绿、蓝两帧。输出均为 JPEG 质量 80、4fps；资源没有音轨。

请检查方向／尺寸、控件 vid、loop/en/dis、playend 事件、视频列表中的帧数和时长，以及初始画面：full 页应为红底白角，clip 页应为绿底白角。可用资源面板的预览功能检查帧序列，不要求启动模拟器或连接硬件。

tim/stim/qty 保留原生停止状态的初值 0，不按资源时长／帧数回填；底层 fps 字段保留原生值 1，资源实际帧率以视频信息的 4fps 为准，不必手工修改隐藏字段。此批核对 HMI 结构、预览和编译，不把它计为实际播放时序或硬件解码验证。

本工具通过 FFmpeg DLL 解码，未调用 FFmpeg EXE，也未读取已有 HMI、内部媒体或字体。每份实际执行六条独立作业，HMI 和完整链接映射与统一入口一致。源码位于 project-0／project-90，重新构建源码才需要配置相应的 FFmpeg 9 shared 库。音轨保留和转换尚未接入，有音轨的输入必须显式选择 drop 才能生成无声输出。

请反馈两份的属性、初始画面／帧序列和编译结果；有异常时保留具体提示，不必重新导入媒体修复。
''', encoding="utf-8")
    print(f"无声视频验收工程已生成：{output}")


if __name__ == "__main__":
    main()

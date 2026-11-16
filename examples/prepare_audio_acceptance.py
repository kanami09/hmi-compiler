"""生成 PCM 音频、重采样音频与有声视频验收工程，不依赖已有 HMI 或内部素材。"""
import argparse
from datetime import date
import hashlib
import json
import math
import os
from pathlib import Path
import struct
import subprocess
import sys
from prepare_video_acceptance import avi


def wave(rate, channels):
    pcm = bytearray()
    for i in range(rate):
        for channel in range(channels):
            value = round(1500 * math.sin(2 * math.pi * (330 + channel * 330) * i / rate))
            pcm.extend(struct.pack("<h", value))
    return (b"RIFF" + struct.pack("<I", 36 + len(pcm)) + b"WAVEfmt "
            + struct.pack("<IHHIIHH", 16, 1, channels, rate, rate * channels * 2, channels * 2, 16)
            + b"data" + struct.pack("<I", len(pcm)) + pcm)


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=root / "target/acceptance-audio")
    parser.add_argument("--binaries", type=Path, default=root / "target/debug")
    parser.add_argument("--ffmpeg-dir", type=Path, help="FFmpeg 9 shared 库目录，省略沿用当前后端设置")
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
        result = subprocess.run([str(binaries / (tool + suffix)), *map(str, arguments), "--json"],
                                env=environment, capture_output=True, text=True, encoding="utf-8", check=False)
        if result.returncode:
            raise RuntimeError(result.stdout + result.stderr)
        value = json.loads(result.stdout)
        if not value["ok"] or value["diagnostics"]:
            raise RuntimeError(result.stdout)
        return value["result"]

    backend = run("hmi-media", "backend")["backend"]
    output.mkdir(parents=True)
    cases = []
    for orientation, width, height, loop in [(0, 480, 272, 0), (90, 272, 480, 1)]:
        project = output / f"project-{orientation}"
        run("hmi", "init", project, "--target", "TJC4827X543_011", "--orientation", orientation)
        assets = project / "assets"
        assets.mkdir()
        (assets / "mono.wav").write_bytes(wave(22050, 1))
        (assets / "stereo.wav").write_bytes(wave(48000, 2))
        (assets / "sound.avi").write_bytes(avi(audio=True))
        (assets / "tone.asset.toml").write_text('schema="hmi.asset/1"\nkind="audio"\nsource="mono.wav"\n', encoding="utf-8")
        (assets / "short.asset.toml").write_text('schema="hmi.asset/1"\nkind="audio"\nsource="stereo.wav"\n[encoding]\nsample_rate=8000\nchannels=1\nsample_format="s16"\n[clip]\nstart_ms=250\nduration_ms=500\n', encoding="utf-8")
        (assets / "sound.asset.toml").write_text('schema="hmi.asset/1"\nkind="video"\nsource="sound.avi"\n[encoding]\nquality=80\nfps=4\n[clip]\nduration_ms=1000\n[audio]\nmode="require"\nsample_rate=8000\nchannels=1\n', encoding="utf-8")
        order = ["tone", "sound", "short"] if orientation == 0 else ["short", "tone", "sound"]
        items = ",".join('{key="' + key + '",source="assets/' + key + '.asset.toml"}' for key in order)
        (project / "hmi.toml").write_text(f'''schema="hmi.project/1"
name="Audio{orientation}"
[target]
profile="TJC4827X543_011"
format="ver21234"
encoding="utf-8"
orientation={orientation}
[program]
source="program.s"
[pages]
sources=["pages/tone/page.toml","pages/short/page.toml","pages/sound/page.toml"]
[assets]
items=[{items}]
''', encoding="utf-8")
        for name in ["tone", "short", "sound"]:
            directory = project / "pages" / name
            directory.mkdir()
            video = name == "sound"
            control = "v0" if video else "wav0"
            geometry = "x=20,y=20,w=160,h=80," if video else ""
            (directory / "page.toml").write_text(f'schema="hmi.page/1"\nname="{name}"\n[[objects]]\nname="{control}"\ntype="{"video" if video else "audio"}"\nproperties={{{geometry}vid={{ref="{name}"}},en=0,loop={loop},dis=100}}\nevents={{playend="end.s"}}\n', encoding="utf-8")
            (directory / "end.s").write_text(f"sys0={control}.qty\n", encoding="utf-8")
        run("hmi", "check", project, "-Werror")
        assert not (project / ".build").exists()
        hmi = output / f"Audio{orientation}.HMI"
        run("hmi", "build", project, "-o", hmi, "-Werror")
        plan = project / ".build/manual"
        run("hmi", "plan", project, "-o", plan)
        jobs = json.loads((plan / "jobs.json").read_bytes())["jobs"]
        for job in jobs:
            run(job["command"], *job["args"])
        assert hmi.read_bytes() == (project / f"dist/Audio{orientation}/{hmi.name}").read_bytes()
        run("hmi-link", plan / "link.toml", "--check", "-Werror")
        for key, rate, count in [("tone", 22050, 22050), ("short", 8000, 4000)]:
            info = run("hmi-media", "inspect", plan / f"assets/a{order.index(key):05}.hma")["metadata"]["audio"]
            assert info["sample_rate"] == rate and info["sample_frames"] == count and info["channels"] == 1
            assert (info["backend"] is None) == (key == "tone")
        video = run("hmi-media", "inspect", plan / f"assets/a{order.index('sound'):05}.hma")["metadata"]["video"]
        assert video["audio_applied"] and video["audio"]["sample_frames"] == 8000
        assert video["audio"]["leading_silence_samples"] == video["audio"]["trailing_silence_samples"] == 0
        assert run("hmi-compile", "inspect", plan / "obj/p00000.hmo")["objects"][1]["properties"]["vid"] == {"ref": "tone"}
        audio_order = [key for key in order if key != "sound"]
        cases.append({"file": hmi.name, "sha256": hashlib.sha256(hmi.read_bytes()).hexdigest(), "profile": "TJC4827X543_011",
                      "orientation": orientation, "page_size": [width, height], "loop": loop, "en": 0,
                      "audio_ids": {key: audio_order.index(key) for key in audio_order}, "video_id": 0,
                      "standalone_jobs": len(jobs), "standalone_equals_driver": True, "opened": None, "compiled": None, "content_verified": None})
    (output / "verification.json").write_text(json.dumps({"generated_on": date.today().isoformat(), "stage": "audio-wave-video-track",
        "automatic_checks_passed": True, "backend": backend, "cases": cases}, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    (output / "README.md").write_text('''# 音频与视频音轨验收

请直接打开 Audio0.HMI、Audio90.HMI，逐页核对后分别编译。资源全部内嵌，验收 HMI 不需要配置本工具 FFmpeg，也无需复制到虚拟 SD。需要保存时请另存副本，原始摘要见 verification.json。

两份均为 TJC4827X543_011，Audio0 为 0°、480×272，Audio90 为 90°、272×480。页面 tone／short／sound 的 ID 分别为 0／1／2；每页一个普通控件，ID 1。两份 loop 分别为 0／1，en 都为 0，初始均停止；from=0、dis=100。playend 事件读取本控件 qty 到 sys0。

| 工程 | tone 音频 ID | short 音频 ID | sound 视频 ID |
|---|---|---|---|
| Audio0.HMI | 0 | 1 | 0 |
| Audio90.HMI | 1 | 0 | 0 |

tone 页：特殊控件栏中为 wav0，没有 x/y/w/h 或字体。资源来自 22050Hz 单声道 PCM16 WAV，保留参数，22050 个采样、1000ms，不调用 FFmpeg 转换。

short 页：同样为非可视 wav0。源文件为 48000Hz 双声道，内部转为 8000Hz 单声道 PCM16；从 250ms 开始截取 500ms，输出4000个采样。该资源与tone分别分配ID，不因资产表中间插入视频而改变类别计数。

sound 页：v0 位于 20/20、160×80，4fps、四帧、1000ms，红／绿／蓝／黄底、左上白角。源 AVI 含 8000Hz 单声道 PCM 音轨，本次 require 保留，输出同为8000Hz单声道、8000个采样；音轨在视频内部，不额外占音频列表ID。请检查资源信息中确有音轨，初始画面为红底白角。

两页音频的tim/stim/qty保留原生停止状态初值0，fps字段保留1，molloc_s/molloc保留64000，不必改成文件参数。音频只有playend事件，不应出现down/up。

请核对资源列表、ID、采样率／声道／时长、非可视控件属性、视频音轨标记和初始画面，并分别编译。若使用资源面板试听可按需要操作，本批不要求启动模拟器、连接硬件、验证实际播放／听感或音画同步；en=0不会为验收主动播放。

源码在 project-0／project-90，原始 WAV／AVI 均由代码生成。每份实际执行八条独立作业，HMI与统一入口一致；重建源码中的转换路径需要 FFmpeg 9 shared DLL，不调用 FFmpeg EXE，也不依赖已有 HMI、内部媒体或字体。
''', encoding="utf-8")
    print(f"音频验收工程已生成：{output}")


if __name__ == "__main__":
    main()

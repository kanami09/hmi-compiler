"""生成 encoded／native／original 媒体输入批量验收，共两份HMI、30页。"""
import argparse
from datetime import date
import hashlib
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import sys
from prepare_pictures_acceptance import png_logo
from prepare_animation_acceptance import gif
from prepare_video_acceptance import avi
from prepare_audio_acceptance import wave


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--font", type=Path, required=True, help="独立UTF-8 ZI字库，建议沿用已验收字高68字库")
    parser.add_argument("--ffmpeg-dir", type=Path, help="准备原始视频资产包时使用的FFmpeg 9 DLL目录")
    parser.add_argument("--output", type=Path, default=root / "target/acceptance-media-inputs")
    parser.add_argument("--binaries", type=Path, default=root / "target/debug")
    args = parser.parse_args()
    output, binaries, font = args.output.resolve(), args.binaries.resolve(), args.font.resolve()
    if output.exists():
        parser.error("输出目录已存在，请使用 --output 指定新目录")
    if not font.is_file():
        parser.error("请提供独立ZI字库")
    environment = dict(os.environ)
    if args.ffmpeg_dir:
        environment["HMI_FFMPEG_DIR"] = str(args.ffmpeg_dir.resolve())
    no_backend = dict(environment, HMI_FFMPEG_DIR=str(output / "unavailable-backend"))
    suffix = ".exe" if os.name == "nt" else ""
    tools = {"hmi", "hmi-compile", "hmi-media", "hmi-link"}
    if any(not (binaries / (tool + suffix)).is_file() for tool in tools):
        parser.error("请先运行 cargo build --workspace --bins")

    def run(tool, *arguments, without_backend=False):
        if tool not in tools:
            raise RuntimeError(f"执行计划包含未知工具：{tool}")
        result = subprocess.run([str(binaries / (tool + suffix)), *map(str, arguments), "--json"],
                                env=no_backend if without_backend else environment,
                                capture_output=True, text=True, encoding="utf-8", check=False)
        if result.returncode:
            raise RuntimeError(result.stdout + result.stderr)
        value = json.loads(result.stdout)
        if not value["ok"] or value["diagnostics"]:
            raise RuntimeError(result.stdout)
        return value["result"]

    def member(package, index):
        metadata = run("hmi-media", "inspect", package)["metadata"]
        data = package.read_bytes()
        offset = 80 + struct.unpack_from("<I", data, 8)[0]
        entry = metadata["members"][index]
        payload = data[offset + entry["offset"]:offset + entry["offset"] + entry["length"]]
        assert hashlib.sha256(payload).hexdigest() == entry["sha256"]
        return payload

    backend = run("hmi-media", "backend")["backend"]
    output.mkdir(parents=True)
    cases = []
    kinds = [("image", "image", "picture", "p0"), ("font", "font", "text", "t0"),
             ("anim", "animation", "animation", "gm0"), ("video", "video", "video", "v0"),
             ("audio", "audio", "audio", "wav0")]
    for orientation in [0, 90]:
        name = f"Inputs{orientation}"
        print(f"正在准备 {name} 的原始资产包", flush=True)
        project = output / name
        run("hmi", "init", project, "--target", "TJC4827X543_011", "--orientation", orientation)
        run("hmi", "plan", project, "-o", project / ".build/seed")
        target = project / ".build/seed/target.json"
        assets = project / "assets"
        assets.mkdir()
        (assets / "source.png").write_bytes(png_logo())
        shutil.copyfile(font, assets / "source.zi")
        (assets / "source.gif").write_bytes(gif(160, 80, [bytes([1] * (160 * 80)), bytes([2] * (160 * 80))], False))
        (assets / "source.avi").write_bytes(avi(audio=True))
        (assets / "source.wav").write_bytes(wave(8000, 1))
        sources = {"image": "source.png", "font": "source.zi", "anim": "source.gif", "video": "source.avi", "audio": "source.wav"}
        settings = {"video": '[encoding]\nfps=4\nquality=80\n[audio]\nmode="require"\nsample_rate=8000\nchannels=1\n'}
        for short, kind, _, _ in kinds:
            seed = assets / f"seed_{short}.toml"
            seed.write_text(f'schema="hmi.asset/1"\nkind="{kind}"\nsource="{sources[short]}"\n' + settings.get(short, ""), encoding="utf-8")
            run("hmi-media", "encode", seed, "--target", target, "-o", assets / f"{short}.hma")
        # 先由正式工具检查包，再按已验证的成员边界提取编码文件，不读任何HMI。
        (assets / "font.zi").write_bytes(member(assets / "font.hma", 0))
        for short, extension in [("anim", "gmov"), ("video", "video"), ("audio", "wav")]:
            (assets / f"{short}.{extension}").write_bytes(member(assets / f"{short}.hma", 0))
        (assets / "xi.asset.toml").write_text('schema="hmi.asset/1"\nkind="image"\nsource="source.png"\n[delivery]\nmode="external"\npath="sd0/seed.xi"\n', encoding="utf-8")
        run("hmi-media", "encode", assets / "xi.asset.toml", "--target", target, "-o", assets / "xi.hma")
        (assets / "image.xi").write_bytes(member(assets / "xi.hma", 0))
        encoded = {"image": "image.xi", "font": "font.zi", "anim": "anim.gmov", "video": "video.video", "audio": "audio.wav"}
        for mode, prefix in [("encoded", "e"), ("native", "n"), ("source", "o")]:
            for short, kind, _, _ in kinds:
                src = encoded[short] if mode == "encoded" else (f"{short}.hma" if mode == "native" else sources[short])
                text = f'schema="hmi.asset/1"\nkind="{kind}"\ninput="{mode}"\nsource="{src}"\n'
                if prefix == "o":
                    text += f'original="{short}.hma"\n' + settings.get(short, "")
                if prefix == "e" and short == "anim":
                    text += f'[delivery]\nmode="external"\npath="sd0/input{orientation}/anim.gmov"\n'
                (assets / f"{prefix}_{short}.asset.toml").write_text(text, encoding="utf-8")
        pages = [(prefix, short, kind, control_type, control) for prefix in ["e", "n", "o"] for short, kind, control_type, control in kinds]
        items = ",".join(f'{{key="{p}_{s}",source="assets/{p}_{s}.asset.toml"}}' for p, s, *_ in pages)
        page_paths = ",".join(f'"pages/{p}_{s}/page.toml"' for p, s, *_ in pages)
        (project / "hmi.toml").write_text(f'''schema="hmi.project/1"
name="{name}"
[target]
profile="TJC4827X543_011"
format="ver21234"
encoding="utf-8"
orientation={orientation}
[program]
source="program.s"
[pages]
sources=[{page_paths}]
[assets]
items=[{items}]
''', encoding="utf-8")
        for prefix, short, _, control_type, control in pages:
            directory = project / f"pages/{prefix}_{short}"
            directory.mkdir()
            if short == "image":
                props = f'x=20,y=20,w=48,h=32,pic={{ref="{prefix}_image"}}'
            elif short == "font":
                props = f'x=20,y=20,w=232,h=100,font={{ref="{prefix}_font"}},txt="ABC"'
            else:
                props = "en=0,loop=0"
                if short != "audio":
                    props += ",x=20,y=20,w=160,h=80"
                props += f',from=1,path={{ref="e_anim"}}' if prefix == "e" and short == "anim" else f',vid={{ref="{prefix}_{short}"}}'
            (directory / "page.toml").write_text(f'schema="hmi.page/1"\nname="{prefix}_{short}"\n[[objects]]\nname="{control}"\ntype="{control_type}"\nproperties={{{props}}}\n', encoding="utf-8")
        print(f"正在无FFmpeg环境中检查和构建 {name}", flush=True)
        run("hmi", "check", project, "-Werror", without_backend=True)
        assert not (project / ".build/hmi-toolchain").exists()
        hmi = output / f"{name}.HMI"
        run("hmi", "build", project, "-o", hmi, "-Werror", without_backend=True)
        plan = project / ".build/manual"
        run("hmi", "plan", project, "-o", plan, without_backend=True)
        jobs = json.loads((plan / "jobs.json").read_bytes())["jobs"]
        assert len(jobs) == 32
        actions = []
        for job in jobs:
            result = run(job["command"], *job["args"], without_backend=True)
            if job["command"] == "hmi-media":
                actions.append(result["input_action"])
        assert actions == ["encoded"] * 5 + ["native"] * 5 + ["original"] * 5
        default = project / f"dist/{name}"
        path = f"sd0/input{orientation}/anim.gmov"
        assert hmi.read_bytes() == (default / hmi.name).read_bytes()
        assert (output / path).read_bytes() == (default / path).read_bytes()
        cases.append({"file": hmi.name, "sha256": hashlib.sha256(hmi.read_bytes()).hexdigest(), "orientation": orientation,
                      "pages": [f"{p}_{s}" for p, s, *_ in pages], "standalone_jobs": len(jobs), "input_actions": actions,
                      "external_file": path, "external_sha256": hashlib.sha256((output / path).read_bytes()).hexdigest(),
                      "without_backend": True, "standalone_equals_driver": True, "opened": None, "compiled": None, "content_verified": None})
    (output / "verification.json").write_text(json.dumps({"generated_on": date.today().isoformat(), "stage": "media-input-modes",
        "backend_for_seed_only": backend, "combinations": 30, "automatic_checks_passed": True, "cases": cases}, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    (output / "README.md").write_text('''# 三种媒体输入批量验收

Inputs0.HMI为X543、0°、480×272；Inputs90.HMI为90°、272×480。每份15页，合计30组合。请先把本目录sd0中的input0、input90两个目录复制到上位机虚拟SD根目录，再分别打开两份HMI，核对各页并各编译一次；需要保存时请另存副本。原始摘要见verification.json。

每份按e_*、n_*、o_*分三组，每组依次为image、font、anim、video、audio五页，每页一个普通控件ID1：e为直接封装编码文件，n为保留原生资产包，o为原始输入加original附件并复用原始编码。原图／字库／动画／音视频源和附件已独立保存在同名工程assets中，不依赖原HMI。

| 页后缀 | 核对内容 |
|---|---|
| image | p0在20/20、48×32，透明背景红／蓝图案；三组画面应一致。e_image没有伪造.is编辑源，图片列表仍应可显示和编译 |
| font | t0在20/20、232×100，文字ABC，建议字高68；三组字体方向及画面一致 |
| anim | gm0在20/20、160×80，红／绿两帧、各500ms，初始停止 |
| video | v0同样位置尺寸，红／绿／蓝／黄四帧、4fps、1000ms，含8000Hz单声道音轨，初始停止 |
| audio | wav0非可视，8000Hz单声道、1000ms，初始停止 |

图片、字体、视频、音频的三组内部ID均为e0/n1/o2。e_anim走外部运行GMOV，from1、path为sd0/input<方向>/anim.gmov、非活动vid65535，不占内部动画ID；n_anim/o_anim为内嵌动画ID0/1，保留原GIF编辑源。三组动画均en0/loop0，不要求启动播放；本批不要求试听、滚动或硬件测试。

e_image来自XI，保留JPEG和透明度字节，原质量无法从XI可靠恢复，因此编码描述quality为null，不虚构质量数值。编码ZI已匹配目标方向，不在encoded模式隐式旋转。n组保留包内所有成员，o组只在源摘要、请求和目标条件一致时复用；这些处理结果通过命令行input_action验证，界面核对的是最终有效内容。

生成阶段先用正式编码器准备五类原始资产包，视频准备需要FFmpeg；随后统一构建和每份32条独立作业均在无可用FFmpeg的环境中完成，五个o_*明确返回original复用。HMI及外部GMOV与统一入口逐字节一致。复制两个配套目录后，打开HMI无需配置本工具FFmpeg；源码变更导致视频需要重新编码时才需要恢复后端。
''', encoding="utf-8")
    print(output)


if __name__ == "__main__":
    main()

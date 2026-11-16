"""生成三类媒体内部／外部播放批量验收工程，共两份 HMI、18页。"""
import argparse
from datetime import date
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
from prepare_animation_acceptance import gif
from prepare_video_acceptance import avi
from prepare_audio_acceptance import wave


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=root / "target/acceptance-external-playback")
    parser.add_argument("--binaries", type=Path, default=root / "target/debug")
    parser.add_argument("--ffmpeg-dir", type=Path, help="FFmpeg 9 shared DLL 目录，省略沿用后端设置")
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
    kinds = [("anim", "animation", "gm0", "clip.gif", "gmov"),
             ("video", "video", "v0", "sound.avi", "video"),
             ("audio", "audio", "wav0", "tone.wav", "wav")]
    for orientation in [0, 90]:
        name = f"MediaOut{orientation}"
        project = output / name
        run("hmi", "init", project, "--target", "TJC4827X543_011", "--orientation", orientation)
        assets = project / "assets"
        assets.mkdir()
        (assets / "clip.gif").write_bytes(gif(160, 80, [bytes([1] * (160 * 80)), bytes([2] * (160 * 80))], False))
        (assets / "sound.avi").write_bytes(avi(audio=True))
        (assets / "tone.wav").write_bytes(wave(8000, 1))
        keys, paths = [], {}
        for short, kind, _, source, extension in kinds:
            paths[kind] = f"sd0/ext{orientation}/{short}.{extension}"
            for delivery in ["embedded", "external"]:
                key = f'{short}_{"in" if delivery == "embedded" else "out"}'
                keys.append(key)
                text = f'schema="hmi.asset/1"\nkind="{kind}"\nsource="{source}"\n'
                if kind == "video":
                    text += '[encoding]\nfps=4\nquality=80\n[audio]\nmode="require"\nsample_rate=8000\nchannels=1\n'
                if delivery == "external":
                    text += f'[delivery]\nmode="external"\npath="{paths[kind]}"\n'
                (assets / f"{key}.asset.toml").write_text(text, encoding="utf-8")
        if orientation == 90:
            keys.reverse()
        pages = [(f"{mode}_{short}", mode, short, kind, control)
                 for mode in ["inside", "outside", "path"] for short, kind, control, _, _ in kinds]
        sources = ",".join(f'"pages/{page}/page.toml"' for page, *_ in pages)
        items = ",".join('{key="' + key + '",source="assets/' + key + '.asset.toml"}' for key in keys)
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
sources=[{sources}]
[assets]
items=[{items}]
''', encoding="utf-8")
        for page, mode, short, kind, control in pages:
            directory = project / "pages" / page
            directory.mkdir()
            props = [f'from={int(mode != "inside")}', "en=0", f"loop={int(orientation == 90)}"]
            if kind != "audio":
                props += ["x=20", "y=20", "w=160", "h=80"]
            if mode == "path":
                props += [f'path="{paths[kind]}"']
            else:
                props += [f'vid={{ref="{short}_in"}}', f'path={{ref="{short}_out"}}']
            (directory / "page.toml").write_text(f'''schema="hmi.page/1"
name="{page}"
[[objects]]
name="{control}"
type="{kind}"
properties={{{",".join(props)}}}
events={{playend="end.s"}}
''', encoding="utf-8")
            (directory / "end.s").write_text(f"sys0={control}.qty\n", encoding="utf-8")
        run("hmi", "check", project, "-Werror")
        assert not (project / ".build").exists()
        hmi = output / f"{name}.HMI"
        run("hmi", "build", project, "-o", hmi, "-Werror")
        plan = project / ".build/manual"
        run("hmi", "plan", project, "-o", plan)
        jobs = json.loads((plan / "jobs.json").read_bytes())["jobs"]
        assert len(jobs) == 17
        for job in jobs:
            run(job["command"], *job["args"])
        default = project / f"dist/{name}"
        assert hmi.read_bytes() == (default / hmi.name).read_bytes()
        run("hmi-link", plan / "link.toml", "--check", "-Werror")
        files = []
        for path in paths.values():
            data = (output / path).read_bytes()
            assert data == (default / path).read_bytes()
            files.append({"path": path, "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()})
        for index, (_, mode, short, _, _) in enumerate(pages):
            props = run("hmi-compile", "inspect", plan / f"obj/p{index:05}.hmo")["objects"][1]["properties"]
            assert props["from"] == int(mode != "inside")
            assert props["vid"] == ({"none": True} if mode == "path" else {"ref": short + "_in"})
        cases.append({"file": hmi.name, "sha256": hashlib.sha256(hmi.read_bytes()).hexdigest(),
                      "profile": "TJC4827X543_011", "orientation": orientation,
                      "pages": [page for page, *_ in pages], "external_files": files,
                      "standalone_jobs": len(jobs), "standalone_equals_driver": True,
                      "opened": None, "compiled": None, "content_verified": None})
    (output / "verification.json").write_text(json.dumps({"generated_on": date.today().isoformat(),
        "stage": "three-media-external-playback", "backend": backend, "combinations": 18,
        "automatic_checks_passed": True, "cases": cases}, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    (output / "README.md").write_text('''# 三类媒体内部／外部播放批量验收

共两份HMI、18页。请先将本目录sd0文件夹内的ext0和ext90两个子目录复制到上位机的虚拟SD根目录，再打开MediaOut0.HMI、MediaOut90.HMI。不要再套一层sd0；例如虚拟SD根目录下应有ext0/anim.gmov。需要保存HMI时请另存副本，原始摘要见verification.json。

| 文件 | 配置 | 页数 |
|---|---|---|
| MediaOut0.HMI | X543／0°／480×272，loop0 | 9 |
| MediaOut90.HMI | X543／90°／272×480，loop1 | 9 |

每份分三组，每组依次为anim、video、audio三页，每页一个普通控件ID1：

- inside_*：from0，vid0，内部播放；path仍保留对应外部文件路径。
- outside_*：from1，path使用外部资产绑定；非活动vid仍保留0。
- path_*：from1，直接声明同一运行时路径，没有vid引用，非活动vid为65535。

请核对属性、资源绑定及可见画面后，每份编译一次。所有控件en0，初始停止；loop1也不会被自动改成播放。动画gm0和视频v0均位于20/20、160×80；音频wav0非可视，无几何或down/up事件。playend均读取本控件qty，其他事件为空，tim/stim/qty保持原生初始0。

内部动画、视频、音频各占本类ID0；外部资产不占内部ID。90°工程有意反转资产顺序，绑定不能因此改变。内部资源列表仍有三类媒体，path_*没有vid引用也不能错误绑定到某个内部资源。

动画两帧红／绿色、各500ms；视频四帧红／绿／蓝／黄色、4fps、1000ms，左上角有白色定位块，含8000Hz单声道PCM音轨；独立音频8000Hz单声道、1000ms。两组外部文件分别为ext0及ext90下的anim.gmov、video.video、audio.wav，共六个文件。动画外部文件使用运行GMOV，不是GIF或编辑源GMOV；VIDEO和WAV同样已完成本工具格式化，不能只给原始AVI／普通WAV改后缀代替。

外部动画／视频的预览依赖虚拟SD文件可访问；编译过程中若遇到缺文件提示，请先检查上面的目录层次和配置。外部音频只核对路径、控件配置和编译，不要求本批试听；也不要求动态播放、时序或硬件测试。复制配套文件完成后无需配置本工具FFmpeg，只有重新编码源码才需要DLL。

每份实际执行17条独立作业，HMI与三个配套文件均和统一构建入口逐字节一致。源码和独立计划在同名子目录。脚本使用代码生成GIF、PCM WAV和有声音轨AVI，不读取已有HMI、字体或内部素材，也不自动修改用户的虚拟SD。
''', encoding="utf-8")
    print(output)


if __name__ == "__main__":
    main()

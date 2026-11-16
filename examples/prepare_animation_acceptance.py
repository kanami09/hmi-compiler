"""生成 X543 不透明／透明 GIF 动画验收工程，不需要现有 HMI 或外部素材。"""
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
from prepare_pictures_acceptance import bitmap


def gif(width, height, frames, transparent):
    """写标准 GIF89a，使用简单的 LZW 清表编码；每帧固定 500ms。"""
    out = bytearray(b"GIF89a" + struct.pack("<HH", width, height))
    out.extend(bytes([0x81, 0, 0, 0, 0, 0, 220, 40, 20, 30, 210, 60, 30, 70, 230]))
    for pixels in frames:
        assert len(pixels) == width * height
        out.extend(b"\x21\xf9\x04" + bytes([9 if transparent else 4]) + struct.pack("<H", 50) + b"\0\0\x2c")
        out.extend(struct.pack("<HHHHBB", 0, 0, width, height, 0, 2))
        data = bytearray()
        bits, count = 0, 0
        # 每个像素前清表，使码宽始终为三位，不依赖第三方 GIF 编码器。
        for pixel in pixels:
            for code in (4, pixel):
                bits |= code << count
                count += 3
                if count >= 8:
                    data.append(bits & 255)
                    bits >>= 8
                    count -= 8
        bits |= 5 << count
        count += 3
        while count > 0:
            data.append(bits & 255)
            bits >>= 8
            count -= 8
        for start in range(0, len(data), 255):
            chunk = data[start:start + 255]
            out.append(len(chunk))
            out.extend(chunk)
        out.append(0)
    out.append(0x3b)
    return bytes(out)


def source_gifs():
    opaque = []
    for frame, color in enumerate([1, 2]):
        opaque.append(bytes(3 if 20 + frame * 80 <= x < 40 + frame * 80 else color
                            for y in range(80) for x in range(160)))
    rings = []
    for frame, color in enumerate([2, 3, 1]):
        rings.append(bytes(color if 9 < math.hypot(x - 34 - frame, y - 26) < 24 else 0
                           for y in range(53) for x in range(69)))
    return gif(160, 80, opaque, False), gif(69, 53, rings, True)


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=root / "target/acceptance-animation")
    parser.add_argument("--binaries", type=Path, default=root / "target/debug")
    args = parser.parse_args()
    output, binaries = args.output.resolve(), args.binaries.resolve()
    if output.exists():
        parser.error("输出目录已存在，请使用 --output 指定新目录")
    suffix = ".exe" if os.name == "nt" else ""
    tools = {"hmi", "hmi-compile", "hmi-media", "hmi-link"}
    if any(not (binaries / (tool + suffix)).is_file() for tool in tools):
        parser.error("请先运行 cargo build --workspace --bins")

    def run(tool, *arguments):
        if tool not in tools:
            raise RuntimeError(f"执行计划包含未知工具：{tool}")
        r = subprocess.run([str(binaries / (tool + suffix)), *map(str, arguments), "--json"],
                           capture_output=True, text=True, encoding="utf-8", check=False)
        if r.returncode:
            raise RuntimeError(r.stdout + r.stderr)
        value = json.loads(r.stdout)
        if not value["ok"] or value["diagnostics"]:
            raise RuntimeError(r.stdout)
        return value["result"]

    opaque, rings = source_gifs()
    output.mkdir(parents=True)
    cases = []
    for orientation, width, height, loop in [(0, 480, 272, 0), (90, 272, 480, 1)]:
        project = output / f"project-{orientation}"
        run("hmi", "init", project, "--target", "TJC4827X543_011", "--orientation", orientation)
        assets = project / "assets"
        assets.mkdir()
        (assets / "opaque.gif").write_bytes(opaque)
        (assets / "ring.gif").write_bytes(rings)
        (assets / "background.bmp").write_bytes(bitmap(width, height))
        for key in ["opaque", "ring"]:
            (assets / f"{key}.asset.toml").write_text(f'schema="hmi.asset/1"\nkind="animation"\nsource="{key}.gif"\n[encoding]\nquality=86\nalpha="preserve"\ntiming="native-compatible"\n', encoding="utf-8")
        (assets / "background.asset.toml").write_text('schema="hmi.asset/1"\nkind="image"\nsource="background.bmp"\n', encoding="utf-8")
        order = ["opaque", "background", "ring"] if orientation == 0 else ["ring", "opaque", "background"]
        items = ",".join('{key="' + key + '",source="assets/' + key + '.asset.toml"}' for key in order)
        (project / "hmi.toml").write_text(f'''schema="hmi.project/1"
name="Animation{orientation}"
[target]
profile="TJC4827X543_011"
format="ver21234"
encoding="utf-8"
orientation={orientation}
[program]
source="program.s"
[pages]
sources=["pages/opaque/page.toml","pages/transparent/page.toml"]
[assets]
items=[{items}]
''', encoding="utf-8")
        for name, clip, w, h, x, y in [("opaque", "opaque", 160, 80, 20, 20), ("transparent", "ring", 69, 53, 30, 40)]:
            directory = project / "pages" / name
            directory.mkdir()
            text = f'schema="hmi.page/1"\nname="{name}"\n'
            if name == "transparent":
                text += f'[[objects]]\nname="bg0"\ntype="picture"\nproperties={{x=0,y=0,w={width},h={height},pic={{ref="background"}}}}\n'
            text += f'[[objects]]\nname="gm0"\ntype="animation"\nproperties={{x={x},y={y},w={w},h={h},vid={{ref="{clip}"}},en=0,loop={loop},dis=100}}\nevents={{playend="gm0.playend.s"}}\n'
            (directory / "page.toml").write_text(text, encoding="utf-8")
            (directory / "gm0.playend.s").write_text("sys0=gm0.qty\n", encoding="utf-8")
        run("hmi", "check", project, "-Werror")
        assert not (project / ".build").exists()
        hmi = output / f"Animation{orientation}.HMI"
        result = run("hmi", "build", project, "-o", hmi, "-Werror")
        assert result["external_files"] == []
        plan = project / ".build/manual"
        run("hmi", "plan", project, "-o", plan)
        jobs = json.loads((plan / "jobs.json").read_bytes())["jobs"]
        for job in jobs:
            run(job["command"], *job["args"])
        assert hmi.read_bytes() == (project / f"dist/Animation{orientation}/{hmi.name}").read_bytes()
        run("hmi-link", plan / "link.toml", "--check", "-Werror")
        for key, frames, duration in [("opaque", 2, 1000), ("ring", 3, 1500)]:
            index = order.index(key)
            info = run("hmi-media", "inspect", plan / f"assets/a{index:05}.hma")["metadata"]["animation"]
            assert info["frame_count"] == frames and info["duration_ms"] == duration
            assert info["has_alpha"] == (key == "ring") and info["timing_adjusted"] is False
        controls = run("hmi-compile", "inspect", plan / "obj/p00001.hmo")["objects"]
        assert controls[2]["properties"]["vid"] == {"ref": "ring"}
        animation_order = [key for key in order if key != "background"]
        cases.append({"file": hmi.name, "sha256": hashlib.sha256(hmi.read_bytes()).hexdigest(),
                      "profile": "TJC4827X543_011", "orientation": orientation, "page_size": [width, height],
                      "loop": loop, "en": 0, "animation_ids": {key: animation_order.index(key) for key in animation_order},
                      "frames": {"opaque": 2, "ring": 3}, "duration_ms": {"opaque": 1000, "ring": 1500},
                      "standalone_jobs": len(jobs), "standalone_equals_driver": True,
                      "opened": None, "compiled": None, "content_verified": None})
    (output / "verification.json").write_text(json.dumps({"generated_on": date.today().isoformat(),
        "stage": "gif-animation", "automatic_checks_passed": True, "cases": cases}, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    (output / "README.md").write_text('''# GIF 动画验收

请直接打开 Animation0.HMI、Animation90.HMI，逐页检查后分别编译。这批资源均已内嵌，无需复制到虚拟 SD，也无需使用官方动画转换工具。需要保存时请另存副本，原始摘要见 verification.json。

两份均为 TJC4827X543_011，Animation0 为 0°、480×272，Animation90 为 90°、272×480。页面顺序为 opaque（ID 0）、transparent（ID 1）。普通图片列表只有背景，图片 ID 0；动画资源列表有 opaque 和 ring 两项，与图片分别编号。

| 工程 | opaque 动画 ID | ring 动画 ID | 两页 gm0.loop |
|---|---|---|---|
| Animation0.HMI | 0 | 1 | 0 |
| Animation90.HMI | 1 | 0 | 1 |

两份 gm0 均 from=0、en=0（停止）、dis=100、私有。loop=1 不自动改为播放。tim/stim/qty 保留原生停止状态的初始零值，不要求属性栏预先显示资产总时间或帧数。playend 事件为 sys0=gm0.qty，其余事件为空。

opaque 页：白底，gm0 为控件 ID 1，位于 20/20、160×80；动画两帧、每帧 500ms，总时长 1000ms。第一帧红底、第二帧绿底，蓝色竖条的位置不同。请核对资源帧和初始显示内容，或使用资源面板自带的预览功能。

transparent 页：bg0 为控件 ID 1，铺满彩色网格背景；gm0 为控件 ID 2，位于 30/40、69×53。动画三帧、每帧 500ms，总时长 1500ms；圆环依次为绿、蓝、红，位置略有移动。圆环的中央孔和四角透明，能看到下方背景；GIF 本身只有透明／不透明两种状态，不要求半透明渐变。

请检查方向、页面和对象几何、vid 引用、loop/en/dis、playend 事件、动画列表中的两项及帧数／总时长、初始图片和透明孔是否正常，然后分别编译，反馈具体提示。本批不要求启动模拟器、连接硬件或验证播放时序／playend 的实际运行；如果预览或编译异常，保留提示，不通过重新导入资源修复。

源 GIF 是代码直接生成的标准 GIF89a，原文件位于 project-0／project-90 的 assets 中。构建从 GIF 生成独立的运行 GMOV 和保留原 GIF 的编辑源成员。每份实际执行七条独立作业，HMI 与统一入口一致；不读取已有 HMI、内部素材或字体。
''', encoding="utf-8")
    print(f"动画验收工程已生成：{output}")


if __name__ == "__main__":
    main()

"""生成 RGB565 图片／切图验收工程，只使用 Python 标准库和已构建的工具。"""

import argparse
from datetime import date
import hashlib
import json
import os
from pathlib import Path
import struct
import subprocess
import sys
import zlib


def bitmap(width, height):
    stride = (width * 3 + 3) & ~3
    pixels = bytearray(stride * height)
    for y in range(height):
        for x in range(width):
            r, g, b = [(255, 0, 0), (0, 255, 0), (0, 0, 255), (255, 255, 0)][
                int(x >= width // 2) + 2 * int(y >= height // 2)
            ]
            if x % 32 == 0 or y % 32 == 0:
                r = g = b = 0
            offset = (height - 1 - y) * stride + x * 3
            pixels[offset:offset + 3] = bytes((b, g, r))
    return (b"BM" + struct.pack("<IHHI", 54 + len(pixels), 0, 0, 54)
            + struct.pack("<IiiHHIIIIII", 40, width, height, 1, 24, 0, len(pixels), 0, 0, 0, 0)
            + pixels)


def png_logo():
    width, height = 48, 32
    raw = bytearray()
    for y in range(height):
        raw.append(0)
        for x in range(width):
            if x < 4 or y < 4 or x >= width - 4 or y >= height - 4:
                rgba = (0, 0, 0, 0)
            elif x < width // 2:
                rgba = (255, 0, 0, 128)
            else:
                rgba = (0, 0, 255, 255)
            raw.extend(rgba)

    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b""))


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=root / "target/acceptance-pictures")
    parser.add_argument("--binaries", type=Path, default=root / "target/debug")
    args = parser.parse_args()
    output, binaries = args.output.resolve(), args.binaries.resolve()
    suffix = ".exe" if os.name == "nt" else ""
    if output.exists():
        parser.error("输出目录已存在，请使用 --output 指定新目录")
    for tool in ("hmi", "hmi-compile", "hmi-media", "hmi-link"):
        if not (binaries / (tool + suffix)).is_file():
            parser.error("请先运行 cargo build --workspace --bins")

    def run(tool, *arguments):
        if tool not in {"hmi", "hmi-compile", "hmi-media", "hmi-link"}:
            raise RuntimeError(f"执行计划包含未知工具：{tool}")
        result = subprocess.run([str(binaries / (tool + suffix)), *map(str, arguments), "--json"],
                                capture_output=True, text=True, encoding="utf-8", check=False)
        if result.returncode:
            raise RuntimeError(result.stdout + result.stderr)
        value = json.loads(result.stdout)
        if not value["ok"] or value["diagnostics"]:
            raise RuntimeError(result.stdout)
        return value["result"]

    output.mkdir(parents=True)
    cases = []
    for orientation, width, height in [(0, 320, 480), (90, 480, 320)]:
        project = output / f"project-{orientation}"
        run("hmi", "init", project, "--target", "TJC4832K035_011", "--orientation", orientation)
        seed = project / "pages/page0/page.toml"
        seed.unlink()
        seed.parent.rmdir()
        assets = project / "assets"
        assets.mkdir()
        (assets / "background.bmp").write_bytes(bitmap(width, height))
        (assets / "logo.png").write_bytes(png_logo())
        (assets / "background.asset.toml").write_text(
            'schema="hmi.asset/1"\nkind="image"\nsource="background.bmp"\n', encoding="utf-8")
        (assets / "logo.asset.toml").write_text('''schema="hmi.asset/1"
kind="image"
source="logo.png"
[encoding]
alpha="flatten"
background="#FFFFFF"
[resize]
width=72
height=48
mode="stretch"
''', encoding="utf-8")
        order = ["background", "logo"] if orientation == 0 else ["logo", "background"]
        items = ",".join('{key="' + key + '",source="assets/' + key + '.asset.toml"}' for key in order)
        (project / "hmi.toml").write_text(f'''schema="hmi.project/1"
name="Pictures{orientation}"
[target]
profile="TJC4832K035_011"
format="ver21234"
encoding="utf-8"
orientation={orientation}
[program]
source="program.s"
[pages]
sources=["pages/overview/page.toml","pages/detail/page.toml"]
[assets]
items=[{items}]
''', encoding="utf-8")
        for name in ["overview", "detail"]:
            (project / "pages" / name).mkdir(parents=True)
        (project / "pages/overview/page.toml").write_text(f'''schema="hmi.page/1"
name="overview"
[[objects]]
name="bg0"
type="picture"
properties={{x=0,y=0,w={width},h={height},pic={{ref="background"}}}}
[[objects]]
name="logo0"
type="picture"
properties={{x=20,y=20,w=72,h=48,pic={{ref="logo"}}}}
events={{up="logo.up.s"}}
''', encoding="utf-8")
        (project / "pages/overview/logo.up.s").write_text("page detail\n", encoding="utf-8")
        x, y = width // 2 - 32, height // 2 - 32
        (project / "pages/detail/page.toml").write_text(f'''schema="hmi.page/1"
name="detail"
[[objects]]
name="crop0"
type="crop"
properties={{x={x},y={y},w=96,h=96,picc={{ref="background"}}}}
events={{up="crop.up.s"}}
''', encoding="utf-8")
        (project / "pages/detail/crop.up.s").write_text("page overview\n", encoding="utf-8")
        run("hmi", "check", project, "-Werror")
        hmi = output / f"Pictures{orientation}.HMI"
        run("hmi", "build", project, "-o", hmi, "-Werror")
        plan = project / ".build/manual"
        run("hmi", "plan", project, "-o", plan)
        jobs = json.loads((plan / "jobs.json").read_bytes())["jobs"]
        for job in jobs:
            run(job["command"], *job["args"])
        manual = project / f"dist/Pictures{orientation}/Pictures{orientation}.HMI"
        assert hmi.read_bytes() == manual.read_bytes()
        run("hmi-link", plan / "link.toml", "--check", "-Werror")
        inspection = run("hmi-compile", "inspect", plan / "obj/p00000.hmo")
        assert inspection["objects"][2]["properties"]["pic"] == {"ref": "logo"}
        cases.append({"file": hmi.name, "sha256": hashlib.sha256(hmi.read_bytes()).hexdigest(),
                      "profile": "TJC4832K035_011", "orientation": orientation, "page_size": [width, height],
                      "background_id": order.index("background"), "logo_id": order.index("logo"),
                      "crop_geometry": [x, y, 96, 96], "standalone_jobs": len(jobs),
                      "standalone_equals_driver": True, "opened": None, "compiled": None, "content_verified": None})
    (output / "verification.json").write_text(json.dumps({"generated_on": date.today().isoformat(), "stage": "pictures-rgb565", "automatic_checks_passed": True, "cases": cases},
                                                        ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    (output / "README.md").write_text('''# 图片与切图验收

请用 USART HMI 打开 Pictures0.HMI 和 Pictures90.HMI，核对后分别编译。需要保存时请另存副本；原始摘要保存在 verification.json。

型号均为 TJC4832K035_011，包含 overview、detail 两页，无字库。

| 文件 | 方向／页面尺寸 | 图片资源 ID |
|---|---|---|
| Pictures0.HMI | 0°，320×480 | 0：全屏背景；1：72×48 图标 |
| Pictures90.HMI | 90°，480×320 | 0：72×48 图标；1：全屏背景 |

overview 中 bg0 是全屏图片，ID 1；logo0 在 20/20，尺寸 72×48，控件 ID 2。背景四区分别为左上红、右上绿、左下蓝、右下黄，带黑色网格。图标由透明 PNG 缩放并在白色背景上去透明，左半为浅红、右半为蓝，四周白边。两份文件刻意交换资产顺序，控件应仍引用正确图片。

detail 中仅有 crop0，控件 ID 1，从全屏背景中心附近取 96×96 区域；0° 坐标为 128/208，90° 为 208/128，页面其余部分白色。切图应显示对应区域，不是整张图缩小。logo0 的 up 为 page detail；crop0 的 up 为 page overview。

请反馈：两份是否正常打开、方向和尺寸是否正确、图片颜色／方向及切图是否符合说明、编译是否通过及具体提示。当前不要求连接硬件，也不要求启动模拟器。

源码在 project-0 和 project-90。每份都已通过统一入口和导出的 6 条独立命令生成，两个输出完全相同；独立链接检查通过。
''', encoding="utf-8")
    print(f"图片验收工程已生成：{output}")


if __name__ == "__main__":
    main()

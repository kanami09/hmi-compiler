"""生成 X543 七位透明度图片验收工程，不需要外部媒体素材。"""
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
import zlib
from prepare_pictures_acceptance import bitmap


def ring_png():
    width, height = 69, 53
    rows = bytearray()
    for y in range(height):
        rows.append(0)
        for x in range(width):
            distance = math.hypot(x - 34, y - 26)
            if distance <= 7 or distance >= 24:
                alpha = 0
            elif distance < 10:
                alpha = round((distance - 7) * 255 / 3)
            elif distance <= 18:
                alpha = 255
            else:
                alpha = round((24 - distance) * 255 / 6)
            rows.extend((0, 220, 255, alpha))
    def chunk(tag, data):
        return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data))
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b""))


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=root / "target/acceptance-alpha")
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
    for orientation, width, height in [(0, 480, 272), (90, 272, 480)]:
        project = output / f"project-{orientation}"
        run("hmi", "init", project, "--target", "TJC4827X543_011", "--orientation", orientation)
        assets = project / "assets"
        assets.mkdir()
        (assets / "background.bmp").write_bytes(bitmap(width, height))
        (assets / "ring.png").write_bytes(ring_png())
        (assets / "background.asset.toml").write_text('schema="hmi.asset/1"\nkind="image"\nsource="background.bmp"\n', encoding="utf-8")
        (assets / "ring.asset.toml").write_text('schema="hmi.asset/1"\nkind="image"\nsource="ring.png"\n[encoding]\nquality=96\nalpha="preserve"\n', encoding="utf-8")
        (assets / "matte.asset.toml").write_text('schema="hmi.asset/1"\nkind="image"\nsource="ring.png"\n[encoding]\nquality=96\nalpha="flatten"\nbackground="#FFFFFF"\n', encoding="utf-8")
        order = ["background", "ring", "matte"] if orientation == 0 else ["matte", "ring", "background"]
        items = ",".join('{key="' + key + '",source="assets/' + key + '.asset.toml"}' for key in order)
        (project / "hmi.toml").write_text(f'''schema="hmi.project/1"
name="Alpha{orientation}"
[target]
profile="TJC4827X543_011"
format="ver21234"
encoding="utf-8"
orientation={orientation}
[program]
source="program.s"
[pages]
sources=["pages/page0/page.toml"]
[assets]
items=[{items}]
''', encoding="utf-8")
        x, y = width - 100, height - 100
        (project / "pages/page0/page.toml").write_text(f'''schema="hmi.page/1"
name="page0"
[[objects]]
name="bg0"
type="picture"
properties={{x=0,y=0,w={width},h={height},pic={{ref="background"}}}}
[[objects]]
name="ring0"
type="picture"
properties={{x=20,y=20,w=69,h=53,pic={{ref="ring"}}}}
[[objects]]
name="ring1"
type="picture"
properties={{x={x},y={y},w=69,h=53,pic={{ref="ring"}}}}
[[objects]]
name="matte0"
type="picture"
properties={{x=120,y=20,w=69,h=53,pic={{ref="matte"}}}}
''', encoding="utf-8")
        run("hmi", "check", project, "-Werror")
        assert not (project / ".build").exists()
        hmi = output / f"Alpha{orientation}.HMI"
        run("hmi", "build", project, "-o", hmi, "-Werror")
        plan = project / ".build/manual"
        run("hmi", "plan", project, "-o", plan)
        jobs = json.loads((plan / "jobs.json").read_bytes())["jobs"]
        for job in jobs:
            run(job["command"], *job["args"])
        manual = project / f"dist/Alpha{orientation}/Alpha{orientation}.HMI"
        assert hmi.read_bytes() == manual.read_bytes()
        run("hmi-link", plan / "link.toml", "--check", "-Werror")
        images = {}
        for i, key in enumerate(order):
            info = run("hmi-media", "inspect", plan / f"assets/a{i:05}.hma")["metadata"]["image"]
            assert info["jpeg"]["has_alpha"] == (key == "ring")
            assert info["editor_source_format"] == ("png" if key == "ring" else "bmp")
            images[key] = {"id": i, **info}
        cases.append({"file": hmi.name, "sha256": hashlib.sha256(hmi.read_bytes()).hexdigest(),
                      "profile": "TJC4827X543_011", "orientation": orientation, "page_size": [width, height],
                      "second_ring_position": [x, y], "images": images,
                      "standalone_jobs": len(jobs), "standalone_equals_driver": True,
                      "opened": None, "compiled": None, "content_verified": None})
    (output / "verification.json").write_text(json.dumps({"generated_on": date.today().isoformat(),
        "stage": "x543-alpha7", "automatic_checks_passed": True, "cases": cases},
        ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    (output / "README.md").write_text('''# X543 透明图片验收

请用 USART HMI 打开 Alpha0.HMI 和 Alpha90.HMI，核对图片后分别编译。需要保存时请另存副本，原始摘要在 verification.json。

型号均为 TJC4827X543_011；0° 页面 480×272，90° 页面 272×480。均为一页、四个图片控件、三个逻辑图片，无字库和事件代码。

| 文件 | 图片 ID 顺序 | ring1 坐标 |
|---|---|---|
| Alpha0.HMI | 0 背景、1 透明圆环、2 白底圆环 | 380/172 |
| Alpha90.HMI | 0 白底圆环、1 透明圆环、2 背景 | 172/380 |

bg0（控件 ID 1）为全屏底图，左上红、右上绿、左下蓝、右下黄，带黑色网格。两份相对各自页面的方向应一致。

ring0（ID 2）位于 20/20、ring1（ID 3）位于上表坐标，均为 69×53，共用同一透明圆环。圆环为青色，有完全不透明的环带，内外边缘逐渐透明；中央孔和四角完全透明，应能看见底图及网格，不能出现白色矩形底或内部补齐的黑边。两个圆环叠在不同底色上，半透明边缘的混合色应随底图变化。

matte0（ID 4）位于 120/20、69×53，使用同一 PNG 显式去透明到白底。它应是带白色矩形底的青色圆环，孔和四角为白色。它用于核对 preserve 与 flatten 的实际区别。

透明圆环颜色质量为 96，运行透明度为七位、存在量化；源 PNG 编辑副本仍保留八位透明度。内部补齐画布为 80×64，90° 存储为 64×80，但画布上仍应显示 69×53，方向、孔和边缘位置正确。

图片列表应有三项，透明圆环带透明标记（例如 png-alpha），背景和白底圆环不带 alpha 后缀。请核对实际预览，不手动修改图像透明度或替换素材。

请反馈两份是否正常打开，透明／半透明、方向、尺寸及叠放是否符合说明，以及编译结果和提示。本批核对编辑器预览和编译，模拟器或硬件运行另行验证。

源码在 project-0／project-90。脚本只使用标准库和公开的底图生成函数，不读取已有 HMI 或内部素材。每份实际执行 6 条独立命令，输出与统一入口相同，独立链接检查通过。
''', encoding="utf-8")
    print(f"透明图片验收工程已生成：{output}")


if __name__ == "__main__":
    main()

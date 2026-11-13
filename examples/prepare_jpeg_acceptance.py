"""生成 X543 JPEG 图片与切图验收工程，只用标准库生成 BMP／PNG 素材。"""
import argparse
from datetime import date
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
from prepare_pictures_acceptance import bitmap, png_logo


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=root / "target/acceptance-jpeg")
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
        seed = project / "pages/page0/page.toml"
        seed.unlink()
        seed.parent.rmdir()
        assets = project / "assets"
        assets.mkdir()
        (assets / "background.bmp").write_bytes(bitmap(width, height))
        (assets / "logo.png").write_bytes(png_logo())
        (assets / "tiny.bmp").write_bytes(bitmap(24, 16))
        (assets / "background.asset.toml").write_text('schema="hmi.asset/1"\nkind="image"\nsource="background.bmp"\n', encoding="utf-8")
        (assets / "logo.asset.toml").write_text('''schema="hmi.asset/1"
kind="image"
source="logo.png"
[encoding]
quality=96
alpha="flatten"
background="#FFFFFF"
[resize]
width=69
height=45
mode="stretch"
''', encoding="utf-8")
        (assets / "tiny.asset.toml").write_text('schema="hmi.asset/1"\nkind="image"\nsource="tiny.bmp"\n[encoding]\nquality=100\n', encoding="utf-8")
        order = ["background", "logo", "tiny"] if orientation == 0 else ["tiny", "logo", "background"]
        items = ",".join('{key="' + key + '",source="assets/' + key + '.asset.toml"}' for key in order)
        (project / "hmi.toml").write_text(f'''schema="hmi.project/1"
name="Jpeg{orientation}"
[target]
profile="TJC4827X543_011"
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
        for page in ["overview", "detail"]:
            (project / "pages" / page).mkdir(parents=True)
        (project / "pages/overview/page.toml").write_text(f'''schema="hmi.page/1"
name="overview"
[[objects]]
name="bg0"
type="picture"
properties={{x=0,y=0,w={width},h={height},pic={{ref="background"}}}}
[[objects]]
name="logo0"
type="picture"
properties={{x=20,y=20,w=69,h=45,pic={{ref="logo"}}}}
events={{up="logo0.up.s"}}
[[objects]]
name="tiny0"
type="picture"
properties={{x=120,y=20,w=24,h=16,pic={{ref="tiny"}}}}
''', encoding="utf-8")
        (project / "pages/overview/logo0.up.s").write_text("page detail\n", encoding="utf-8")
        x, y = width // 2 - 32, height // 2 - 32
        (project / "pages/detail/page.toml").write_text(f'''schema="hmi.page/1"
name="detail"
[[objects]]
name="crop0"
type="crop"
properties={{x={x},y={y},w=96,h=96,picc={{ref="background"}}}}
events={{up="crop0.up.s"}}
''', encoding="utf-8")
        (project / "pages/detail/crop0.up.s").write_text("page overview\n", encoding="utf-8")
        run("hmi", "check", project, "-Werror")
        assert not (project / ".build").exists()
        hmi = output / f"Jpeg{orientation}.HMI"
        run("hmi", "build", project, "-o", hmi, "-Werror")
        plan = project / ".build/manual"
        run("hmi", "plan", project, "-o", plan)
        jobs = json.loads((plan / "jobs.json").read_bytes())["jobs"]
        for job in jobs:
            run(job["command"], *job["args"])
        manual = project / f"dist/Jpeg{orientation}/Jpeg{orientation}.HMI"
        assert hmi.read_bytes() == manual.read_bytes()
        run("hmi-link", plan / "link.toml", "--check", "-Werror")
        images = {}
        for index, key in enumerate(order):
            info = run("hmi-media", "inspect", plan / f"assets/a{index:05}.hma")["metadata"]["image"]
            assert info["codec"] == "jpeg" and info["orientation"] == orientation
            images[key] = {"id": index, **info}
        assert images["logo"]["width"] == 69 and images["logo"]["height"] == 45
        assert images["tiny"]["jpeg"]["padded_width"] == 48
        cases.append({"file": hmi.name, "sha256": hashlib.sha256(hmi.read_bytes()).hexdigest(),
                      "profile": "TJC4827X543_011", "orientation": orientation, "page_size": [width, height],
                      "images": images, "crop_geometry": [x, y, 96, 96], "standalone_jobs": len(jobs),
                      "standalone_equals_driver": True, "opened": None, "compiled": None, "content_verified": None})
    (output / "verification.json").write_text(json.dumps({"generated_on": date.today().isoformat(),
        "stage": "x543-jpeg", "automatic_checks_passed": True, "cases": cases},
        ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    (output / "README.md").write_text('''# X543 JPEG 图片验收

请用 USART HMI 打开 Jpeg0.HMI 和 Jpeg90.HMI，核对图片后分别编译。需要保存时请另存副本，原始摘要在 verification.json。

型号均为 TJC4827X543_011，包含 overview、detail 两页，无字库。0° 页面 480×272，90° 页面 272×480。

| 文件 | 图片 ID 顺序 |
|---|---|
| Jpeg0.HMI | 0：全屏背景；1：69×45 图标；2：24×16 小图 |
| Jpeg90.HMI | 0：24×16 小图；1：69×45 图标；2：全屏背景 |

## overview

bg0（控件 ID 1）是全屏背景，左上红、右上绿、左下蓝、右下黄，带黑色网格。两份相对各自页面的颜色方向应一致，图片不应倒置或多转一次。背景质量为 80，允许 JPEG 压缩产生少量颜色和边缘误差。

logo0（ID 2）位于 20/20，69×45，由透明 PNG 在白底上去透明并缩放，左侧浅红、右侧蓝，四周白边；质量为 96。tiny0（ID 3）位于 120/20，24×16，为红／绿／蓝／黄四区小图，质量为 100。

重点核对图标和小图保持逻辑尺寸，未被内部补齐拉伸。图标内部画布为 80×48，小图内部为 48×48；90° 时存储光栅旋转，但画布上仍应显示原逻辑大小和正确方向。媒体输入为 BMP／PNG，本批运行数据都是 JPEG，透明度已显式去除。

## detail

只有 crop0（控件 ID 1），取全屏背景的 96×96 区域。0° 坐标为 208/104，90° 为 104/208；应显示原背景在相同位置的局部，页面其余部分为白色，不应显示整张图缩小。

logo0.up 为 page detail；crop0.up 为 page overview。

请反馈两份是否正常打开，图片列表是否齐全，颜色／方向／大小和切图是否符合说明，以及编译结果和提示。本批核对编辑器预览和编译；模拟器与硬件显示另行验证。

源码在 project-0／project-90。脚本只用标准库和仓库中的公开图片生成函数，不读取已有 HMI 或内部素材。每份实际执行 7 条独立命令，输出与统一入口相同，独立链接检查通过。
''', encoding="utf-8")
    print(f"JPEG 图片验收工程已生成：{output}")


if __name__ == "__main__":
    main()

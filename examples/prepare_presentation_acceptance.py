"""生成拖动、不透明度和加载特效组合验收，共两份HMI、12页。"""
import argparse
from datetime import date
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
from prepare_pictures_acceptance import bitmap
from prepare_browser_icons_acceptance import arrow, png


def tile(size):
    pixels = bytearray()
    for y in range(size):
        for x in range(size):
            color = (30, 90, 230, 255) if min(x, y, size - 1 - x, size - 1 - y) < 3 else (250, 190, 30, 255)
            pixels.extend(color)
    return png(size, size, pixels)


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--font", type=Path, required=True)
    parser.add_argument("--output", type=Path, default=root / "target/acceptance-presentation")
    parser.add_argument("--binaries", type=Path, default=root / "target/debug")
    args = parser.parse_args()
    output, binaries, font = args.output.resolve(), args.binaries.resolve(), args.font.resolve()
    if output.exists():
        parser.error("输出目录已存在，请使用 --output 指定新目录")
    if not font.is_file():
        parser.error("请提供独立UTF-8 ZI字库，建议沿用已验收字高68字库")
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
    cases, pages = [], ["boot", "drag", "opacity", "effects", "page_fx", "combined"]
    for orientation, width, height in [(0, 480, 272), (90, 272, 480)]:
        name = f"Visual{orientation}"
        print(f"正在生成 {name} 的公共属性验收页面", flush=True)
        project = output / name
        run("hmi", "init", project, "--target", "TJC4827X543_011", "--orientation", orientation)
        assets = project / "assets"
        assets.mkdir()
        shutil.copyfile(font, assets / "ui.zi")
        (assets / "ui.toml").write_text('schema="hmi.asset/1"\nkind="font"\nsource="ui.zi"\n', encoding="utf-8")
        images = {"full": ("bmp", bitmap(width, height)), "tile": ("png", tile(64)), "small": ("png", tile(32)),
                  "prev": ("png", arrow(False)), "next": ("png", arrow(True))}
        for key, (extension, data) in images.items():
            (assets / f"{key}.{extension}").write_bytes(data)
            (assets / f"{key}.toml").write_text(f'schema="hmi.asset/1"\nkind="image"\nsource="{key}.{extension}"\n', encoding="utf-8")
        keys = ["ui", *images]
        if orientation == 90:
            keys.reverse()
        items = ",".join(f'{{key="{key}",source="assets/{key}.toml"}}' for key in keys)
        sources = ",".join(f'"pages/{page}/page.toml"' for page in pages)
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
        for index, page in enumerate(pages):
            folder = project / "pages" / page
            folder.mkdir()
            background = 'sta=1,bco=0' if page == "boot" else f'sta=2,pic={{ref="full"}},effect={int(page == "page_fx")}'
            text = f'schema="hmi.page/1"\nname="{page}"\nproperties={{{background}}}\n'

            def add(control, kind, props):
                return f'[[objects]]\nname="{control}"\ntype="{kind}"\nproperties={{{props}}}\n'

            if page in ["drag", "combined", "page_fx"]:
                active = page != "page_fx"
                combined = page == "combined"
                text += add("p0", "picture", f'x=20,y=30,w=64,h=64,pic={{ref="tile"}},drag={int(active)},aph={96 if combined else 127},effect={4 if combined else 0}')
                text += add("t0", "text", f'x=20,y=130,w=232,h=100,font={{ref="ui"}},txt="ABC",drag={int(active)},aph={80 if combined else 127},effect={3 if combined else 0}')
                text += add("z0", "gauge", f'x=100,y=30,w=120,h=120,val=45,drag={int(active)},aph={96 if combined else 127},effect={2 if combined else 0}')
            elif page == "opacity":
                for n, alpha in enumerate([127, 64, 0]):
                    text += add(f"p{n}", "picture", f'x={20 + n * 80},y=30,w=64,h=64,pic={{ref="tile"}},aph={alpha}')
                text += add("z0", "gauge", 'x=20,y=120,w=110,h=110,val=135,aph=64')
            elif page == "effects":
                for effect in range(1, 9):
                    text += add(f"p{effect}", "picture", f'x={20 + ((effect - 1) % 2) * 100},y={20 + ((effect - 1) // 2) * 48},w=32,h=32,pic={{ref="small"}},effect={effect}')
            for control, key, x, destination in [("back", "prev", 12, pages[(index - 1) % len(pages)]),
                                                  ("forward", "next", width - 44, pages[(index + 1) % len(pages)])]:
                text += add(control, "picture", f'x={x},y={height - 32},w=32,h=24,pic={{ref="{key}"}}') + f'events={{up="{control}.s"}}\n'
                (folder / f"{control}.s").write_text(f"page {destination}\n", encoding="utf-8")
            (folder / "page.toml").write_text(text, encoding="utf-8")
        run("hmi", "check", project, "-Werror")
        assert not (project / ".build").exists()
        hmi = output / f"{name}.HMI"
        run("hmi", "build", project, "-o", hmi, "-Werror")
        plan = project / ".build/manual"
        run("hmi", "plan", project, "-o", plan)
        jobs = json.loads((plan / "jobs.json").read_bytes())["jobs"]
        assert len(jobs) == 14
        for job in jobs:
            run(job["command"], *job["args"])
        assert hmi.read_bytes() == (project / f"dist/{name}/{name}.HMI").read_bytes()
        drag = run("hmi-compile", "inspect", plan / "obj/p00001.hmo")["objects"][1]["properties"]
        assert drag["drag"] == 1 and drag["aph"] == 127 and drag["effect"] == 0
        page_fx = run("hmi-compile", "inspect", plan / "obj/p00004.hmo")["objects"][0]["properties"]
        # 页面inspect也应显示公共effect，不只列背景配置。
        assert page_fx["effect"] == 1
        cases.append({"file": hmi.name, "sha256": hashlib.sha256(hmi.read_bytes()).hexdigest(), "orientation": orientation,
                      "pages": pages, "standalone_jobs": len(jobs), "standalone_equals_driver": True,
                      "opened": None, "compiled": None, "content_verified": None, "simulator_verified": None})
    (output / "verification.json").write_text(json.dumps({"generated_on": date.today().isoformat(), "stage": "presentation-properties",
        "automatic_checks_passed": True, "cases": cases}, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    (output / "README.md").write_text('''# 拖动、不透明度和加载特效验收

Visual0.HMI为X543／0°／480×272，Visual90为90°／272×480。每份六页，共12页。图片和字体全部内嵌，不需要虚拟SD或FFmpeg。请各编译一次，并在模拟器使用页底左右蓝色箭头连续查看各页；需要保存时另存副本，原始摘要见verification.json。

| 页面 | 检查内容 |
|---|---|
| boot | 黑色开机页；点击右箭头进入drag，避免第一个页面不显示加载特效的原生限制 |
| drag | 图片p0、文本t0、指针z0均drag1、aph127、effect0；尝试分别拖动三个控件 |
| opacity | 三张相同图片在20/30、100/30、180/30，aph依次127／64／0；应为不透明／半透明／不可见。下方指针aph64 |
| effects | 八张32×32图，按两列四行排列：从左上按行依次effect1～8；重新进入此页可重看加载过程 |
| page_fx | 页面自身effect1，三个控件effect0、drag0、aph127，用于单独核对页面加载特效 |
| combined | 图片／文本／指针均drag1，aph96／80／96，effect4／3／2，核对组合配置没有互相重置 |

图片为蓝框黄色方块，背景是全屏四象限彩色网格。drag/page_fx/combined三页的图片20/30、64×64；文本20/130、232×100、ABC、font0；指针100/30、120×120、val45。opacity页指针20/120、110×110、val135。效果页坐标为x20／120、y20／68／116／164，均32×32。已有指针尖长／尾长及其他形状参数保持默认，没有开放含义和范围仍未确认的format/left/wid。

控件特效1～8依次为上、下、左、右、左上、右上、左下、右下飞入；0为立即加载。只核对上位机配置、模拟器拖动／透明和加载表现，不要求实机。页面effect当前开放0～8，本批实际检查1；画布静态显示不能代替模拟器动态特效验证。effect初始为0时不能靠运行指令改成其他特效，本工程在源码中直接设定，不通过脚本猜测或自动改变初始值。

导航图片控件均保持默认drag0/aph127/effect0；其余未显式设置的公共属性也使用这些默认值。前三个可视控件在drag/page_fx/combined页分别ID1／2／3，导航在末尾。图片列表在90°反序，字段引用按当前资产名绑定，显示方向应正确。

每份实际执行14条独立作业，HMI与统一构建入口逐字节相同；check不写构建目录。只需独立ZI生成本批，其余图片和箭头由代码生成，不读已有HMI或内部素材。T0／K0通过同一型号约束表固定公共属性为默认值，不属于本批X系列动态验收。
''', encoding="utf-8")
    print(output)


if __name__ == "__main__":
    main()

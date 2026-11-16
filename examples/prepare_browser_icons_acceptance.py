"""生成自动图标、显式图标和两种自定义映射的验收工程，共两份HMI、10页。"""
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
import zlib


def png(width, height, pixels):
    def chunk(tag, data):
        return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data))
    raw = b"".join(b"\0" + pixels[y * width * 4:(y + 1) * width * 4] for y in range(height))
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b""))


def icon(color, folder=False):
    pixels = bytearray(64 * 64 * 4)
    def rect(left, top, right, bottom, rgb):
        for y in range(top, bottom):
            for x in range(left, right):
                pixels[(y * 64 + x) * 4:(y * 64 + x) * 4 + 4] = bytes((*rgb, 255))
    if folder:
        rect(8, 10, 31, 24, color)
        rect(6, 20, 58, 54, color)
    else:
        rect(15, 6, 49, 58, color)
        for y in [25, 33, 41]:
            rect(23, y, 41, y + 3, (255, 255, 255))
    return png(64, 64, pixels)


def arrow(right):
    pixels = bytearray(32 * 24 * 4)
    for y in range(24):
        for x in range(32):
            xx = x if right else 31 - x
            if (5 <= xx <= 19 and 9 <= y <= 14) or (12 <= xx <= 27 and abs(y - 12) <= (27 - xx)):
                pixels[(y * 32 + x) * 4:(y * 32 + x) * 4 + 4] = bytes([30, 90, 220, 255])
    return png(32, 24, pixels)


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--font", type=Path, required=True, help="独立UTF-8 ZI，建议沿用已验收字高68字库")
    parser.add_argument("--output", type=Path, default=root / "target/acceptance-browser-icons")
    parser.add_argument("--binaries", type=Path, default=root / "target/debug")
    args = parser.parse_args()
    output, binaries, font = args.output.resolve(), args.binaries.resolve(), args.font.resolve()
    if output.exists():
        parser.error("输出目录已存在，请使用 --output 指定新目录")
    if not font.is_file():
        parser.error("请提供独立ZI字库")
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
    cases, pages = [], ["auto", "mixed", "explicit", "typed", "numeric"]
    for orientation, width, height in [(0, 480, 272), (90, 272, 480)]:
        name = f"Icons{orientation}"
        print(f"正在生成 {name} 的五种图标配置", flush=True)
        project = output / name
        run("hmi", "init", project, "--target", "TJC4827X543_011", "--orientation", orientation)
        assets = project / "assets"
        assets.mkdir()
        shutil.copyfile(font, assets / "ui.zi")
        (assets / "ui.toml").write_text('schema="hmi.asset/1"\nkind="font"\nsource="ui.zi"\n', encoding="utf-8")
        images = {"txt": icon((30, 200, 80)), "log": icon((230, 50, 50)),
                  "folder": icon((150, 60, 210), True), "file": icon((245, 150, 30)),
                  "prev": arrow(False), "next": arrow(True)}
        for key, data in images.items():
            (assets / f"{key}.png").write_bytes(data)
            (assets / f"{key}.toml").write_text(f'schema="hmi.asset/1"\nkind="image"\nsource="{key}.png"\n', encoding="utf-8")
        keys = ["ui", *images]
        if orientation == 90:
            keys.reverse()
        image_keys = [key for key in keys if key != "ui"]
        ids = {key: image_keys.index(key) for key in image_keys}
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
        directory = output / f"sd0/browser{orientation}"
        (directory / "sub").mkdir(parents=True)
        for path, content in [("a.txt", b"A\n"), ("b.log", b"B\n"), ("c.bin", bytes(range(16))), ("sub/child.txt", b"C\n")]:
            (directory / path).write_bytes(content)
        for index, page in enumerate(pages):
            folder = project / "pages" / page
            folder.mkdir()
            props = ["x=20", "y=20", f"w={width - 40}", f"h={height - 72}", 'font={ref="ui"}',
                     f'dir="sd0/browser{orientation}/"', 'filter="*.txt|*.log|*.bin"', "psta=1"]
            if page in ["mixed", "explicit"]:
                props += ['pic1={ref="folder"}']
            if page == "explicit":
                props += ['pic2={ref="file"}']
            if page == "typed":
                props += ['vvs2=[{pattern="*.txt",pic={ref="txt"}},{pattern="*.log",pic={ref="log"}}]']
            if page == "numeric":
                props += [f'vvs2="*.txt/{ids["txt"]}*.log/{ids["log"]}"']
            text = f'''schema="hmi.page/1"
name="{page}"
[[objects]]
name="fb0"
type="file_browser"
properties={{{",".join(props)}}}
events={{up="select.s"}}
'''
            for control, key, x, destination in [("back", "prev", 12, pages[(index - 1) % len(pages)]),
                                                  ("forward", "next", width - 44, pages[(index + 1) % len(pages)])]:
                text += f'[[objects]]\nname="{control}"\ntype="picture"\nproperties={{x={x},y={height - 32},w=32,h=24,pic={{ref="{key}"}}}}\nevents={{up="{control}.s"}}\n'
                (folder / f"{control}.s").write_text(f"page {destination}\n", encoding="utf-8")
            (folder / "page.toml").write_text(text, encoding="utf-8")
            (folder / "select.s").write_text("sys0=fb0.val\n", encoding="utf-8")
        run("hmi", "check", project, "-Werror")
        assert not (project / ".build").exists()
        hmi = output / f"{name}.HMI"
        run("hmi", "build", project, "-o", hmi, "-Werror")
        plan = project / ".build/manual"
        run("hmi", "plan", project, "-o", plan)
        jobs = json.loads((plan / "jobs.json").read_bytes())["jobs"]
        assert len(jobs) == 16
        assert [j["args"][1] for j in jobs if j["command"] == "hmi-media" and j["args"][0] == "builtin"] == ["browser-folder", "browser-file"]
        for job in jobs:
            run(job["command"], *job["args"])
        default = project / f"dist/{name}"
        assert hmi.read_bytes() == (default / hmi.name).read_bytes()
        mapping = json.loads((output / "link-map.json").read_bytes())
        implicit = [a for a in mapping["assets"] if a.get("implicit")]
        assert [a["id"] for a in implicit] == [6, 7]
        inspected = run("hmi-compile", "inspect", plan / "obj/p00003.hmo")
        assert len(inspected["implicit_assets"]) == 2
        assert inspected["objects"][1]["properties"]["vvs2"][0]["pic"] == {"ref": "txt"}
        cases.append({"file": hmi.name, "sha256": hashlib.sha256(hmi.read_bytes()).hexdigest(), "orientation": orientation,
                      "pages": pages, "image_ids": {**ids, "builtin_folder": 6, "builtin_file": 7}, "implicit_assets": 2,
                      "standalone_jobs": len(jobs), "standalone_equals_driver": True, "virtual_sd": f"browser{orientation}",
                      "opened": None, "compiled": None, "content_verified": None, "simulator_icons_verified": None})
    (output / "verification.json").write_text(json.dumps({"generated_on": date.today().isoformat(), "stage": "browser-icons",
        "automatic_checks_passed": True, "cases": cases}, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    (output / "README.md").write_text('''# 文件浏览器图标验收

共两份HMI，各五页。Icons0为X543／0°／480×272，Icons90为90°／272×480。请先把本目录sd0中的browser0、browser90两个子目录复制到上位机虚拟SD根目录，再打开HMI核对属性、每份编译一次，并在模拟器中查看文件列表。设计画布若不列出文件，以模拟器中的列表为准。需要保存HMI时请另存副本，原始摘要见verification.json。

各页底部左、右蓝色箭头可循环切换页面，顺序auto→mixed→explicit→typed→numeric→auto。可滚动列表查看未显示的项目；有sub文件夹及a.txt、b.log、c.bin三份文件。字体ID0，建议沿用字高68。fb0始终ID1，位置20/20、宽440／232、高200／408，单色白底、psta1、目录sd0/browser<方向>/；两个切换图片控件ID2／3。fb0的up事件sys0=fb0.val。

| 页面 | 文件夹sub | a.txt、b.log | c.bin |
|---|---|---|---|
| auto | 自动黄色文件夹 | 自动蓝白文件图标 | 同左 |
| mixed | 显式紫色文件夹 | 自动蓝白文件图标 | 同左 |
| explicit | 显式紫色文件夹 | 显式橙色文件图标 | 同左 |
| typed | 自动黄色文件夹 | a.txt绿色、b.log红色 | 自动蓝白文件图标 |
| numeric | 同typed | 同typed | 同typed |

typed使用结构化pattern/pic符号引用，numeric使用原生数值字符串；本工程已按各自最终资产顺序填写数值，两页应显示一致。自定义映射不能影响默认文件夹图标，未匹配的.bin仍使用pic2。图标为64×64透明图像，内置两张由项目自绘；显式四张也是代码生成，无FFmpeg需求。

| 图片 | Icons0 ID | Icons90 ID |
|---|---|---|
| txt绿色 | 0 | 5 |
| log红色 | 1 | 4 |
| folder紫色 | 2 | 3 |
| file橙色 | 3 | 2 |
| prev左箭头 | 4 | 1 |
| next右箭头 | 5 | 0 |
| 内置黄色文件夹 | 6 | 6 |
| 内置蓝白文件 | 7 | 7 |

显式图片在90°反序，字体仍ID0；两张内置图标固定按文件夹／默认文件顺序追加，五页共享，不逐页重复生成。typed/numeric的vvs2分别应为0°的*.txt/0*.log/1、90°的*.txt/5*.log/4；实际容量64字节，其他原生缓存不因映射改写。

每份实际执行16条独立作业，其中两条hmi-media builtin负责内置图标；输出HMI与统一入口逐字节一致。check不生成.build或补写工程源文件。只需指定独立ZI生成本批工程，其余PNG、箭头和SD示例由代码生成，不读取已有HMI或内部素材。脚本不自动修改用户虚拟SD；本批不要求硬件测试。
''', encoding="utf-8")
    print(output)


if __name__ == "__main__":
    main()

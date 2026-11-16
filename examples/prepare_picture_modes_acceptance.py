"""生成文本／按钮／滑块／进度条图片模式验收工程，不读取已有 HMI。"""
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
from prepare_pictures_acceptance import bitmap


def panel(width, height, color, cursor=False):
    stride = (width * 3 + 3) & ~3
    pixels = bytearray(stride * height)
    for y in range(height):
        for x in range(width):
            rgb = (255, 230, 0) if cursor and (abs(x - width // 2) < 2 or abs(y - height // 2) < 2) else color
            if not cursor and (x % 20 == 0 or y % 20 == 0):
                rgb = tuple(max(0, c - 35) for c in color)
            p = (height - 1 - y) * stride + x * 3
            pixels[p:p + 3] = bytes(rgb[::-1])
    return (b"BM" + struct.pack("<IHHI", 54 + len(pixels), 0, 0, 54)
            + struct.pack("<IiiHH6I", 40, width, height, 1, 24, 0, len(pixels), 0, 0, 0, 0) + pixels)


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--font", type=Path, required=True, help="UTF-8 ZI 字库，建议使用已验收的字高 68 字库")
    parser.add_argument("--output", type=Path, default=root / "target/acceptance-picture-modes")
    parser.add_argument("--binaries", type=Path, default=root / "target/debug")
    args = parser.parse_args()
    output, binaries, font = args.output.resolve(), args.binaries.resolve(), args.font.resolve()
    if output.exists():
        parser.error("输出目录已存在，请使用 --output 指定新目录")
    if not font.is_file():
        parser.error("请提供独立 ZI 字库")
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

    output.mkdir(parents=True)
    cases = []
    specs = [("ModesPic", "TJC4832K035_011", 0, 320, 480, "picture"),
             ("ModesCrop", "TJC4832K035_011", 0, 320, 480, "crop"),
             ("ModesColor", "TJC4832K035_011", 0, 320, 480, "color"),
             ("ModesX90", "TJC4827X543_011", 90, 272, 480, "picture")]
    for name, profile, orientation, width, height, mode in specs:
        project = output / name
        run("hmi", "init", project, "--target", profile, "--orientation", orientation)
        assets = project / "assets"
        assets.mkdir()
        shutil.copyfile(font, assets / "ui.zi")
        (assets / "ui.asset.toml").write_text('schema="hmi.asset/1"\nkind="font"\nsource="ui.zi"\n', encoding="utf-8")
        panel_width = 280 if profile == "TJC4832K035_011" else 232
        inputs = {"full": bitmap(width, height), "panel": panel(panel_width, 100, (190, 220, 250)),
                  "pressed": panel(panel_width, 100, (250, 195, 135)), "bar": panel(panel_width, 40, (140, 150, 160)),
                  "green": panel(panel_width, 40, (30, 210, 70)), "cursor": panel(20, 20, (230, 30, 30), True)}
        for key, data in inputs.items():
            (assets / f"{key}.bmp").write_bytes(data)
            (assets / f"{key}.asset.toml").write_text(f'schema="hmi.asset/1"\nkind="image"\nsource="{key}.bmp"\n', encoding="utf-8")
        keys = ["full", "panel", "pressed", "bar", "green", "cursor", "ui"]
        if orientation == 90:
            keys.reverse()
        items = ",".join('{key="' + key + '",source="assets/' + key + '.asset.toml"}' for key in keys)
        (project / "hmi.toml").write_text(f'''schema="hmi.project/1"
name="{name}"
[target]
profile="{profile}"
format="ver21234"
encoding="utf-8"
orientation={orientation}
[program]
source="program.s"
[pages]
sources=["pages/text/page.toml","pages/button/page.toml","pages/slider/page.toml","pages/progress/page.toml"]
[assets]
items=[{items}]
''', encoding="utf-8")
        for page, kind, control in [("text", "text", "t0"), ("button", "button", "b0"), ("slider", "slider", "h0"), ("progress", "progress_bar", "j0")]:
            directory = project / "pages" / page
            directory.mkdir()
            is_text = page in ["text", "button"]
            sta = {"picture": 2, "crop": 0, "color": 1}[mode] if page != "progress" else (0 if mode == "color" else 1)
            props = [f'x={10 if is_text else 20}', f'y={10 if is_text else 40}', f'w={panel_width}', f'h={100 if is_text else 40}', f'sta={sta}']
            if is_text:
                props.extend(['font={ref="ui"}', f'txt="{"ABC" if page == "text" else "OK"}"'])
            else:
                props.append("val=40")
            mapping = {"text": [("pic", "panel")], "button": [("pic", "panel"), ("pic2", "pressed")],
                       "slider": [("pic", "bar"), ("pic1", "green"), ("pic2", "cursor")],
                       "progress": [("bpic", "bar"), ("ppic", "green")]}[page]
            if mode != "picture" and page != "progress":
                mapping += [("picc", "full")]
                if page == "button":
                    mapping += [("picc2", "full")]
                if page == "slider":
                    mapping += [("picc1", "full")]
            props.extend(field + '={ref="' + key + '"}' for field, key in mapping)
            if page == "slider":
                props.append("psta=" + ("0" if mode == "color" else "1"))
            text = f'schema="hmi.page/1"\nname="{page}"\n[[objects]]\nname="{control}"\ntype="{kind}"\nproperties={{{",".join(props)}}}\n'
            if page != "text":
                events = ["down", "up", "slide"] if page == "slider" else ["down", "up"]
                text += 'events={' + ",".join(event + '="' + event + '.s"' for event in events) + '}\n'
                for index, event in enumerate(events):
                    (directory / f"{event}.s").write_text(f"sys0={index + 1}\n", encoding="utf-8")
            (directory / "page.toml").write_text(text, encoding="utf-8")
        run("hmi", "check", project, "-Werror")
        assert not (project / ".build").exists()
        hmi = output / f"{name}.HMI"
        run("hmi", "build", project, "-o", hmi, "-Werror")
        plan = project / ".build/manual"
        run("hmi", "plan", project, "-o", plan)
        jobs = json.loads((plan / "jobs.json").read_bytes())["jobs"]
        for job in jobs:
            run(job["command"], *job["args"])
        assert hmi.read_bytes() == (project / f"dist/{name}/{name}.HMI").read_bytes()
        run("hmi-link", plan / "link.toml", "--check", "-Werror")
        control = run("hmi-compile", "inspect", plan / "obj/p00002.hmo")["objects"][1]
        assert control["properties"]["pic2"] == {"ref": "cursor"}
        image_order = [key for key in keys if key != "ui"]
        cases.append({"file": hmi.name, "sha256": hashlib.sha256(hmi.read_bytes()).hexdigest(), "profile": profile,
                      "orientation": orientation, "page_size": [width, height], "mode": mode,
                      "image_ids": {key: image_order.index(key) for key in image_order}, "font_id": 0,
                      "standalone_jobs": len(jobs), "standalone_equals_driver": True,
                      "opened": None, "compiled": None, "content_verified": None})
    (output / "verification.json").write_text(json.dumps({"generated_on": date.today().isoformat(), "stage": "picture-background-modes",
        "automatic_checks_passed": True, "cases": cases}, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    (output / "README.md").write_text('''# 图片背景与图片游标验收

请分别打开 ModesPic.HMI、ModesCrop.HMI、ModesColor.HMI、ModesX90.HMI，核对 text／button／slider／progress 四页并编译。资源均已内嵌，无需虚拟 SD 或 FFmpeg；需要保存时请另存副本，原始摘要见 verification.json。

前三份为 K035、0°、320×480，每页一个普通控件 ID1；文本／按钮为10/10、280×100，滑块／进度条为20/40、280×40。字体ID0，建议字高68，文字ABC／OK。文本事件为空，按钮／进度条down/up为sys0=1／2，滑块另有slide的sys0=3。

| 文件 | 文本／按钮 | 滑块 | 进度条 |
|---|---|---|---|
| ModesPic | sta=2 图片 | sta=2 图片背景、psta=1 图片游标 | sta=1 图片 |
| ModesCrop | sta=0 全屏切图 | sta=0 切图、psta=1 图片游标 | 保持图片 |
| ModesColor | sta=1 单色，图片引用保留 | sta=1/psta=0，五项图片引用保留 | sta=0 单色，图片引用保留 |

图片ID0为全屏彩色网格，1为淡蓝面板，2为淡橙按下态，3为灰条，4为绿条，5为红底黄十字的20×20游标。文本pic=1，按钮pic/pic2=1/2，滑块pic/pic1/pic2=3/4/5，进度条bpic/ppic=3/4。切图时picc/picc2/picc1引用全屏ID0。切回单色的文件仍明确保留这些值，不需要手工清空或再次切模式查看隐藏字段。

请核对：图片模式可看到相应棋盘面板、灰／绿条和红黄游标；切图模式显示全屏色块在控件位置的对应区域；单色模式恢复原生颜色，不受已保留的非活动引用影响。slider/progress的val=40，游标wid/hig保留255自动，进度条方向dez=0；K035游标／圆角外观沿用其原生表现。

ModesX90 为 X543、90°、272×480，采用JPEG，四页均为图片模式。控件宽度改为232，其余位置／高度同上；图片列表有意反向排列，cursor/green/bar/pressed/panel/full对应ID0/1/2/3/4/5，字体仍ID0。请核对方向、图片绑定与游标没有因重排而错位。全屏资源保持当前方向的逻辑尺寸，不因JPEG内部补齐或旋转修改几何。

本批只核对布局、属性、事件、图片引用和上位机编译，不要求运行模拟器或实机。需要查看按钮按下态时可使用软件已有预览功能，不必修改原始文件来强行触发。图片／字库内容显示有问题时请保留具体提示，不通过重新导入修复。

源码在各文件同名工程目录。每份实际执行13条独立作业，输出HMI与统一入口一致。脚本仅使用独立ZI和代码生成的BMP，不读取已有HMI、内部模板或素材。
''', encoding="utf-8")
    print(f"图片模式验收工程已生成：{output}")


if __name__ == "__main__":
    main()

"""生成指针／波形图片背景和二维码中心 LOGO 验收工程，不需要已有素材。"""
import argparse
from datetime import date
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
from prepare_pictures_acceptance import bitmap
from prepare_picture_modes_acceptance import panel


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=root / "target/acceptance-graphics-modes")
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
    specs = [("GraphicPic", "TJC4832K035_011", 0, 320, 480, 2),
             ("GraphicCrop", "TJC4832K035_011", 0, 320, 480, 0),
             ("GraphicColor", "TJC4832K035_011", 0, 320, 480, 1),
             ("GraphicX90", "TJC4827X543_011", 90, 272, 480, 3)]
    for name, profile, orientation, width, height, mode in specs:
        project = output / name
        run("hmi", "init", project, "--target", profile, "--orientation", orientation)
        assets = project / "assets"
        assets.mkdir()
        # 图片尺寸显式匹配验收控件；编码器不因控件几何隐式调整共享图片。
        inputs = {"full": bitmap(width, height), "dial": panel(160, 160, (190, 220, 250)),
                  "wave": panel(200, 100, (250, 195, 135)), "logo": panel(24, 24, (230, 30, 30), True)}
        for key, data in inputs.items():
            (assets / f"{key}.bmp").write_bytes(data)
            (assets / f"{key}.asset.toml").write_text(f'schema="hmi.asset/1"\nkind="image"\nsource="{key}.bmp"\n', encoding="utf-8")
        keys = list(inputs)
        if orientation == 90:
            keys.reverse()
        pages = [("gauge", "gauge", "z0"), ("wave", "waveform", "s0"), ("qrcode", "qrcode", "qr0")]
        sources = ",".join('"pages/' + page + '/page.toml"' for page, _, _ in pages)
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
sources=[{sources}]
[assets]
items=[{items}]
''', encoding="utf-8")
        qr_mode = int(mode != 1)
        for page, kind, control in pages:
            directory = project / "pages" / page
            directory.mkdir()
            text = f'schema="hmi.page/1"\nname="{page}"\n'
            if mode == 3:
                text += 'properties={sta=2,pic={ref="full"}}\n'
            geometry = 'x=20,y=40,w=200,h=100' if page == "wave" else 'x=20,y=40,w=160,h=160'
            if page == "gauge":
                props = f'{geometry},sta={mode},pic={{ref="dial"}},picc={{ref="full"}},val=135,down=-10'
            elif page == "wave":
                props = f'{geometry},sta={mode},pic={{ref="wave"}},picc={{ref="full"}},ch=2,dir=1,dis=200'
            else:
                props = f'{geometry},sta={qr_mode},pic={{ref="logo"}},txt="HMI-LOGO",txt_maxl=16'
            text += f'[[objects]]\nname="{control}"\ntype="{kind}"\nproperties={{{props}}}\nevents={{down="down.s",up="up.s"}}\n'
            (directory / "page.toml").write_text(text, encoding="utf-8")
            (directory / "down.s").write_text("sys0=1\n", encoding="utf-8")
            (directory / "up.s").write_text("sys0=2\n", encoding="utf-8")
        run("hmi", "check", project, "-Werror")
        assert not (project / ".build").exists()
        hmi = output / f"{name}.HMI"
        run("hmi", "build", project, "-o", hmi, "-Werror")
        plan = project / ".build/manual"
        run("hmi", "plan", project, "-o", plan)
        jobs = json.loads((plan / "jobs.json").read_bytes())["jobs"]
        assert len(jobs) == 9
        for job in jobs:
            run(job["command"], *job["args"])
        assert hmi.read_bytes() == (project / f"dist/{name}/{name}.HMI").read_bytes()
        run("hmi-link", plan / "link.toml", "--check", "-Werror")
        for index, key in enumerate(["dial", "wave", "logo"]):
            inspected = run("hmi-compile", "inspect", plan / f"obj/p{index:05}.hmo")["objects"][1]["properties"]
            assert inspected["pic"] == {"ref": key}
            assert inspected["sta"] == (qr_mode if index == 2 else mode)
        cases.append({"file": hmi.name, "sha256": hashlib.sha256(hmi.read_bytes()).hexdigest(),
                      "profile": profile, "orientation": orientation, "page_size": [width, height],
                      "background_mode": mode, "qr_logo": qr_mode, "image_ids": {key: keys.index(key) for key in keys},
                      "standalone_jobs": len(jobs), "standalone_equals_driver": True,
                      "opened": None, "compiled": None, "content_verified": None})
    (output / "verification.json").write_text(json.dumps({"generated_on": date.today().isoformat(),
        "stage": "gauge-waveform-backgrounds-and-qr-logo", "automatic_checks_passed": True,
        "cases": cases}, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    (output / "README.md").write_text('''# 指针、波形背景与二维码 LOGO 验收

请打开四份 HMI，逐页核对属性及预览，再分别编译。每份均含 gauge、wave、qrcode 三页，每页一个普通控件 ID1，页面自身 ID0；无字体、FFmpeg 或虚拟 SD 依赖。需要保存时请另存副本，原始摘要见 verification.json。

| 文件 | 配置 | 指针／波形 | 二维码 |
|---|---|---|---|
| GraphicPic.HMI | K035／0°／320×480 | sta2 图片 | sta1 有 LOGO |
| GraphicCrop.HMI | 同上 | sta0 全屏切图 | sta1 有 LOGO |
| GraphicColor.HMI | 同上 | sta1 单色，图片引用保留 | sta0 无 LOGO，pic 保留 |
| GraphicX90.HMI | X543／90°／272×480 | sta3 透明，页面使用全屏图片 | sta1 有 LOGO |

几何均从20/40开始：指针和二维码160×160，波形200×100。指针z0为val135、尾长down=-10；原有中心圆及形状参数保持。波形s0为ch2、dir1、dis200，无预置曲线；本批只核对背景。二维码qr0文本HMI-LOGO、容量16、默认白底黑码。普通控件down/up事件sys0=1/2，页面事件为空。

前三份图片ID为full0（全屏彩色网格）、dial1（160×160淡蓝棋盘）、wave2（200×100淡橙棋盘）、logo3（24×24红底黄十字）。指针pic1/picc0，波形pic2/picc0，二维码pic3。图片模式显示对应棋盘背景，切图显示全屏网格对应区域，单色显示各自默认颜色，二维码只在sta1时显示中心十字。无需切换模式查看隐藏字段。

GraphicX90图片ID反向：logo0、wave1、dial2、full3。三页页面背景sta2/pic3；指针pic2/picc3、波形pic1/picc3，均sta3，背景应透出页面网格，保留的小背景图不应填充。二维码sta1/pic0，仍白底黑码及中心十字。请同时核对方向和图片绑定。

每份实际执行9条独立作业，输出与统一入口逐字节相同。图片均由标准库生成，不读取原有HMI或素材，源码和独立计划在同名子目录。本批不要求动态波形、扫码或硬件运行测试。
''', encoding="utf-8")
    print(output)


if __name__ == "__main__":
    main()

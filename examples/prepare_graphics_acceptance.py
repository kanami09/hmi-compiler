"""生成指针、静态波形和无中心 LOGO 二维码验收工程，不需要媒体素材。"""

import argparse
from datetime import date
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=root / "target/acceptance-graphics")
    parser.add_argument("--binaries", type=Path, default=root / "target/debug")
    args = parser.parse_args()
    output, binaries = args.output.resolve(), args.binaries.resolve()
    if output.exists():
        parser.error("输出目录已存在，请使用 --output 指定新目录")
    suffix = ".exe" if os.name == "nt" else ""
    tools = {"hmi", "hmi-compile", "hmi-link"}
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
    for tag, profile, width, height in [
        ("K035", "TJC4832K035_011", 320, 480), ("X543", "TJC4827X543_011", 480, 272)
    ]:
        project = output / f"project-{tag}"
        run("hmi", "init", project, "--target", profile)
        seed = project / "pages/page0/page.toml"
        seed.unlink()
        seed.parent.rmdir()
        pages = ["gauge", "wave", "qrcode", "qrcode2"]
        sources = ",".join(f'"pages/{name}/page.toml"' for name in pages)
        (project / "hmi.toml").write_text(f'''schema="hmi.project/1"
name="Graphics{tag}"
[target]
profile="{profile}"
format="ver21234"
encoding="utf-8"
orientation=0
[program]
source="program.s"
[pages]
sources=[{sources}]
''', encoding="utf-8")
        for name in pages:
            (project / "pages" / name).mkdir(parents=True)
        (project / "pages/gauge/page.toml").write_text('''schema="hmi.page/1"
name="gauge"
[[objects]]
name="z0"
type="gauge"
properties={x=20,y=20,w=160,h=160,val=45}
events={up="z0.up.s"}
[[objects]]
name="z1"
type="gauge"
properties={x=180,y=20,w=120,h=120,val=135,down=-10,hig=0,pco=63488}
''', encoding="utf-8")
        (project / "pages/gauge/z0.up.s").write_text("z0.val=135\n", encoding="utf-8")
        (project / "pages/wave/page.toml").write_text('''schema="hmi.page/1"
name="wave"
[[objects]]
name="s0"
type="waveform"
properties={x=20,y=20,w=280,h=100,ch=1}
events={down="s0.down.s",up="s0.up.s"}
[[objects]]
name="s1"
type="waveform"
properties={x=20,y=140,w=280,h=100,ch=4,dir=1,dis=200,gdw=20,gdh=25}
''', encoding="utf-8")
        (project / "pages/wave/s0.down.s").write_text("sys0=1\n", encoding="utf-8")
        (project / "pages/wave/s0.up.s").write_text("s0.dis=200\n", encoding="utf-8")
        for name, text in [("qrcode", "HMI-A"), ("qrcode2", "HMI-BB")]:
            (project / f"pages/{name}/page.toml").write_text(f'''schema="hmi.page/1"
name="{name}"
[[objects]]
name="qr0"
type="qrcode"
properties={{x=60,y=40,w=200,h=200,txt="{text}",txt_maxl=16}}
events={{up="qr0.up.s"}}
''', encoding="utf-8")
            target_page = "qrcode2" if name == "qrcode" else "gauge"
            (project / f"pages/{name}/qr0.up.s").write_text(f"page {target_page}\n", encoding="utf-8")
        run("hmi", "check", project, "-Werror")
        assert not (project / ".build").exists()
        hmi = output / f"Graphics{tag}.HMI"
        run("hmi", "build", project, "-o", hmi, "-Werror")
        plan = project / ".build/manual"
        run("hmi", "plan", project, "-o", plan)
        jobs = json.loads((plan / "jobs.json").read_bytes())["jobs"]
        assert len(jobs) == 6 and all(job["command"] != "hmi-media" for job in jobs)
        for job in jobs:
            run(job["command"], *job["args"])
        manual = project / f"dist/Graphics{tag}/Graphics{tag}.HMI"
        assert hmi.read_bytes() == manual.read_bytes()
        run("hmi-link", plan / "link.toml", "--check", "-Werror")
        gauge = run("hmi-compile", "inspect", plan / "obj/p00000.hmo")
        assert gauge["objects"][2]["properties"]["down"] == -10
        for page_id, text in [(2, "HMI-A"), (3, "HMI-BB")]:
            inspected = run("hmi-compile", "inspect", plan / f"obj/p{page_id:05}.hmo")
            assert inspected["objects"][1]["properties"]["txt"] == text
        cases.append({"file": hmi.name, "sha256": hashlib.sha256(hmi.read_bytes()).hexdigest(),
                      "profile": profile, "orientation": 0, "page_size": [width, height], "pages": pages,
                      "media_assets": 0, "standalone_jobs": len(jobs), "standalone_equals_driver": True,
                      "opened": None, "compiled": None, "content_verified": None})
    (output / "verification.json").write_text(json.dumps({"generated_on": date.today().isoformat(),
        "stage": "gauge-waveform-qrcode", "automatic_checks_passed": True, "cases": cases},
        ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    (output / "README.md").write_text('''# 指针、波形和二维码验收

请用 USART HMI 打开 GraphicsK035.HMI、GraphicsX543.HMI，逐页核对后分别编译。需要保存时请另存副本，原始摘要在 verification.json。

| 文件 | 配置 |
|---|---|
| GraphicsK035.HMI | TJC4832K035_011，0°，320×480 |
| GraphicsX543.HMI | TJC4827X543_011，0°，480×272 |

两份均含 gauge、wave、qrcode、qrcode2 四页、六个普通控件；无字库、图片或其他媒体。

## gauge（页面 ID 0）

z0（ID 1）位于 20/20，160×160，val=45，白底黑指针、红色中心圆。up=32767 自动尖长、down=30、中心直径 hig=24，尖宽／腰宽／尾宽分别为 1／7／2。

z1（ID 2）位于 180/20，120×120，val=135、红色指针；down=-10，hig=0，无中心圆。重点核对负尾长能否保存和编译，及两个不同角度的预览。控件只画指针，没有自动生成的表盘背景。

z0.up 为 z0.val=135；z1 事件为空。

## wave（页面 ID 1）

两个波形均为单色黑底、网格颜色 gdc=1024，没有预置曲线数据。

| 控件／ID | 位置、尺寸 | 关键属性 |
|---|---|---|
| s0／1 | 20/20，280×100 | ch=1、dir=0、dis=100，gdw=gdh=40 |
| s1／2 | 20/140，280×100 | ch=4、dir=1、dis=200，gdw=20、gdh=25 |

四个通道颜色字段均保留，pco0/pco1/pco2/pco3 为 64495／65519／63488／2016。s0.down 为 sys0=1，s0.up 为 s0.dis=200；s1 事件为空。静态预览应为网格，不要求出现曲线。

## qrcode、qrcode2（页面 ID 2、3）

每页只有 qr0（ID 1），均位于 60/40、200×200，白底黑码、sta=0、无中心 LOGO，容量 txt_maxl=16。

- qrcode：txt 为 HMI-A，up 为 page qrcode2。
- qrcode2：txt 为 HMI-BB，up 为 page gauge。

核对二维码预览和 txt 属性，两页内容应分别保持上述值。本批核对静态预览和编译；扫码内容、动态绘图及硬件运行另行验证。

请反馈两份能否正常打开、属性和预览是否符合说明，以及编译结果和具体提示。源码在 project-K035／project-X543，每份实际执行 6 条独立工具命令，输出与统一入口相同，独立链接检查通过。
''', encoding="utf-8")
    print(f"指针、波形和二维码验收工程已生成：{output}")


if __name__ == "__main__":
    main()

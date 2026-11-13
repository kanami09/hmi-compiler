"""生成 X543 数据记录验收工程，不访问虚拟 SD 或生成 .data 文件。"""
import argparse
from datetime import date
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--font", type=Path, required=True, help="UTF-8 ZI 字库，建议字高 68、覆盖 ASCII")
    parser.add_argument("--output", type=Path, default=root / "target/acceptance-data-record")
    parser.add_argument("--binaries", type=Path, default=root / "target/debug")
    args = parser.parse_args()
    output, binaries, font = args.output.resolve(), args.binaries.resolve(), args.font.resolve()
    if output.exists():
        parser.error("输出目录已存在，请使用 --output 指定新目录")
    if not font.is_file():
        parser.error("请提供已有的独立 ZI 字库")
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
    for variant, length, count, columns, widths, titles in [
        (0, 32, 16, 2, "200^200", "T^V"),
        (1, 63, 1000, 3, "100^150^150", "N^T^V"),
    ]:
        project = output / f"project-{variant}"
        run("hmi", "init", project, "--target", "TJC4827X543_011")
        assets = project / "assets"
        assets.mkdir()
        shutil.copyfile(font, assets / "ui.zi")
        (assets / "ui.asset.toml").write_text('schema="hmi.asset/1"\nkind="font"\nsource="ui.zi"\n', encoding="utf-8")
        (project / "hmi.toml").write_text(f'''schema="hmi.project/1"
name="Record{variant}"
[target]
profile="TJC4827X543_011"
format="ver21234"
encoding="utf-8"
orientation=0
[program]
source="program.s"
[pages]
sources=["pages/page0/page.toml"]
[assets]
items=[{{key="ui",source="assets/ui.asset.toml"}}]
''', encoding="utf-8")
        runtime_path = f"sd0/hmic_record{variant}.data"
        (project / "pages/page0/page.toml").write_text(f'''schema="hmi.page/1"
name="records"
[[objects]]
name="data0"
type="data_record"
properties={{x=20,y=20,w=440,h=232,font={{ref="ui"}},path="{runtime_path}",lenth={length},maxval={count},dez={columns},format="{widths}",dir="{titles}",order={variant}}}
events={{up="data0.up.s"}}
''', encoding="utf-8")
        (project / "pages/page0/data0.up.s").write_text("sys0=data0.val\n", encoding="utf-8")
        run("hmi", "check", project, "-Werror")
        assert not (project / ".build").exists()
        hmi = output / f"Record{variant}.HMI"
        run("hmi", "build", project, "-o", hmi, "-Werror")
        map_path = output / f"Record{variant}.link-map.json"
        shutil.copyfile(output / "link-map.json", map_path)
        driver_map = json.loads(map_path.read_bytes())
        plan = project / ".build/manual"
        run("hmi", "plan", project, "-o", plan)
        jobs = json.loads((plan / "jobs.json").read_bytes())["jobs"]
        manual_map = project / "standalone-map.json"
        for job in jobs:
            extra = ["--map", manual_map, "-Werror"] if job["command"] == "hmi-link" else []
            run(job["command"], *job["args"], *extra)
        manual = project / f"dist/Record{variant}/Record{variant}.HMI"
        assert hmi.read_bytes() == manual.read_bytes()
        assert driver_map["runtime_files"] == json.loads(manual_map.read_bytes())["runtime_files"]
        run("hmi-link", plan / "link.toml", "--check", "-Werror")
        inspection = run("hmi-compile", "inspect", plan / "obj/p00000.hmo")
        structure = {"record_bytes": length, "max_records": count, "columns": columns}
        dependency = inspection["metadata"]["runtime_dependencies"][0]
        assert dependency == {"object": 1, "file": {"kind": "data_record", "path": runtime_path,
                                                   "structure": structure, "create_if_missing": True}}
        assert inspection["objects"][1]["properties"]["font"] == {"ref": "ui"}
        assert driver_map["runtime_files"][0]["generated"] is False
        assert not (output / "sd0").exists() and not (project / "sd0").exists()
        cases.append({"file": hmi.name, "sha256": hashlib.sha256(hmi.read_bytes()).hexdigest(),
                      "profile": "TJC4827X543_011", "orientation": 0, "page_size": [480, 272],
                      "runtime_path": runtime_path, "structure": structure, "order": variant,
                      "format": widths, "dir": titles, "data_file_generated": False,
                      "standalone_jobs": len(jobs), "standalone_equals_driver": True,
                      "opened": None, "compiled": None, "content_verified": None})
    (output / "verification.json").write_text(json.dumps({"generated_on": date.today().isoformat(),
        "stage": "data-record-runtime-dependencies", "automatic_checks_passed": True, "cases": cases},
        ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    (output / "README.md").write_text('''# 数据记录控件验收

请用 USART HMI 打开 Record0.HMI、Record1.HMI，核对属性、表头和编译结果。需要保存时请另存副本，原始摘要见 verification.json。

两份均为 TJC4827X543_011、0°、480×272，一页 records（页面 ID 0），一个数据记录 data0（控件 ID 1）。字体 ID 0；建议使用高 68、覆盖 ASCII 的字库。data0 位于 20/20、440×232，强制全局，单色白底黑字、style=2、边框颜色 0／宽度 2，行高 hig=72。

| 文件 | path | lenth | maxval | dez | format | dir | order |
|---|---|---|---|---|---|---|---|
| Record0.HMI | sd0/hmic_record0.data | 32 | 16 | 2 | 200^200 | T^V | 0，新数据在前 |
| Record1.HMI | sd0/hmic_record1.data | 63 | 1000 | 3 | 100^150^150 | N^T^V | 1，新数据在后 |

mode=1、dis=1、xcen=0、left=1、ch=8、spax=0；gdc=50712、gdw=gdh=1；表头背景 bco1=23765、文字 pco1=65535，选中项背景 bco2=31、文字 pco2=65535。up 事件只读取选中 ID：sys0=data0.val。qty/txt 和滚动上限由原生环境处理，不要求显示固定记录数或预填数据。

本工具没有生成、读取或修改 .data 文件，也没有访问虚拟 SD。mode=1 允许原生环境在缺少对应文件时自动创建；打开工程或预览时，上位机可能访问这些路径。若路径下已有不匹配的文件，请保留提示，不必删除原有数据；可以反馈后改用新的实验路径。不要手工创建零字节 .data 文件。

Record0.link-map.json／Record1.link-map.json 的 runtime_files 记录初始路径、结构参数和使用者，generated=false 表示未生成配套文件。每份实际执行 4 条独立作业，输出和需求映射均与统一入口一致。页面目标保存文件需求，独立链接不需要读取源码或数据文件。

本批验收控件配置和 HMI 编译，不要求插入／删除记录、启动模拟器或连接硬件。若上位机自动生成了 .data 文件，可先保留，后续单独核对文件结构。请反馈两份是否正常打开、表头和属性是否符合说明，以及编译结果和提示。
''', encoding="utf-8")
    print(f"数据记录验收工程已生成：{output}")


if __name__ == "__main__":
    main()

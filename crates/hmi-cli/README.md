# 新工具链：第一阶段

当前可从零新建空白工程，编译页面自身事件和全局代码，再链接为 HMI。构建过程不需要已有 HMI、逆向资料或媒体工具。

当前支持的型号和方向见根目录 `data/profiles/README.md`。暂未接入普通控件、媒体资产、新格式导入、原始数据附件和增量缓存。`build` 与 `rebuild` 在此阶段都执行完整构建。原有 `hmi-compiler` 入口保留，迁移期间继续运行原有回归测试。

## 构建与使用

在仓库根目录执行：

```powershell
cargo build --workspace --bins --locked

# 使用显式型号创建空白工程。
.\target\debug\hmi.exe init panel --target TJC4827X543_011
.\target\debug\hmi.exe check panel
.\target\debug\hmi.exe build panel
```

默认输出为 `panel/dist/Panel/Panel.HMI`，同目录的 `link-map.json` 记录页面 ID 和资源位置。可以用 `-o` 指定 HMI 输出路径；命令行路径相对于当前目录，`hmi.toml` 中的路径相对于工程目录。

`--json` 输出结构化结果，`-Werror` 将检查产生的警告作为错误。检查失败不会覆盖原有 HMI。`check` 不创建构建目录。

## 工程文件

```toml
schema = "hmi.project/1"
name = "Panel"

[target]
profile = "TJC4827X543_011"
format = "ver21234"
encoding = "utf-8"
orientation = 0

[program]
source = "program.s"

[pages]
sources = ["pages/page0/page.toml"]

[assets]
items = []
```

页面源码声明名称、单色背景和事件脚本：

```toml
schema = "hmi.page/1"
name = "page0"

[properties]
sta = 1
bco = 65535

[events]
load = "events/load.s"
```

事件路径相对于页面目录。当前事件槽包括 load、loadend、down、up、unload；每个引用文件都必须存在。未引用的事件为空，代码中间的空行会保留。页面名称暂使用 1～16 字节的字母、数字、下划线，以字母或下划线开头。

页面 ID 来自 sources 数组的顺序，页面自身对象的 ID 始终为 0。页面重排不会自动改写脚本，引用按最终页面名称和 ID 检查。当前静态分析检查已支持的引用形式，不代替 USART HMI 的完整脚本编译。

## 独立工具

```powershell
.\target\debug\hmi.exe plan panel -o panel/.build/manual
```

计划目录包含 `target.json`、`link.toml` 和 `jobs.json`，不会预先生成目标文件。jobs.json 的每项包含 command 和 args，可按顺序调用相应程序。参数是数组，无需自行拼接 shell 命令。

- `hmi-compile program` 编译全局代码。
- `hmi-compile page` 编译单个页面。
- `hmi-compile inspect` 检查 `.hmo` 描述、版本和摘要。
- `hmi-link` 仅使用链接描述、target.json 和 `.hmo`，不依赖源码。
- `hmi-link --check` 在内存中生成并校验 HMI，不写输出文件。

统一入口在 `.build/hmi-toolchain` 保存相同的目标文件和链接描述，可用独立链接器再次链接。此目录尚未用作增量缓存，也未提供多进程构建调度。

## 验证

```powershell
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
```

自动测试包括固定原生输出摘要、四组初始配置、独立链接、页面重排、事件空行、错误时保留输出、输入文件保护和 JSON 结果。上位机打开、保存、编译需要对本阶段生成的工程单独验收。

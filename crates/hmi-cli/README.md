# 新工具链：基础控件

当前可从零新建工程，添加热区、变量和定时器，编译页面、控件事件和全局代码，再链接为 HMI。构建过程不需要已有 HMI、逆向资料或媒体工具。

当前支持的型号和方向见根目录 `data/profiles/README.md`。其他控件、媒体资产、新格式导入、原始数据附件和增量缓存尚未接入。`build` 与 `rebuild` 在此阶段都执行完整构建。原有 `hmi-compiler` 入口保留，迁移期间继续运行原有回归测试。

四个内置型号均可通过 `init --orientation 0/90/180/270` 选择工程方向。新建页面会使用该方向对应的尺寸；方向字段和页面几何分别写入 HMI。

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

## 控件

控件写在页面文件的 objects 数组中。数组顺序决定图层和最终控件 ID，普通控件从 1 开始编号。增加、删除或重排条目后重新构建即可，不需要手动维护 id。

```toml
[[objects]]
name = "counter"
type = "variable"
properties = { sta = 0, val = -7, vscope = 1 }

[[objects]]
name = "tap"
type = "hotspot"
properties = { x = 20, y = 30, w = 120, h = 60 }
events = { up = "events/tap.up.s" }

[[objects]]
name = "tm0"
type = "timer"
properties = { tim = 400, en = 0 }
events = { timer = "events/tm0.timer.s" }
```

| 类型 | 当前可编辑属性 | 事件 |
|---|---|---|
| hotspot（热区） | x/y/w/h 必填；位置为 0～65535，尺寸为 1～65535，端点不能超过 65535；vscope 默认 0，sendkey 默认 0 | down、up |
| variable（变量） | sta 默认 0：0 数值、1 字符串；val 默认为 0，范围为 i32；txt 默认为 newtxt，txt_maxl 默认为 10；vscope 默认 0 | 无 |
| timer（定时器） | tim 默认 400，范围为 50～65534 毫秒；en 默认 1；vscope 默认 0 | timer |

vscope 和 en 使用整数 0／1。坐标、尺寸、字体、图片等属性不会自动加到没有这些字段的控件上；例如变量和定时器不接受 x/y/w/h。type、id、objname 和几何端点由工具生成，不作为 properties 输入。

变量无论当前模式如何，始终保存 sta、val、txt 和 txt_maxl；仅修改 sta 不删除另一种模式的字段。txt_maxl 按 UTF-8 字节计算。脚本中的数据属性随模式变化：数值模式使用 val，字符串模式使用 txt 和 txt_maxl；持久化保留字段不等于当前模式允许访问该数据属性。

页面名称全工程唯一，控件名称在所属页内唯一，并且不能与页面自身同名。名称只按当前工程解释：把一个控件改名为 confirm，再把另一个控件命名为 tap，脚本里的 tap 就引用当前名为 tap 的控件，不会自动改绑到 confirm。数字 ID 同样按最终排列解释。

静态确定的缺失页面、控件、属性、变量模式不符和跨页私有访问会阻止链接。动态 ID、无法确认的跳转表达式仍为警告，可用 -Werror 升级。事件诊断会记录实际所属的页面、控件、事件槽和脚本行号。

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

当前 `.hmo` 使用 `hmi.object/2`，保存对象记录、脚本分析结果和 ID 重定位表。编译阶段的控件 ID 为零占位值，链接阶段才分配最终 ID，并生成页面头、索引和 CRC。更新到此阶段后应重新构建工程，旧 `.hmo` 和 target.json 不可继续使用；这些格式／数据版本与分发版本号分别管理。

`hmi-compile inspect` 会按注册类型显示属性，包括变量的有符号 val、UTF-8 文本和容量。普通控件显示 id_pending=true，表示 ID 尚未由链接器填写；最终 ID 以 HMI 或 link-map.json 为准。

## 验证

```powershell
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
```

自动测试包括固定原生输出摘要、型号与方向、独立链接、控件增删改名重排、变量模式和字节容量、作用域、事件归属、损坏的重定位数据、错误时保留输出、输入文件保护和 JSON 结果。上位机打开、保存、编译需要对本阶段生成的工程单独验收。

需要生成基础控件的验收工程时，在仓库根目录执行：

```powershell
cargo build --workspace --bins --locked
python examples/prepare_controls_acceptance.py
```

结果位于 `target/acceptance-basic-controls/`，该目录的 README.md 列出页面、控件、事件及核对步骤。脚本使用 Python 标准库，不读取已有 HMI 或内部资料；已有验收目录不会覆盖，重新生成时可用 `--output` 指定新目录。

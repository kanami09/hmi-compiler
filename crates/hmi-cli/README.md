# 新工具链使用说明

当前可从零新建工程，添加热区、变量、定时器、单色文本和按钮，以及 K/T 配置下的图片和切图；支持 ZI 字库方向适配，编译页面、控件事件和全局代码，再链接为 HMI。构建过程不需要已有 HMI 或逆向资料。

当前支持的型号和方向见根目录 `data/profiles/README.md`。X 系列图片编码、其他控件和媒体种类、新格式导入、原始数据附件和增量缓存尚未接入。`build` 与 `rebuild` 在此阶段都执行完整构建。原有 `hmi-compiler` 入口保留，迁移期间继续运行原有回归测试。

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

控件写在页面文件的 objects 数组中。数组顺序决定图层和最终控件 ID，普通控件从 1 开始编号。增加、删除或重排条目后重建即可，不需要手动维护 id。

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
| picture（图片） | x/y/w/h 必填；pic 为图片引用表；vscope、sendkey 默认 0 | down、up |
| crop（切图） | x/y/w/h 必填；picc 须引用与当前页面同尺寸的全屏图片，取图区域不得越界；vscope、sendkey 默认 0 | down、up |
| text（文本） | x/y/w/h、font 引用表、txt 必填；当前 sta=1；style 为 0 平面或 1 边框 | down、up |
| button（按钮） | x/y/w/h、font 引用表、txt 必填；当前 sta=1；style 为 1 边框或 4 3D_Auto，默认 4 | down、up |

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
- `hmi-media probe/encode/inspect` 探测、编码和检查媒体资产，详见[媒体编码器](../hmi-media/README.md)。
- `hmi-link` 仅使用链接描述、target.json、`.hmo` 和 `.hma`，不依赖源码。
- `hmi-link --check` 在内存中生成并校验 HMI，不写输出文件。

统一入口在 `.build/hmi-toolchain` 保存相同的目标文件和链接描述，可用独立链接器再次链接。此目录尚未用作增量缓存，也未提供多进程构建调度。

当前 `.hmo` 使用 `hmi.object/2`，保存对象记录、脚本分析结果和 ID 重定位表。编译阶段的控件 ID 为零占位值，链接阶段才分配最终 ID，并生成页面头、索引和 CRC。更新到此阶段后应重建工程，旧 `.hmo` 和 target.json 不可继续使用；这些格式／数据版本与分发版本号分别管理。

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

## 图片资产与引用

工程清单使用有序的资产条目：

```toml
[assets]
items = [
    { key = "background", source = "assets/background.asset.toml" },
    { key = "logo", source = "assets/logo.asset.toml" },
]
```

每个描述指向原图及编码参数；单个资产生成一个 `.hma`，其中运行图片和编辑源只占一个逻辑图片 ID。未引用资产仍保留，不根据内容相同自动合并。输入按文件签名识别，BMP、PNG、JPG 都可用于当前 K/T RGB565 路线。

控件属性使用 `pic = { ref = "logo" }` 或 `picc = { ref = "background" }`。也可显式写 `{ id = 0 }` 表示最终图片列表中的第 0 项；不能直接写裸整数，图片和切图不接受空图片。

资源符号按当前清单解析。重排资产时，ref 仍绑定当前同名资产，id 则继续表示最终数值位置；脚本不自动改写。资源 ID 在链接时填写，编译器不读取原图以补全控件属性。

图片编码尺寸由资产参数决定，控件 w/h 不会隐式缩放共享资源。当前不推断普通图片控件在所有型号上的显示缩放行为；需要两份不同编码尺寸时，分别声明资产。切图使用全屏资源的对应区域，不是把整图缩小。

`check` 使用图片探测信息检查最终引用、几何和脚本，不生成 `.hma` 或 HMI；`build` 实际编码，并在输入与探测结果一致后完成链接。计划增加 hmi-media 作业，独立链接可以在删除原图后执行。

生成 0°／90° 图片验收工程：

```powershell
cargo build --workspace --bins --locked
python examples/prepare_pictures_acceptance.py
```

结果在 `target/acceptance-pictures/`。脚本生成标准 BMP 和透明 PNG，验证去透明、缩放、图片与切图引用，并实际执行导出的独立命令比较输出；不会覆盖已有验收目录。

## 字库、文本和按钮

字库描述示例：

```toml
schema = "hmi.asset/1"
kind = "font"
source = "fonts/ui.zi"
```

将该描述加入工程的 assets.items，例如 `{ key = "ui_font", source = "assets/ui.asset.toml" }`。字库与图片可以交错排列，各自按相对顺序分配 ID；两份内容相同的显式资产也不会自动合并。输入为受支持的 UTF-8 ZI v6，不从 TTF 自动制字，方向由编码器内部适配。

```toml
[[objects]]
name = "title"
type = "text"
properties = { x = 10, y = 10, w = 280, h = 100, font = { ref = "ui_font" }, txt = "ABC012" }

[[objects]]
name = "confirm"
type = "button"
properties = { x = 10, y = 160, w = 160, h = 60, font = { id = 0 }, txt = "ABC", style = 1 }
events = { up = "events/confirm.up.s" }
```

font 必须使用 `{ ref = "..." }` 或 `{ id = ... }`。ref 引用当前资产符号，id 指最终字体列表的位置，范围为 0～254；不能写裸整数、none 或保留值 255。引用按资源类别检查，不会把图片 ID 当作字体 ID。字体由多个控件共享，控件尺寸不会改变字高。

文本和按钮都检查 txt_maxl 的 UTF-8 字节容量，默认 10。例如“测试A”占 7 字节，需要容量至少为 7。工具目前不检查每个字符是否存在字形，UTF-8 字库也可能只含少量字符；请按字库覆盖范围设置文字。

本批仅开放单色背景 sta=1。文本的 style 默认 0；按钮默认 4。即使 UI 隐藏图片等字段，输出仍保留原生布局和非活动值，但不能通过源码设置尚未开放的背景模式。

| 公共属性 | 默认值与范围 |
|---|---|
| vscope、sendkey | 默认 0，可设 0／1 |
| bco、pco | RGB565 颜色值，0～65535；文本默认白底黑字，按钮普通状态 bco=50712、pco=0 |
| borderc、borderw | 默认 0、2，范围分别为 0～65535、0～255 |
| xcen、ycen | 默认 1；0 靠左／靠上、1 居中、2 靠右／靠下 |
| txt_maxl | 默认 10，范围 0～65535，单位为 UTF-8 字节 |
| isbr、spax、spay | 默认均为 0；isbr 为 0／1，spax、spay 为 0～255 |

文本另有 pw，默认 0，可设 0／1。按钮另有按下状态颜色 bco2=1024、pco2=65535，颜色范围同上；val 为 0／1，默认 0。文本的 key 和所有非活动图片字段暂由原生配方生成，不作为可编辑属性。

生成字库／文本验收工程时，提供两份包含 ABC012 的独立字库，建议字高分别为 24 和 68：

```powershell
cargo build --workspace --bins --locked
python examples/prepare_fonts_acceptance.py --font-small path/to/small.zi --font-large path/to/large.zi
```

脚本不附带字库，也不读取已有 HMI。它复制输入 ZI 作为验收工程素材，生成 T035 的 0° 和 K035 的 90° 工程，交换字体顺序以核对引用，检查文本／按钮的属性和事件，并实际执行独立工具计划比较输出。结果和核对说明位于 `target/acceptance-fonts/`；已有目录不会覆盖，可另用 --output。命令行字库路径仅用于定位文件，工程内的 source 指向复制后的独立素材。

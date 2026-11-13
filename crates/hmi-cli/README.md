# 新工具链使用说明

当前可从零新建工程，使用下表列出的 16 类控件，编译页面、控件事件和全局代码，再链接为 HMI。媒体支持 ZI 字库方向适配，以及 K/T 配置下的 RGB565 图片。构建过程不需要已有 HMI 或逆向资料。

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
| number（数字） | x/y/w/h、font 引用表必填；val 为 i32；当前 sta=1，显示位数使用原生字段 lenth | down、up |
| xfloat（虚拟浮点数） | x/y/w/h、font 引用表必填；val 为 i32，vvs0/vvs1 控制整数位／小数位；当前 sta=1 | down、up |
| checkbox（复选框） | x/y/w/h 必填；val 为 0／1，可设颜色、style 和边框 | down、up |
| radio（单选框） | x/y/w/h 必填；val 为 0／1，可设颜色；互斥逻辑由脚本实现 | down、up |
| progress_bar（进度条） | x/y/w/h 必填；val 为 0～100；当前 sta=0；dez 为方向字段 | down、up |
| slider（滑块） | x/y/w/h 必填；val/minval/maxval 为 u16；当前仅单色背景及单色游标 | down、up、slide |
| dual_state_button（双态按钮） | x/y/w/h、font 引用表、txt 必填；val 为 0／1，当前 sta=1 | down、up |
| scrolling_text（滚动文本） | x/y/w/h、font 引用表、txt 必填；可设滚动方向、幅度、周期和使能，当前 sta=1 | down、up |
| touch_cap（触摸捕捉） | 仅 vscope 可设置；每页最多一个，无几何属性；val 为只读捕捉结果 | down、up |

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

## 数字、选择与进度控件

```toml
[[objects]]
name = "n0"
type = "number"
properties = { x = 10, y = 10, w = 280, h = 70, font = { ref = "ui_font" }, val = -12 }

[[objects]]
name = "x0"
type = "xfloat"
properties = { x = 10, y = 90, w = 280, h = 70, font = { ref = "ui_font" }, val = -1234, vvs1 = 2 }

[[objects]]
name = "c0"
type = "checkbox"
properties = { x = 20, y = 20, w = 48, h = 48, val = 1 }

[[objects]]
name = "r0"
type = "radio"
properties = { x = 120, y = 20, w = 48, h = 48, val = 0 }

[[objects]]
name = "j0"
type = "progress_bar"
properties = { x = 20, y = 100, w = 280, h = 40, val = 40 }
```

上面各条目独立演示属性，应按实际页面安排位置。数字／虚拟浮点数需要字体引用；复选框、单选框和单色进度条不需要图片或字库。五类均支持 vscope、sendkey，默认 0；事件槽为 down、up。

数字／虚拟浮点数的 val 是有符号 32 位整数，默认 0，范围为 -2147483648～2147483647。xfloat 的 `val=-1234,vvs1=2` 显示为 -12.34；将 vvs1 改为 3 后显示为 -1.234，存储的整数不变。不要将 val 写成 TOML 浮点数，也不应先转换为 IEEE 浮点数位模式。

| 类型 | 格式字段 |
|---|---|
| number | lenth：0～15，默认 0 表示自动位数。官方说明称为 length，本工程格式保留 HMI 的实际拼写 lenth，不接受 length 别名。format：0 数字、1 货币、2 Hex，默认 0 |
| xfloat | vvs0：0～10，默认 0 表示整数位自动；vvs1：0～8，默认 2，设为 0 表示无小数 |

两类当前仅支持单色背景 sta=1；style 为 0～4，默认 0。支持与文本相同的颜色、边框、对齐及字间距属性，但没有 txt、txt_maxl 或 pw。isbr 默认 1。未启用的图片字段仍保留在原生记录中，源码暂不能设置图片背景。

复选框和单选框的 val 默认 0，可设 0／1，表示未选中／选中；默认白底 bco=65535、前景 pco=1024。复选框另有 style（0～4，默认 2）、borderc（默认 0）和 borderw（默认 2）。单选框没有这些边框／风格字段，也不会自动生成互斥脚本；如果需要互斥，应在事件中显式设置其他单选框的 val。

进度条使用独立的模式定义：sta=0 表示单色，图片模式 sta=1 暂未接入。val 默认 0，范围 0～100；dez=0 横向、dez=1 竖向，默认 0。默认背景 bco=50712、前景 pco=1024，颜色范围均为 0～65535。

dis 是进度条的圆角百分比字段，范围 0～100、默认 25；不能据此保证所有型号画出相同形状。此前 K035 画布显示直角，且属性栏未显示 dis；X543 显示圆角。这里保留字段和值，不在 K035 上自动清零；本工具不会模拟圆角或生成替代图片。

生成这一批的验收工程：

```powershell
cargo build --workspace --bins --locked
python examples/prepare_numeric_acceptance.py --font path/to/ui.zi
```

字库须包含数字、负号和小数点，建议字高为 68。脚本复制该独立素材，生成 K035 和 X543 的 0° 工程，各有 numbers、choices、progress 三页，核对整数／小数显示、选择状态，以及两种进度方向。它还检查有符号目标属性和字体引用，并实际执行独立工具计划比较输出。结果及核对说明位于 `target/acceptance-numeric/`，已有目录不会覆盖，可另用 --output。脚本不读取已有 HMI 或内部资料，也不附带字库。

## 滑块、双态按钮、滚动文本和触摸捕捉

```toml
[[objects]]
name = "h0"
type = "slider"
properties = { x = 20, y = 20, w = 280, h = 40, minval = 0, maxval = 100, val = 40 }
events = { slide = "events/h0.slide.s" }

[[objects]]
name = "bt0"
type = "dual_state_button"
properties = { x = 20, y = 80, w = 280, h = 70, font = { ref = "ui_font" }, txt = "ABC", val = 1 }

[[objects]]
name = "g0"
type = "scrolling_text"
properties = { x = 20, y = 160, w = 280, h = 70, font = { ref = "ui_font" }, txt = "SCROLL ABC 123", en = 0 }

[[objects]]
name = "tc0"
type = "touch_cap"
events = { up = "events/tc0.up.s" }
```

四类均支持 vscope，默认 0。滑块、双态按钮、滚动文本还支持 sendkey，默认 0。当前保持单色背景，不接受图片背景、图片游标或透明模式；原生记录中未启用的图片字段仍保留为空值。

滑块的 mode 为 0 横向／1 竖向，默认 0；sta 当前为 1。val 默认为 0，minval/maxval 默认 0/100，三者范围均为 0～65535，且 minval 不能大于 maxval。官方说明中的 minval/maxval 只在滑动时限制 val，不能据此拒绝范围外的显式初始值，工具不会自动夹紧 val。

| 滑块属性 | 默认值与范围 |
|---|---|
| psta | 默认 0；通用范围为 0／1／2，K0 已确认限制为 0／1。1 为图片游标，尚未接入 |
| wid、hig | 默认 255，范围 0～255；255 表示自动，0 表示无游标 |
| dis、ch | 默认 100、25，范围 0～100；分别为圆角比例、背景宽度比例 |
| bco、bco1、pco | 默认 50712、1024、63488，范围 0～65535；分别为背景、前景和游标颜色 |

psta=0 的实际外观有型号差异：此前 K035 预览为方形，X543 为圆形。工具保留相同字段和值，不在 K035 上自动改成其他模式，也不保证编辑器、模拟器和硬件表现完全相同。

K0 的上位机只有 0／1 两种游标选项；X5 另有 2（单色四边形）。型号属性差异使用统一约束表，未记录差异的型号沿用通用范围，不因尚未逐项验证而预先收窄。当前 T0 按通用范围处理，可使用 psta=2；以后确认差异时再增加配置。图片游标仍受编码实现进度限制。

target 的 control_property_constraints 列出已展开的型号例外，支持数值范围和枚举值集合。内置规则见 `data/profiles/control-constraints.json`，具体格式和覆盖顺序见[型号配置说明](../../data/profiles/README.md)。页面编译与独立目标文件检查共用这些规则。

双态按钮的 val 为 0 弹起／1 按下，默认 0；原生双态状态由控件自身处理，不额外生成切换脚本。颜色、字体和容量与普通按钮相同，style 支持 0～4、默认 4。txt 必填，txt_maxl 默认 10，按 UTF-8 字节检查容量。

滚动文本没有普通文本的 pw 字段。txt 必填，txt_maxl 默认 64；style 为 0～4、默认 0；默认左对齐 xcen=0、垂直居中 ycen=1，白底黑字。其余边框、间距、容量规则与文本相同。

| 滚动属性 | 默认值与范围 |
|---|---|
| dir | 默认 1；0 左到右、1 右到左、2 上到下、3 下到上 |
| dis | 默认 4，范围 2～50，单位像素；不同于进度条或滑块的同名属性 |
| tim | 默认 80，范围 80～65534 毫秒；不能套用定时器控件的最小周期 |
| en | 默认 1，0 关闭／1 开启 |

滚动文本的四个 vvs 保留字段由工具初始化为零，未作为可编辑属性。静态画布是否滚动不用于判断 en 是否正确，属性和实际运行需要分别核对。

触摸捕捉没有可见几何；val 是当前捕捉到的控件 ID，初始化为 0，不能在源码 properties 中设置。工具按官方建议限制每页最多一个，独立链接器也会复查数量。脚本可以读取 `tc0.val`，例如 `sys0=tc0.val`；静态确定的直接赋值、复合赋值或自增／自减会报错，包括名称和常量 ID 引用。比较表达式如 `if(tc0.val==1)` 不属于赋值。该检查不代替对所有指令输出参数或方法调用的完整语义分析。

生成交互控件验收工程：

```powershell
cargo build --workspace --bins --locked
python examples/prepare_interaction_acceptance.py --font path/to/ui.zi
```

字库需覆盖示例中的 ASCII 字符，建议字高为 68。脚本生成 K035、X543 的 0° 工程，各含 sliders、buttons、scroll、capture 四页，并实际执行 7 条独立命令比较输出。结果与核对说明在 `target/acceptance-interaction/`；已有目录不会覆盖，可用 --output 指定新目录。脚本只读取独立 ZI，不附带字库、不读取已有 HMI 或内部资料。

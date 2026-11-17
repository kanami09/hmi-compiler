# 新工具链使用说明

当前可从零新建工程，使用下表列出的 30 类控件，编译页面、控件事件和全局代码，再链接为 HMI。具体型号开放的控件以 target 的 control_types 为准。媒体支持图片、ZI 字库、GIF 动画、WAV 音频和有声／无声视频编码。构建过程不需要已有 HMI 或逆向资料。

当前支持的型号和方向见根目录 `data/profiles/README.md`。尚未覆盖的控件属性、未识别媒体变体及非标准几何迁移仍待接入；已有受支持的工程可导入新版格式。`build` 按内容摘要复用页面、全局代码和媒体产物，`rebuild` 执行全量重建。原有 `hmi-compiler` 入口保留，迁移期间继续运行原有回归测试。

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

### 增量构建与清理

```powershell
.\target\debug\hmi.exe build panel     # 复用未变化的编译、编码产物
.\target\debug\hmi.exe rebuild panel   # 忽略派生缓存，执行全量重建
.\target\debug\hmi.exe clean panel     # 清理工具登记的构建产物
```

命令结果显示本次编译、媒体处理和复用的数量。全局代码和各页面分别计数；媒体处理也包括封装 `encoded`／`native` 输入、复用显式 `original` 附件，并不表示一定发生转码。`--json` 的 `result.build` 提供 `compiled`、`reused_objects`、`encoded`、`reused_assets` 四项计数。

缓存位于 `.build/hmi-cache`，按源文件、事件脚本、资产描述、实际媒体输入、原始附件及所需 FFmpeg DLL 的内容摘要检查。文件时间不作为复用依据。缓存损坏或丢失会重新处理对应单元；源文件或声明的附件缺失仍然报错，不用缓存代替输入。更新程序也会使缓存失效，与分发版本号是否改变无关。

修改某页面的属性或事件，只重新编译该页面；修改某媒体，只重新处理该媒体。页面／资产重排、资产符号更名、修改最终输出路径不要求重新编译或编码。方向改变会更新页面和与方向有关的媒体；音频可继续复用。各阶段当前串行执行，尚无并行调度。

每次构建仍执行工程级检查和链接，以当前页面、控件名称、资源顺序和作用域分配最终 ID、解析引用。缓存中的脚本分析结果继续参与检查；改用 `-Werror` 后，原有警告也会阻止发布。`rebuild` 只忽略派生缓存，仍按源码声明读取 `native` 包或复用符合条件的 `original`，不会擅自把它们改为常规媒体转码。

`build`、`rebuild` 和 `clean` 使用同一工程锁。另一进程正在操作该工程时会明确报错；进程退出后锁自动释放，遗留的 `.build/hmi.lock` 文件不需要手工删除。不同工程之间互不阻塞。

`clean` 根据 `.build/hmi-owned.json` 的产物记录清理 `.build/hmi-cache` 和 `.build/hmi-toolchain`，不递归删除整个 `.build`，也不清理 `dist`、源码和 `.preserve` 原始附件。未登记文件及内容已被修改的产物会保留。若某个登记产物后来被声明为工程输入，清理会停止并说明冲突；清理记录损坏、页面或资产描述无法读取时，也会停止。工具会拒绝构建状态目录中的符号链接或目录联接。旧版本没有登记的构建文件不会自动删除。

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

页面源码声明名称、背景和事件脚本：

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

页面背景 sta=0 为无背景、1 为单色、2 为图片，默认 1；bco 是 RGB565 颜色，默认 65535。图片模式必须提供 `pic = { ref = "background" }`，资源必须与当前方向的页面逻辑尺寸完全相同。页面背景的 sta 数值与普通文字控件不同。无背景模式不主动绘制背景，实际显示取决于运行时已有画面。x/y 固定为 0，w/h 取目标画布，不能在页面源码中覆盖；页面不支持 sta=3。页面还可设置effect加载特效，默认0；当前X系列开放0～8，T0／K0限制为0。

pic 的引用、空值和非活动字段保留规则与下文图片背景一致。例如 `properties = { sta = 1, bco = 2016, pic = { ref = "background" } }` 显示绿色单色背景，同时保留图片配置。未写 pic 时为原生空值 65535，不会默认引用图片 ID0。

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
| text（文本） | x/y/w/h、font 引用表、txt 必填；sta 为 0～3，3 仅 X 系列；style 为 0 平面或 1 边框 | down、up |
| button（按钮） | x/y/w/h、font 引用表、txt 必填；sta 为 0～3，3 仅 X 系列；style 为 1 边框或 4 3D_Auto，默认 4 | down、up |
| number（数字） | x/y/w/h、font 引用表必填；val 为 i32；sta 为 0～3，3 仅 X 系列；显示位数使用原生字段 lenth | down、up |
| xfloat（虚拟浮点数） | x/y/w/h、font 引用表必填；val 为 i32，vvs0/vvs1 控制整数位／小数位；sta 为 0～3，3 仅 X 系列 | down、up |
| checkbox（复选框） | x/y/w/h 必填；val 为 0／1，可设颜色、style 和边框 | down、up |
| radio（单选框） | x/y/w/h 必填；val 为 0／1，可设颜色；互斥逻辑由脚本实现 | down、up |
| progress_bar（进度条） | x/y/w/h 必填；val 为 0～100；sta=0 单色、1 图片；dez 为方向字段 | down、up |
| slider（滑块） | x/y/w/h 必填；val/minval/maxval 为 u16；背景支持单色／图片／切图，游标支持单色／图片 | down、up、slide |
| dual_state_button（双态按钮） | x/y/w/h、font 引用表、txt 必填；val 为 0／1；sta 为 0～3，3 仅 X 系列 | down、up |
| scrolling_text（滚动文本） | x/y/w/h、font 引用表、txt 必填；可设滚动方向、幅度、周期和使能；sta 为 0～3，3 仅 X 系列 | down、up |
| touch_cap（触摸捕捉） | 仅 vscope 可设置；每页最多一个，无几何属性；val 为只读捕捉结果 | down、up |
| gauge（指针） | x/y/w/h 必填；val 为 0～360，尖长／尾长为有符号整数；sta 为 0～3，3 仅 X 系列 | down、up |
| waveform（波形） | x/y/w/h 必填；ch 为 1～4，支持方向、网格、通道颜色和缩放；sta 为 0～3 | down、up |
| qrcode（二维码） | x/y/w/h、txt 必填；txt_maxl 为 0～192；sta=0 无中心 LOGO、1 使用 pic 图片 | down、up |
| switch（状态开关） | x/y/w/h、font 引用表必填；val 为 0／1，dez 为方向；当前仅 X543 | down、up |
| combo_box（下拉框） | x/y/w/h、font 引用表必填；path 是选项集合，txt 是独立保存的文本；当前仅 X543 | down、up |
| text_select（选择文本） | x/y/w/h、font 引用表必填；path 是选项集合，val 为 u32，txt 只读；当前仅 X543 | down、up |
| sliding_text（滑动文本） | x/y/w/h、font 引用表、txt 必填；maxval_y 只读，当前仅 X543 | down、up |
| file_browser（文件浏览器） | x/y/w/h、font、pic1／pic2 引用表必填；可设目录、过滤条件和显示参数；当前仅 X543 | down、up |
| file_stream（文件流） | 强制私有，当前仅声明未打开状态，无几何属性；当前仅 X543 | 无 |
| data_record（数据记录） | x/y/w/h、font 引用表、path、lenth、maxval、dez 必填；强制全局，每工程最多八个；当前仅 X543 | down、up |
| external_picture（外部图片） | x/y/w/h、path 必填；path 可为运行时路径字符串或外部图片资产的 ref；当前仅 X543 | down、up |
| animation（动画） | x/y/w/h 必填；from=0 使用 vid 内嵌资源，1 使用外部 path，支持播放状态、循环和速度；当前仅 X543 | down、up、playend |
| video（视频） | x/y/w/h 必填；from=0 使用 vid 内嵌资源，1 使用外部 path，支持单音轨保留；当前仅 X543 | down、up、playend |
| audio（音频） | 无几何属性；from=0 使用 vid 内嵌资源，1 使用外部 path，支持播放状态、循环和速度；当前仅 X543 | playend |

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

统一入口在 `.build/hmi-toolchain` 保存相同的目标文件和链接描述，可用独立链接器再次链接。此目录是本次构建计划及目标文件的导出位置；供增量构建复用的产物单独保存在 `.build/hmi-cache`。

当前 `.hmo` 使用 `hmi.object/2`，保存对象记录、脚本分析结果和 ID 重定位表。编译阶段的控件 ID 为零占位值，链接阶段才分配最终 ID，并生成页面头、索引和 CRC。更新到此阶段后应重建工程，旧 `.hmo` 和 target.json 不可继续使用；这些格式／数据版本与分发版本号分别管理。

`hmi-compile inspect` 会按注册类型显示属性，包括变量的有符号 val、UTF-8 文本和容量。普通控件显示 id_pending=true，表示 ID 尚未由链接器填写；最终 ID 以 HMI 或 link-map.json 为准。

## 验证

```powershell
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
```

自动测试包括固定原生输出摘要、型号与方向、独立链接、控件增删改名重排、变量模式和字节容量、作用域、事件归属、损坏的重定位数据、错误时保留输出、输入文件保护和 JSON 结果。上位机打开、保存、编译需要对本阶段生成的工程单独验收。

增量构建另有局部失效、缓存损坏、警告重放、原始附件、清理保护及跨进程锁测试。使用已支持的现有 HMI 做命令行组合验证时，可执行：

```powershell
python examples/verify_incremental_build.py --inputs path/to/first.HMI path/to/second.HMI
```

脚本在新目录导入输入，依次比较首次构建、缓存复用、全量重建、独立链接和清理后再次构建，核对原始附件与最终输出均未被清理。结果写入 `target/acceptance-incremental/verification.json`；输出目录存在时停止，可用 `--output` 指定其他目录。此流程不访问虚拟 SD，也不替代上位机或硬件验收。

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

X543 同样接收这三种输入，编码为 JPEG，并开放 picture／crop。codec=auto 按型号选择编码，JPEG quality 默认 80、范围 1～100；质量 100 仍使用 JPEG。alpha=preserve 默认保留七位运行透明度，flatten 配合背景色可显式去透明。内部补齐和旋转不改变逻辑尺寸，详见[媒体编码器](../hmi-media/README.md)。

生成 X543 的 0°／90° JPEG 验收工程：

```powershell
cargo build --workspace --bins --locked
python examples/prepare_jpeg_acceptance.py
```

结果与核对说明在 `target/acceptance-jpeg/`。脚本复用公开的 BMP／PNG 生成函数，覆盖默认质量、96／100 质量、去透明、缩放、小尺寸补齐和切图，并实际执行独立工具计划比较输出。已有目录不会覆盖，可另用 --output。无需已有 HMI 或内部素材。

生成透明图片与白底去透明的对照工程：

```powershell
python examples/prepare_alpha_acceptance.py
```

结果在 `target/acceptance-alpha/`，包含 X543 的 0°／90° 工程。透明圆环共用一个资产，分别叠在不同底色上，另有同一 PNG 显式去透明的白底版本。脚本核对 has_alpha 和 PNG／BMP 编辑源，并实际执行独立工具计划；无需外部素材或字库，已有目录不会覆盖。

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

文本和普通按钮当前支持 sta=0 切图、1 单色、2 图片，X 系列另支持 3 透明；文本 style 默认 0，按钮默认 4。完整原生字段始终保留，隐藏字段不会因为模式变化被删除。图片字段和模式要求见后文“图片背景与图片游标”。

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

两类支持 sta=0 切图、1 单色、2 图片，X 系列另支持 3 透明；style 为 0～4，默认 0。支持与文本相同的颜色、边框、对齐及字间距属性，但没有 txt、txt_maxl 或 pw。isbr 默认 1。pic／picc 使用同一套条件图片引用规则，非活动值仍保留在原生记录中。

复选框和单选框的 val 默认 0，可设 0／1，表示未选中／选中；默认白底 bco=65535、前景 pco=1024。复选框另有 style（0～4，默认 2）、borderc（默认 0）和 borderw（默认 2）。单选框没有这些边框／风格字段，也不会自动生成互斥脚本；如果需要互斥，应在事件中显式设置其他单选框的 val。

进度条使用独立的模式定义：sta=0 表示单色，图片模式 sta=1 使用 bpic／ppic 引用。val 默认 0，范围 0～100；dez=0 横向、dez=1 竖向，默认 0。默认背景 bco=50712、前景 pco=1024，颜色范围均为 0～65535。

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

四类均支持 vscope，默认 0。滑块、双态按钮、滚动文本还支持 sendkey，默认 0。滑块已接入图片／切图背景和图片游标；双态按钮及滚动文本也已接入图片／切图背景。双态按钮和滚动文本在 X 系列另支持透明背景，原生记录保留完整字段。

滑块的 mode 为 0 横向／1 竖向，默认 0；sta 为 0 切图、1 单色、2 图片，默认 1。val 默认为 0，minval/maxval 默认 0/100，三者范围均为 0～65535，且 minval 不能大于 maxval。官方说明中的 minval/maxval 只在滑动时限制 val，不能据此拒绝范围外的显式初始值，工具不会自动夹紧 val。

| 滑块属性 | 默认值与范围 |
|---|---|
| psta | 默认 0；通用范围为 0／1／2，K0 已确认限制为 0／1。1 为图片游标，必须提供 pic2 引用 |
| wid、hig | 默认 255，范围 0～255；255 表示自动，0 表示无游标 |
| dis、ch | 默认 100、25，范围 0～100；分别为圆角比例、背景宽度比例 |
| bco、bco1、pco | 默认 50712、1024、63488，范围 0～65535；分别为背景、前景和游标颜色 |

psta=0 的实际外观有型号差异：此前 K035 预览为方形，X543 为圆形。工具保留相同字段和值，不在 K035 上自动改成其他模式，也不保证编辑器、模拟器和硬件表现完全相同。

K0 的上位机只有 0／1 两种游标选项；X5 另有 2（单色四边形）。型号属性差异使用统一约束表，未记录差异的型号沿用通用范围，不因尚未逐项验证而预先收窄。当前 T0 按通用范围处理，可使用 psta=2；以后确认差异时再增加配置。图片游标 psta=1 已接入，沿用相同的型号范围检查。

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

## 指针、波形和二维码

这三类当前都不需要图片或字体资产。x/y/w/h 必填，vscope、sendkey 默认 0，事件槽为 down、up。

```toml
[[objects]]
name = "z0"
type = "gauge"
properties = { x = 60, y = 40, w = 200, h = 200, val = 45 }

[[objects]]
name = "s0"
type = "waveform"
properties = { x = 20, y = 40, w = 280, h = 160, ch = 2 }

[[objects]]
name = "qr0"
type = "qrcode"
properties = { x = 60, y = 40, w = 200, h = 200, txt = "HMI-A", txt_maxl = 16 }
```

示例条目应分别安排在相应页面或调整实际位置。

指针 sta=0 为切图、1 为单色、2 为图片、3 为透明；透明仅 X 系列支持，T0／K0 限制为 0～2。默认 sta=1、bco=65535，sta=2 必须提供 pic、sta=0 必须提供全屏 picc 引用。val 为 0～360，默认 0；指针色 pco 默认 0、中心圆颜色 pco2 默认 63488，颜色范围均为 0～65535。

| 指针形状属性 | 默认值与范围 |
|---|---|
| up | 尖长，默认 32767 表示自动；有符号 16 位，范围 -32768～32767 |
| down | 尾长，默认 30；有符号 16 位，范围 -32768～32767 |
| hig | 中心圆直径，默认 24，范围 0～100；0 表示无中心圆 |
| vvs0、vvs1、vvs2 | 尖宽、腰宽、尾宽，默认 1／7／2，范围均为 0～128 |

原生的 format、left、wid 本批分别初始化为 0、0、2，暂不作为可编辑属性。工具创建指针本身，不自动生成表盘背景。

波形 sta=0 为切图、1 为单色、2 为图片、3 为透明，默认 sta=1、黑底 bco=0、网格色 gdc=1024。sta=2 必须提供 pic，sta=0 必须提供全屏 picc 引用。颜色与网格参数始终保留，背景和网格色用于单色模式。波形文档未单独注明 sta=3 的系列限制，当前沿用 0～3 通用范围；后续确认例外时通过同一型号约束表调整。ch 为 1～4、默认 1，四个颜色字段始终保留；减少通道数不会清除其他通道的颜色。

| 波形属性 | 默认值与范围 |
|---|---|
| dir | 默认 0；0 左到右、1 右到左、2 靠右对齐 |
| gdw、gdh | 网格宽／高，默认 40，范围 0～255；0 关闭对应网格 |
| dis | 数据缩放百分比，默认 100，范围 10～1000，按两字节保存 |
| pco0、pco1、pco2、pco3 | 通道颜色，默认 64495／65519／63488／2016，范围 0～65535 |

objWid、objHig、inittrue、molloc_s、molloc 按原生配方初始化为零，名字和字段宽度保留；它们不作为用户输入。创建后的波形没有预置数据，曲线由运行时指令提供；当前静态分析不完整检查 add／addt 的参数和实际绘图效果。

二维码的 sta 是中心 LOGO 开关：0 无、1 有，默认 0，不能套用其他类型的背景模式。sta=1 必须提供 pic 图片引用，不允许省略或 none；sta=0 时 pic 默认为原生空值65535，也可保留非活动引用。默认 bco=65535、pco=0，txt 必填，txt_maxl 默认 16、范围 0～192，按 UTF-8 字节检查。文本保存在控件中，由上位机／屏幕生成二维码，不需要导入字体或预先编码成图片。原生 dis 保持 8，暂不作为可编辑属性。LOGO 使用普通图片资产与引用，不要求全屏；工具不会根据控件几何缩放共享源图，不修改二维码文本、颜色或控件尺寸。

本批验证控件结构、静态预览及上位机编译；扫码结果、动态曲线和硬件运行分别验收。

生成这一批的验收工程：

```powershell
cargo build --workspace --bins --locked
python examples/prepare_graphics_acceptance.py
```

脚本无需媒体素材，生成 K035、X543 的 0° 工程，每份含 gauge、wave、qrcode、qrcode2 四页，并实际执行 6 条独立命令比较输出。它核对负尾长的有符号属性显示、二维码文本及空资产构建；结果和说明在 `target/acceptance-graphics/`，已有目录不会覆盖，可另用 --output。脚本不读取已有 HMI、字库或内部资料。

## X 系列交互控件

本批四类控件按官方系列说明开放于 X543，T0、K0 已记录为不支持。限制位于通用型号配置的 control_support 中，编译器不为每个类型单独判断型号。四类都支持 vscope、sendkey，默认均为 0；字体引用使用 ref／id 表，当前需要有效字体资产。

```toml
[[objects]]
name = "sw0"
type = "switch"
properties = { x = 20, y = 20, w = 320, h = 88, font = { ref = "ui" }, txt = "0/1", val = 0 }

[[objects]]
name = "cb0"
type = "combo_box"
properties = { x = 20, y = 20, w = 300, h = 84, font = { ref = "ui" }, path = "A\nB\nC", path_m = 64, txt = "LABEL", val = 1 }

[[objects]]
name = "select0"
type = "text_select"
properties = { x = 20, y = 20, w = 300, h = 232, font = { ref = "ui" }, path = "A\nB\nC", path_m = 64, val = 1 }

[[objects]]
name = "slt0"
type = "sliding_text"
properties = { x = 20, y = 20, w = 440, h = 232, font = { ref = "ui" }, txt = "A\nB\nC\nD", txt_maxl = 64, path_m = 0 }
```

这些条目用于分别演示四页配置。path 在下拉框／选择文本中表示选项文本，每行一项，不是本机文件路径；滑动文本的正文使用 txt。可以写 TOML 转义换行，也可以使用多行字符串。LF 和 CRLF 均统一保存为原生 CRLF；空行和末尾换行保留，容量按转换后的 UTF-8 字节数检查，单独的 CR 报错。

状态开关的 val 为 0／1，dez 为 0 横向／1 竖向，默认均为 0；txt 默认为空，可以使用如 0/1 的两段标签，txt_maxl 默认 24、允许 0～24。dis 为 0～100、默认 100。状态 0 的 bco/pco 默认 50712／65535，状态 1 的 bco2/pco2 默认 1024／65535；文字色 pco1 默认 0。颜色均为 RGB565 的 0～65535。

下拉框、选择文本和滑动文本支持 sta=0 切图、1 单色、2 图片、3 透明，默认单色白底黑字，边框默认颜色 0、宽度 2；下拉框／滑动文本的 style 为 0～4，选择文本为 0～1，默认均为 0。

| 下拉框属性 | 默认值与范围 |
|---|---|
| path、path_m | 选项集合默认空，容量默认 64，范围 0～65535 字节 |
| txt、txt_maxl | 独立保存的文本默认空，容量默认 16，范围 0～65535 字节 |
| val | 初始选择值，0～255、默认 0，不在编译时据此重写 txt |
| dir、qty | 展开方向 0 上／1 下／2 左／3 右，默认 1；展开数量 1～254，默认 2 |
| hig、down | 单元格高／宽 16～255，默认 72；展开状态 0 收缩／1 展开，默认 0 |
| up、pco3 | 箭头显示 0／1，默认 1；箭头颜色默认 42292 |
| bco1、pco1、bco2、pco2 | 普通单元格背景／字体默认 65535／0，选中背景／字体默认 31／65535 |
| mode、wid | 滑动标识类型 0～2、默认 2；标识高／宽 8～255、默认 12 |
| vvs0、vvs1 | 单元格间距、标识间距，0～255，默认 8／2 |
| ch、dis | 滑动惯性 0～32、默认 8；圆角比例 0～100、默认 25 |

下拉框还支持 xcen/ycen（0～2、默认 1）和 spax（0～255、默认 0）。drastate 固定初始化为零，未作为输入。用户可以明确设置 txt，但运行时如何随 val 更新由控件处理；构建过程不会强行把示例 LABEL 改成 B。

选择文本的 path/path_m 同样是集合和容量，默认空／64。val 按四字节 u32 保存，范围 0～4294967295；不把它误作下拉框的一字节值。txt 是运行时只读结果，当前原生缓存初始化为空、txt_maxl 保留 255，不接受这两个源码属性；也不根据 val 预先填入某项文字。选择索引在运行时的具体行为另行核对。

选择文本的 hig 为 16～255、默认 72；dis 为选中项横线开关 0／1、默认 1，pco1 为横线色、默认 31；pco2 为选中字体色、默认 63488。ch 为 0～32、默认 8，spax 为 0～255、默认 0。

滑动文本的 txt 必填，txt_maxl 默认 64、范围 0～65535；path_m 表示行缓冲容量，默认 0 自动取 txt_maxl，按每行两个字节计算可用行数。文本行数不能超过此容量的一半；这里不是选项集合的长度。原生 path 保持为空，不把正文复制进去。

滑动文本支持 xcen（0～2、默认 0）、left（0 隐藏／1 操作时显示／2 始终显示进度条，默认 1）、ch（0～32、默认 8）、isbr（0／1、默认 0）、spax/spay（0～255、默认 0）。maxval_y 由运行时计算、只读；val_y 可在脚本中设置，本批在文件中初始化为零，不开放初始位置输入。leftshow 初始化为零。

只读规则由控件注册表声明，触摸捕捉 val、选择文本 txt、滑动文本 maxval_y 共用静态赋值检查；读取和比较可用。检查范围仍以本工具已支持的静态引用形式为限。

生成本批两份状态对照工程：

```powershell
cargo build --workspace --bins --locked
python examples/prepare_x_interaction_acceptance.py --font path/to/ui.zi
```

字库须覆盖示例 ASCII 字符，建议字高 68。脚本生成两份 X543、0° 工程，各含 switches、combo、select、sltext 四页；三类选择值分别为 0／1，滑动文本末行分别为 D／EE。下拉框的 txt 明确设为 INIT 并保留，不因 val 自动改写。每份执行 7 条独立命令比较输出。结果和说明位于 `target/acceptance-x-interaction/`；已有目录不会覆盖，可另用 --output。脚本只读取独立 ZI，不读取已有 HMI 或内部资料。

## 文件浏览器和文件流

```toml
[[objects]]
name = "fb0"
type = "file_browser"
properties = { x = 20, y = 20, w = 440, h = 232, font = { ref = "ui" }, pic1 = { ref = "folder_icon" }, pic2 = { ref = "file_icon" }, dir = "sd0/", filter = "*.txt|*.data" }
events = { up = "events/fb0.up.s" }

[[objects]]
name = "fs0"
type = "file_stream"
```

这两类当前在 X543 中开放，T0／K0 的限制记录在通用 control_support 配置中。文件浏览器的 pic1 是文件夹图标、pic2 是默认文件图标。两项都可使用普通图片引用，省略时按需补齐项目自绘的64×64透明图标；显式配置不会被自动替换，none仍不允许。内置图标按文件夹／默认文件顺序追加到显式资产之后，多页共享，不占据固定的前两个ID。重排资产后按最终列表重定位，显式图标也可使用已支持的PNG透明图片编码。

| 浏览器属性 | 默认值与范围 |
|---|---|
| dir | 运行时目录，默认 sd0/，最大 255 个 UTF-8 字节 |
| filter | 文件名过滤条件，默认 *，最大 64 个 UTF-8 字节，可用 `|` 分隔模式 |
| buffsize | 目录结果缓冲区字节数，u32，默认 32000；本工具不估算运行时可用内存 |
| psta | 是否支持子目录，0／1、默认 0 |
| fwarning | 编译时非 UTF-8 警告开关，0／1、默认 1；运行脚本不能读写 |
| left、ch | 进度条显示 0～2、默认 1；滑动惯性 0～32、默认 8 |
| dis、spax、spay | 文件名行数、横向／纵向间距，0～255，默认 3／24／8 |

浏览器支持 sta=0 切图、1 单色、2 图片、3 透明，默认 sta=1、白底黑字、style=2，style 可设 0～4；borderc/borderw 默认 0／2。选中背景 bco2=31、字体 pco2=65535，颜色范围均为 0～65535；xcen 默认 1、范围 0～2。vscope 默认 0，可设 0／1，但 txt 只能在所属页读取，全局作用域不会解除这一限制。

dir 是屏幕的目录路径，不是构建机路径；构建时不会枚举、创建或清空 SD 目录，也不把 filter 当作本机文件匹配条件。txt、qty、最大滚动值和缓存保留原生初始状态，由运行环境更新，不伪造文件名或数量。val 的初始四字节为 FF FF FF FF，本批不开放源文件中的初始赋值，运行脚本可按控件规则设置；自定义文件图标 vvs2 支持按后缀映射，语法及容量见后文。

文件流是非可视控件，只有 vscope=0 可配置，没有 x/y/w/h、字体或事件槽。文件初始未打开，val=0、qty=0、en=0；原生方法分派及非零保留字段按已审查配方生成，不作为用户地址或缓存数值修改。需要打开、读取或写入时，在页面或其他控件事件中调用方法，当前静态分析不完整校验这些方法的参数和读写效果。

脚本可在所属页设置文件流 val、读取 qty/en；qty/en 只读，文件流不允许跨页访问或调用。属性访问规则集中在注册表的 access 中：readonly 表示不可写，page_local 表示只限所属页，compile_only 表示不能在运行脚本中访问。这些限制与源码 properties 是否可编辑分别声明。

文件浏览器支持显式及自动图标，真实SD目录内容仍由运行环境提供；构建不枚举目录。数据记录的文件处理范围见下一节。

生成文件控件的验收工程：

```powershell
cargo build --workspace --bins --locked
python examples/prepare_files_acceptance.py --font path/to/ui.zi
```

脚本用标准库绘制两个 64×64 透明图标，并读取指定的独立字库，生成 X543 的 Files0／Files1 两份工程。两份过滤条件分别为 *.txt／*.txt|*.data，文件流名称分别为 fs0／fs1；每份实际执行 7 条独立命令比较输出。结果在 `target/acceptance-files/`，已有目录不会覆盖，可另用 --output。脚本不访问虚拟 SD，不读取已有 HMI 或内部素材。

## 数据记录

```toml
[[objects]]
name = "data0"
type = "data_record"
properties = { x = 20, y = 20, w = 440, h = 232, font = { ref = "ui" }, path = "sd0/records.data", lenth = 32, maxval = 16, dez = 2, format = "200^200", dir = "T^V" }
```

数据记录在当前内置型号中仅对 X543 开放，vscope 固定为 1。源码编译检查单页数量，链接检查整个工程的总数，最多八个。背景支持 sta=0 切图、1 单色、2 图片、3 透明，默认 1；原生记录包含全部 72 个属性；方法分派、初始化字段和非零保留字段按创建配方生成，不开放为用户数值。

| 文件与结构属性 | 含义和范围 |
|---|---|
| path | 必填，屏幕运行时的数据文件路径，1～255 个 UTF-8 字节；不是本机输入路径 |
| lenth | 必填，每条记录的最大字节数，0～255；不含文件槽位末尾的 NUL；保留原生属性名拼写 |
| maxval | 必填，数据文件的最大记录数，按 u32 保存；超过后由控件循环覆盖旧记录 |
| dez | 必填，列数，1～12，按 u16 保存 |
| format | 默认空字符串，由控件处理默认列宽；也可用 `^` 分隔列宽，例如 200^200；最多 256 个 UTF-8 字节 |
| dir | 默认空字符串，表示隐藏表头；也可用 `^` 分隔标题，例如 T^V；最多 255 个 UTF-8 字节。这是表头，不是目录 |
| mode | 默认 1，路径下文件不存在时允许原生环境自动创建；0 表示不允许创建，需自行提供结构匹配的文件 |
| order | 默认 0，新数据在前；1 表示新数据在后 |
| dis | 默认 1，允许触摸选中；0 表示不允许 |

lenth、maxval、dez 可在源码 properties 中配置，运行脚本只能读取。修改这三个参数后，已有 .data 文件可能不再匹配；本工具不会修改或删除已有数据，也不会用空文件替代这种专用格式。请为新结构使用新的运行时路径，或自行迁移已有数据。

界面默认白底黑字、style=2、边框颜色 0／宽度 2、xcen=0；style 为 0～4，xcen 为 0～2。hig 是行高，1～255、默认 72；left 是进度条显示方式，0～2、默认 1；ch 为 0～32、默认 8，spax 为 0～255、默认 0。gdc 默认 50712；gdw/gdh 是横向／纵向表格线宽度，均为 0～255、默认 1，0 表示关闭。表头 bco1/pco1 默认 23765／65535，选中项 bco2/pco2 默认 31／65535；颜色均为 0～65535。

val 默认 4294967295，即原生的未选中状态；val、val_x、val_y 可配置初始 u32 值，滚动值默认零，实际有效范围由运行环境的数据内容决定。qty、txt、maxval_x、maxval_y 只读，文件中初始化为零或空字符串；不会根据 val 预先编造记录内容。insert/delete/up/clear 是方法，可在事件脚本中调用；当前静态分析检查静态属性访问，不完整验证方法参数或真实文件读写效果。

页面目标文件的 `metadata.runtime_dependencies` 保存每个控件的文件需求，包括对象序号、运行时路径、结构参数和是否允许自动创建。链接器从这些声明汇总 `link-map.json` 的 `runtime_files`，并核对声明与目标内的原生记录一致；只持有 `.hmo/.hma` 时也能完成检查。相同初始路径的结构参数不同会报错；结构相同会产生占用冲突警告，因为多个控件不能同时打开同一文件，需自行通过脚本协调。sd0/ 路径的 ASCII 字母按大小写不敏感比较，保存的路径不改写。

数据记录条目的 `runtime_files[].generated` 仍为 false，表示这是部署需求，链接器未生成 .data 文件。`structure.record_bytes/max_records/columns` 分别对应 lenth/maxval/dez；`users` 列出使用者的页面、控件、最终 ID 及 create_if_missing。此表只描述源码中的初始绑定，运行脚本后续修改 path 不会自动生成新的依赖。构建不读取虚拟 SD，不嵌入或创建 .data 文件；mode=1 时由原生环境创建，mode=0 时需要用户提供已有文件。当前不需要由工具生成 .data：mode=1 时由原生环境自动创建，mode=0 时由用户提供匹配文件。预置记录或离线制作数据文件属于将来的可选扩展，不计入本次必做范围。

生成验收工程：

```powershell
python examples/prepare_data_record_acceptance.py --font path/to/ui.zi
```

输出位于 `target/acceptance-data-record/`，包含 Record0／Record1 两份工程，分别使用两列／三列、32／63 字节记录上限和 16／1000 条容量，绑定不同的实验路径。两份均为 X543、0°，每份实际执行 4 条独立作业并比较 HMI 和文件需求映射。脚本不访问虚拟 SD，已有输出目录不会覆盖，可用 --output 指定新目录。

## 外部图片与配套文件输出

外部图片控件在 HMI 中只保存路径，不把图片内容嵌入容器。当前内置型号中仅 X543 开放此控件，支持四个工程方向；默认私有，也可设置 vscope=1。可用已有 JPG／XI 的运行时路径，或引用由工程生成的外部图片资产：

```toml
[[objects]]
name = "exp0"
type = "external_picture"
properties = { x = 20, y = 20, w = 138, h = 106, path = { ref = "logo" } }
```

在工程 `[assets].items` 中声明 logo 的资产描述，其内容例如：

```toml
schema = "hmi.asset/1"
kind = "image"
source = "logo.png"

[encoding]
quality = 96
alpha = "preserve"

[delivery]
mode = "external"
path = "sd0/pictures/logo.xi"
```

| 字段 | 作用 |
|---|---|
| source | 构建机上的原图输入，相对于资产描述文件；支持已有的 BMP／PNG／JPEG 输入及显式 resize |
| delivery.mode | 默认 embedded；external 表示生成独立文件，不占普通图片 ID，也不生成内嵌 .is |
| delivery.path | external 时必填，指定屏幕读取的路径；当前输出路线要求 sd0/ 下的 .xi 文件，不能使用绝对路径、反斜杠、.、.. 或非法文件名 |
| 控件 path | 字符串表示直接使用运行时文件；`{ ref = "logo" }` 则在链接时替换为该资产的 delivery.path。路径引用不接受 id／none |

例如输出 HMI 为 `dist/Panel/Panel.HMI`，同时生成 `dist/Panel/sd0/pictures/logo.xi`。自行指定 `-o` 时，配套文件也跟随 HMI 的父目录输出。用户将 sd0/ 下的文件按相同目录结构复制到 SD 卡或虚拟 SD 后，上位机才能预览或运行外部图片；工具不会自动写入用户的虚拟 SD。

外部编码复用 JPEG Baseline YUV420、四方向旋转和七位透明度流，以 XI 的签名和头部包装。源图片尺寸与控件几何分别配置，不因修改控件宽高而隐式修改共享资产。当前可从常规图片生成 XI，也可直接封装受支持的 XI；普通 JPG 的 encoded 原样输出尚未接入。

如果写 `path = "sd0/existing.jpg"`，构建只保存这个字符串并记录需求，不读取构建机同名文件，也不会自动生成或复制该图片。由工程声明的 external 资产即使暂时没有控件引用也会输出；多个控件可以共享一个外部图片资产。两个资产声明相同输出路径会报错，避免互相覆盖。普通图片的 pic 只能引用内嵌图片，外部图片的 path 引用只能指向 external 资产。

链接映射中外部资产的 id 为 null，runtime_files 中的 generated=true 表示本次构建包含对应输出；仅填写路径字符串且没有相应资产时为 false。外部文件仍在 HMI 之外，不能仅复制 HMI 就视为完整部署。

统一入口和独立 hmi-link 共用输出流程：写入前检查全部输出与输入的路径冲突，再将所有文件暂存；配套文件和链接映射先写入，最后替换 HMI。暂存失败保留已有文件；发布途中失败会报告已写入的文件，多文件不承诺整体事务。需要整包切换时请输出到新的发布目录。此机制不改变 .data 当前仅记录需求的状态。

生成外部图片验收工程：

```powershell
python examples/prepare_external_acceptance.py
```

输出位于 `target/acceptance-external/`，包括 X543 的 0°／90° HMI 和 sd0/ 下的两个 XI 文件，不需要外部素材或字库。请先把两个 XI 复制到上位机的虚拟 SD 根目录，再打开对应 HMI；详细核对项目见输出目录的 README。每份实际执行五条独立作业，比较 HMI、XI 和链接映射；已有输出目录不会覆盖，可用 --output 指定新目录。

## GIF 动画

在工程资产表中声明动画描述，原始 GIF 作为唯一必需的媒体输入：

```toml
schema = "hmi.asset/1"
kind = "animation"
source = "loading.gif"

[encoding]
quality = 80
alpha = "preserve"
timing = "native-compatible"
```

| 字段 | 含义与默认行为 |
|---|---|
| source | 相对于资产描述文件的 GIF 路径；检查实际签名、逻辑画布、帧范围及数据块，按 GIF 语义合成局部帧和处理 disposal |
| encoding.quality | JPEG 运行帧质量，1～100、默认 80；100 仍使用 JPEG，不切换到私有编码 |
| encoding.alpha | 默认 preserve，保留透明度；flatten 时必须指定 background="#RRGGBB"，不改写原始 GIF |
| encoding.timing | 当前只支持 native-compatible：丢弃零延时帧，按原生规则处理不足 30ms 的短帧，单帧延时最大 65000ms |
| delivery | 默认 embedded；external 输出 sd0/ 下的 .gmov 运行文件，不占内部动画 ID |

动画不接受 fps 或 codec 参数，播放时序来自 GIF 延时；loop/en 是控件属性，不放在共享资产中。当前不提供动画缩放参数，保留原始逻辑尺寸；修改控件几何不会隐式改写共享 GIF。尺寸补齐只发生在运行帧的编码光栅中，先按 16 对齐再旋转，不改变逻辑画布。

当原生时序规则改变了帧或延时，会产生 animation_timing 警告，列出源帧数／时长与输出帧数／时长；-Werror 会阻止这类构建替换已有 HMI。资产描述中 timing 固定为上述策略，不用静默忽略短帧。inspect 中的 frame_indices、delays_ms、timing_adjusted 记录具体调整，frame_count/duration_ms 与 source_frames/source_duration_ms 可直接比较。独立 hmi-media 的检查／编码结果也报告帧数和时长。

运行 `.gmov` 是 76 字节头、40 字节帧索引、JPEG／透明度帧和扩展区；编辑源 `.gmovs` 包装完整原 GIF，便于上位机继续编辑。二者作为不同成员封装在自包含 `.hma` 中，链接器不调用 GIF 或 JPEG 解码器，也不需要原始 GIF 仍在本机。动画与普通图片、字库分别分配 ID，重排动画资产只按最终动画列表解析引用。

控件示例（clip 是当前工程的动画资产符号）：

```toml
[[objects]]
name = "gm0"
type = "animation"
properties = { x = 20, y = 20, w = 160, h = 80, vid = { ref = "clip" }, en = 0, loop = 1, dis = 100 }
events = { playend = "events/gm0.playend.s" }
```

from=0 时 vid 必须为动画的 `{ ref = "..." }` 或最终动画列表的 `{ id = ... }`，不能使用裸整数、none 或其他媒体类别。from=1 时 vid 可省略或写 `{ none = true }`（原生65535），也可保留有效内部动画引用。当前内置型号中仅 X543 开放动画，支持四个方向；T0/K0 的限制通过统一能力表配置。

| 控件属性 | 含义与范围 |
|---|---|
| from | 默认 0，使用 vid 内嵌动画；1 使用外部 path。非活动配置保留 |
| vscope、sendkey | 默认均为 0，可设 0／1；动画默认私有 |
| en | 默认 0 停止，1 播放，2 暂停 |
| loop | 默认 0，不循环；1 表示循环；不会因为 loop=1 自动把 en 改为播放 |
| dis | 播放速度百分比，10～1000、默认 100 |
| tim | 当前播放时间的 u32 初值，默认 0；实际播放和定位行为需在运行环境核对 |
| stim、qty | 运行时总时长／总帧数，只读；文件中初始化为 0，不从资产信息预先回填 |

事件槽为 down、up、playend。原生保留字段按创建配方写入，不作为用户可编辑数值；属性只读检查、作用域和事件归属沿用共同规则。当前 GIF 解码使用 Rust 图像库，无需 FFmpeg 或原厂转换工具；视频／音频的输入范围和 DLL 要求见后文。支持范围为最多 1000 个源帧、源文件不超过 64 MiB，单帧解码 RGBA 不超过 64 MiB、保留帧 RGBA 总量及运行资源各不超过 128 MiB；纯文本 GIF 扩展当前明确拒绝。

生成动画验收工程：

```powershell
python examples/prepare_animation_acceptance.py
```

输出位于 `target/acceptance-animation/`，包括 X543 的 Animation0／Animation90 两份工程，每份有不透明／透明动画两页。GIF 由代码生成，不需要外部素材或字库；资源全部内嵌，无需部署到虚拟 SD。两份有意重排动画资产并使用不同 loop 值，每份实际执行七条独立作业比较 HMI。已有输出目录不会覆盖，可用 --output 指定新目录，详细核对项目见输出目录的 README。

## 视频与 FFmpeg DLL

视频编码当前使用 Windows x64 的 FFmpeg 9 shared 库解码，再生成 JPEG 帧和 HMI 的 VIDEO 资源。构建前设置库目录，检查后端：

```powershell
$env:HMI_FFMPEG_DIR = "D:\tools\ffmpeg\bin"
.\target\debug\hmi-media.exe backend --json
```

需要同包的 avutil-61、swresample-7、avcodec-63、avformat-63、swscale-10 DLL；不调用 FFmpeg EXE。未设置目录时，读取可执行文件旁的 ffmpeg 子目录。库不随本仓库自动下载，版本不匹配会报错；目录和绑定信息见[后端说明](../hmi-media/src/ffmpeg/README.md)。普通图片、GIF、字库和独立链接不需要 FFmpeg；已有 `.hma` 可在没有解码器的环境中链接。

资产示例：

```toml
schema = "hmi.asset/1"
kind = "video"
source = "intro.mp4"

[encoding]
quality = 80
fps = 4

[clip]
start_ms = 0
duration_ms = 1000

[audio]
mode = "drop"
```

| 字段 | 当前行为 |
|---|---|
| source | 本地原始视频，相对于资产描述文件；实际编码由 FFmpeg 判断，不能仅改后缀冒充。保留源画面尺寸，当前没有缩放参数 |
| encoding.quality | JPEG 质量，1～100、默认 80；100 仍是 Baseline YUV420 JPEG |
| encoding.fps | 必填，4、6、8、10、12、14、18、24 中的一项，不高于源帧率；首批要求源帧率÷目标帧率为正整数，例如 24→4／6／8／12／24 |
| clip.start_ms | 默认 0，以第一幅有效解码画面为起点；须精确落在源帧边界 |
| clip.duration_ms | 正整数，省略时使用 start 之后的有效画面长度；结束位置须落在源帧边界，时长须包含整数个目标帧。超出范围、非整帧或不能精确表示的边界报错，不补帧、不静默取整 |
| audio.mode | 默认 if-present，有一条音轨时保留、无音轨时无声；drop 显式丢弃；require 必须有音轨。需要保留多音轨输入时拒绝，不擅自选轨或混音 |
| audio.sample_rate／channels | 输出音轨默认 8000Hz／单声道；可选采样率 8000、11025、22050、24000、32000、44100，声道 1／2；转换为 s16 PCM。drop 时禁止填写，if-present 无音轨时报告 audio_applied=false |
| delivery | 默认 embedded；external 输出 sd0/ 下的 .video，不占内部视频 ID |

首批只处理时间戳精确等间隔的固定帧率视频。变帧率、隔行、源旋转矩阵、非方形像素、HDR、透明画面或中途改变尺寸／格式的输入明确拒绝；不把这些情况擅自当作普通画面处理。输入文件和输出 VIDEO 各不超过 128 MiB，源帧最多 10000 个，单帧 RGBA 不超过 64 MiB、全部解码 RGBA 不超过 128 MiB。当前会解码全部画面核对时序，长视频的流式处理待后续完善。

probe／inspect 的视频信息记录源帧率和帧数、截取起点、输出帧数／时长、源音轨数量和处理策略、源文件摘要及后端指纹。音轨按源时间戳与画面截取范围对齐；非整采样边界、音轨中途参数变化或时间戳跳变拒绝。音轨开始较晚或提前结束时，按时间轴补静音并报告采样数（audio_silence 警告，-Werror 可提升）。只有显式 drop 才丢弃声音。

VIDEO 帧采用直接 JPEG 包装，与动画帧的 20 字节前缀不同。HMI 中的资源名是 N.v，引用 kind 为 v；视频与图片、动画分别分配 ID。原始 MP4／AVI 仍由源码工程保存，不嵌入 HMI；`.hma` 保存完整运行资源，链接器无需解码原视频。

控件用 `type="video"`，`vid={ref="clip"}` 或 `{id=...}` 引用当前视频列表。x/y/w/h 必填；from 默认 0，另支持 1 外部文件；vid 与 path 的条件要求见后文。en 为 0 停止／1 播放／2 暂停，loop 为 0／1，dis 为 10～1000、默认 100，tim 为 u32 初值、默认 0；stim/qty 初始零且运行时只读。事件为 down、up、playend。保留字段 fps 固定为原生值 1，不按资源帧率改写；molloc_s/molloc 保留 64000，不作为源文件大小修改。当前内置型号中仅 X543 支持，四方向分别编码画面并写方向标记。

生成无声视频验收工程：

```powershell
python examples/prepare_video_acceptance.py --ffmpeg-dir path/to/ffmpeg/bin
```

输出位于 `target/acceptance-video/`，包含 Video0／Video90 两份 X543 工程，各有完整片段和截取片段两页。原始 AVI 由代码生成，4fps、四帧、1000ms；截取从 250ms 开始、长 500ms。每份实际执行六条独立作业，比较 HMI 和链接映射。资源全部内嵌，打开验收 HMI 不需要配置本工具的 FFmpeg；重新构建源码才需要。已有输出目录不会覆盖，可用 --output 指定新目录。


## WAV 音频与音频控件

音频的源文件为 WAV。例：

```toml
schema = "hmi.asset/1"
kind = "audio"
source = "startup.wav"

[encoding]
sample_rate = 8000
channels = 1
sample_format = "s16"

[clip]
start_ms = 250
duration_ms = 500
```

| 字段 | 默认与含义 |
|---|---|
| source | 相对资产描述文件的 WAV 输入，检查 RIFF、fmt、data 与块边界；支持 PCM 8/16/24/32 位和浮点 32/64 位，其他压缩 WAV 暂不支持 |
| encoding.sample_rate | 省略保持源采样率；当前输出支持 8000、11025、22050、24000、32000、44100Hz。不支持的源参数需显式指定转换，不自动选值 |
| encoding.channels | 省略保持源声道数；输出支持 1／2。多声道源转换需提供明确的输入布局；不猜测未知多声道的扬声器位置 |
| encoding.sample_format | 默认且当前只支持 s16，有符号 16 位小端 PCM；不存在图片的 quality、fps 或 resize 参数 |
| clip | 起点默认 0，duration 省略到结束。按转换后的时间轴截取，起点／时长须能精确表示为输出采样，越界不裁剪或悄悄补零 |
| delivery | 默认 embedded；external 输出 sd0/ 下的格式化 .wav，不占内部音频 ID |

PCM16 参数不变时直接处理，不需要 FFmpeg；需要位深、声道或采样率转换才加载 DLL。FFmpeg 执行完整解码和重采样刷新，再按明确边界截取；原 WAV 文件不改写。音频没有屏幕方向，同一资产目标可在四方向中使用。原文件不超过 64 MiB，输出 PCM 也不超过 64 MiB。

运行 WAV 采用标准 PCM16 fmt、512 字节 hmic 和 data 块。按原生规则四字节对齐，必要时补一个静音采样；content_samples 是有效采样数，sample_frames 包含对齐补入值，padding_samples 明确记录差异。duration_ms 为实际采样时长的整数毫秒部分，精确时长可用 sample_frames/sample_rate 计算。

hmic 的 name 缺省采用源文件名，不带扩展名；已有 name 和其他受支持的 hmic 项原样保留，不依据资产 key 追踪改名。crc 重新计算，采用已验证的原生覆盖范围与算法，不使用标准 ZIP CRC32。已有工程导入时的完整未知数据保留仍属于后续解析器迁移范围。

控件用 `type="audio"`、`vid={ref="tone"}` 或当前音频列表的 `{id=...}`。音频与图片、动画、视频分别编号，资源名 N.wav、引用 kind 为 wav；视频内的音轨不占独立音频 ID。当前仅对已有音频证据的 X543 开放，不据此表示全部 X 系列型号都有扬声器。

音频是特殊控件栏中的非可视控件，不接受 x/y/w/h、字体或图片。vscope 默认 0，可设 0／1；from 默认 0，另支持 1 外部文件，en 为 0 停止／1 播放／2 暂停，loop 为 0／1，dis 为 10～1000、默认 100，tim 默认 0。stim/qty 运行时只读，初始零，不从文件回填。保留 fps=1、molloc_s/molloc=64000；只有 playend 事件，无 down/up。当前不根据全局作用域承诺跨页播放行为，实际播放、声道听感和音画同步需在模拟器或硬件单独验收。

生成音频与有声视频验收工程：

```powershell
python examples/prepare_audio_acceptance.py --ffmpeg-dir path/to/ffmpeg/bin
```

输出位于 `target/acceptance-audio/`，包含 Audio0／Audio90 两份工程，各有原参数 PCM 音频、重采样截取音频和有声视频三页。素材由标准库生成，不读取已有 HMI 或内部资源；每份实际执行八条独立作业比较 HMI。资源全部内嵌，打开验收 HMI 无需配置 FFmpeg 或虚拟 SD；重建包含转换的源码才需要 DLL。已有输出目录不会覆盖，可用 --output 指定新目录。


## 图片背景与图片游标

当前接入页面背景，以及文本、普通按钮、滑块、进度条、数字、虚拟浮点数、双态按钮、滚动文本、指针、波形的图片模式，以及二维码的中心 LOGO；X 系列下拉框、选择文本、滑动文本、文件浏览器和数据记录也支持图片／切图／透明背景。其他类型维持各自现有范围，不因共用字段布局自动放开。

| 控件 | 模式与必需图片 |
|---|---|
| page（页面自身） | sta=0 无背景、1 单色、2 全屏图片；sta=2 时 pic 必填，图片逻辑尺寸须与页面相同 |
| text／number／xfloat／scrolling_text | sta=2 时 pic 必填；sta=0 时 picc 必填；sta=1 单色；X 系列另支持 sta=3 透明 |
| button／dual_state_button | sta=2 时 pic／pic2 分别为普通／按下图片；sta=0 时 picc／picc2 为两种状态切图；sta=1 单色；X 系列另支持 sta=3 透明 |
| combo_box／text_select／sliding_text／file_browser／data_record | sta=2 时 pic 必填；sta=0 时 picc 必填；sta=1 单色、3 透明；五类控件当前仅在 X543 开放 |
| gauge／waveform | sta=2 时 pic 必填；sta=0 时 picc 必填；sta=1 单色、3 透明；指针透明仅 X 系列，波形当前按通用范围 |
| qrcode | sta=1 时 pic 为必填 LOGO；sta=0 不显示 LOGO，pic 可省略或保留 |
| slider | sta=2 时 pic／pic1 为背景／前景；sta=0 时 picc／picc1 为两项切图；sta=1 单色。独立的 psta=1 使用 pic2 游标图片 |
| progress_bar | sta=0 单色，sta=1 使用 bpic 背景与 ppic 前景；此类型的 sta 数值与文本等不同 |

示例：

```toml
[[objects]]
name = "b0"
type = "button"
properties = { x = 10, y = 10, w = 280, h = 100, sta = 2, font = { ref = "ui" }, txt = "OK", pic = { ref = "normal" }, pic2 = { ref = "pressed" } }

[[objects]]
name = "h0"
type = "slider"
properties = { x = 20, y = 40, w = 280, h = 40, sta = 1, psta = 1, pic2 = { ref = "cursor" }, val = 40 }
```

这些图片字段使用与普通图片相同的 `{ ref = "..." }`／`{ id = ... }` 引用语法。非活动字段可以省略，默认 `{ none = true }`，写入原生 65535；也可显式声明引用，用于保留切换前的配置。启用模式时，对应必需字段不能省略或为 none。普通图片控件和文件浏览器图标仍必须有有效图片，不能因此使用 none。

切回单色不会清空已声明的 pic／picc 等值；非活动引用也按最终资产表解析，删掉被引用的资产仍会报错。工具不跟踪“上一次模式”的历史，源码显式写出的最终属性就是当前配置。需要清除非活动字段时，可在源码省略它或写 none，而不是假定切换模式会帮你删除。

页面图片背景和当前启用的切图字段要求与该方向页面的逻辑画布同尺寸，控件裁切区域必须在全图范围内。隐藏的切图引用保留，不对它套用当前未启用模式的几何要求。source 图片大小与控件几何分别处理，不因控件尺寸自动缩放或重编码共享图片；需要改变资源尺寸时在资产描述中显式 resize。

滑块图片游标保留 wid/hig 的既有语义，默认 255 自动；K0 的 psta 范围仍为0／1，其他未记录例外的型号沿用通用0／1／2。切换到图片模式不擅自改游标尺寸、style、颜色、val 或事件脚本。

控件注册表的 ResourceRule 集中声明图片启用条件和切图规则，源码检查与目标记录检查共用；链接器负责最终ID、资源类别和启用切图的几何绑定。当前 .hmo 给所有已注册图片字段保存重定位，包括非活动和 none 字段，避免在独立链接时丢失或绕过引用。

生成本批验收工程：

```powershell
python examples/prepare_picture_modes_acceptance.py --font path/to/ui.zi
```

输出位于 `target/acceptance-picture-modes/`，包括 K035 的图片／切图／单色保留三份工程和 X543 的90°图片工程，每份四页。源图片由代码生成，只需提供独立字库；无需 FFmpeg 或虚拟 SD。每份实际执行13条独立作业比较 HMI，已有目录不会覆盖，可用 --output 指定新目录。详细属性与显示预期见输出 README。


数字、虚拟浮点数、双态按钮和滚动文本沿用相同的 ResourceRule，不另建图片ID或编码路线。它们的原生记录分别仍为39／39／42／46项：仅按模式和显式引用设置图片字段，不因换背景改写 val、lenth/format、vvs0/vvs1、txt_maxl、滚动参数或事件。数字的 val 保持有符号i32，图片模式不会把它变成文本；双态按钮 val=1 时使用其第二状态图片。这四类与文本、普通按钮的 sta=3 透明背景仅对 X 系列开放，T0／K0 按已确认的系列规则限制为 0～2。

```powershell
python examples/prepare_additional_picture_modes_acceptance.py --font path/to/ui.zi
```

此脚本复用上一批生成器，输出在 `target/acceptance-picture-modes-more/`，含 MorePic／MoreCrop／MoreColor／MoreX90 四份工程，每份 number／xfloat／dual／scroll 四页。前三份为 K035 的图片／切图／单色保留，最后一份为 X543 90° JPEG 和 ID 重排，双态按钮初值为 1。每份实际执行10条独立作业；只读取指定 ZI，图片均由代码生成，已有目录不会覆盖。


六类文字控件的透明背景 sta=3 不填充控件背景，保留字体、颜色、边框及其他属性；其下方页面或控件的画面可透出。它不要求 pic／picc 等背景图片；显式声明的非活动引用仍保留并解析。透明背景是控件的显示模式，与源图片是否带透明通道、媒体编码的透明度位数是不同配置，不会隐式生成透明图片。

页面自身 ID0 的图片引用也保存为 ImageId 重定位，复用现有资产表、编码及独立链接。页面注册定义与普通控件分开，不将 page 作为可添加的普通控件，因此普通控件类型数仍为 30。target 数据修订更新后，旧 target.json／页面目标需要从源码重新生成；分发版本号不因此变化。

生成页面背景与透明背景验收工程：

```powershell
python examples/prepare_page_background_acceptance.py --font path/to/ui.zi
```

输出位于 `target/acceptance-page-backgrounds/`。PageBgK 为 K035 的图片／单色／无背景三页；Transparent0／Transparent90 为 X543 两个方向的六类文字控件，每页用图片页面背景核对透明显示，并保留非活动图片引用。源图片由代码生成，只读取指定 ZI，无需 FFmpeg 或虚拟 SD。三份分别实际执行6／12／12条独立作业，输出与统一入口相同；已有目录不会覆盖，可用 --output 指定新目录。详细属性和显示预期见输出 README。

指针／波形背景和二维码 LOGO 验收：

```powershell
python examples/prepare_graphics_modes_acceptance.py
```

输出位于 `target/acceptance-graphics-modes/`，包括 K035 的 GraphicPic／GraphicCrop／GraphicColor 和 X543 90° 的 GraphicX90。每份 gauge／wave／qrcode 三页：前三份验证图片、切图、切回单色及关闭 LOGO 后保留引用；最后一份验证透明背景和图片 ID 重排。GraphicPic／GraphicCrop／GraphicX90 显示中心 LOGO，GraphicColor 关闭 LOGO 但保留 pic。所有图片均由代码生成，无需字库、已有 HMI、FFmpeg 或虚拟 SD；每份实际执行9条独立作业比较 HMI，已有目录不会覆盖。显示预期和详细属性见输出 README，动态曲线和扫码不属于本批验收内容。


五类 X 系列控件共用 ResourceRule：sta=2 使用 pic，sta=0 使用全屏 picc；sta=1／3 不要求背景图片，显式声明的非活动引用仍保留和解析。背景模式不改写选项、当前值、文本缓存、文件浏览器图标、数据文件结构或运行时依赖。选中项、表头、弹出列表、边框等仍按各自属性及原生行为绘制，透明背景不代表这些部分的颜色也被清除。文件浏览器的 pic1／pic2 与背景 pic／picc 是不同用途的图片引用，前两者在任何模式下都须绑定有效图片，省略时使用内置图标，不能设为none。

批量生成五类控件的四种背景验收工程：

```powershell
python examples/prepare_x_backgrounds_acceptance.py --font path/to/ui.zi
```

输出位于 `target/acceptance-x-backgrounds/`，共两份 HMI、20 页，每页一个控件。XModes0 为 X543 0°，前五页图片、后五页单色；XModes90 为90°，前五页切图、后五页透明，并反转图片ID。每组依次为下拉框、选择文本、滑动文本、文件浏览器、数据记录，核对后每份编译一次即可。背景图片和图标均由代码生成，只读取指定ZI；无需FFmpeg、虚拟SD或预置.data。每份实际执行18条独立作业，输出与统一入口相同；已有目录不会覆盖，可用 --output 指定新目录。完整页面清单、图片ID和显示预期见输出 README。


## 动画、视频和音频的外部播放

这三类控件使用同一套规则：from=0 使用内嵌 vid，from=1 使用外部 path，默认 from=0。vid 是本类内部资源的 ref／id 引用；path 可写运行时路径字符串，也可写对应 external 资产的 ref。动画只能绑定动画，视频只能绑定视频，音频只能绑定音频，不能用外部资产冒充内部 ID。

```toml
# 资产：仍以原始视频为输入，编码后生成独立 VIDEO 文件。
schema = "hmi.asset/1"
kind = "video"
source = "movie.avi"
[delivery]
mode = "external"
path = "sd0/media/movie.video"
```

```toml
# 页面控件：路径绑定到上面在工程资产表中声明的 movie_out。
[[objects]]
name = "v0"
type = "video"
properties = { x = 20, y = 20, w = 160, h = 80, from = 1, path = { ref = "movie_out" }, en = 0 }
```

from=0 时 vid 必填且不能为 none；path 可省略（默认空字符串）或保留已有路径配置。from=1 时 path 必须是非空字符串或有效 ref；vid 可省略／none，写入原生65535，也可保留有效内嵌引用。切换 from 不清空另一套配置，显式非活动引用仍按当前工程解析；资产不存在、类别不符或部署方式错误时仍报错。播放状态 en、loop、速度、时间初值、几何和事件也不会因为切换播放源而自动修改。

三类 delivery 默认 embedded；external 必须指定 sd0/ 下的安全输出路径，后缀分别为动画 .gmov、视频 .video、音频 .wav。外部动画输出的是运行 GMOV，编辑源仍在 .hma 内保存，不部署到 SD，也不进入 HMI。VIDEO/WAV 复用相同运行字节和校验包装，外部输出不会额外占本类 ID，重排混合资产也不会使外部项挤占内部编号。

例如 HMI 输出为 dist/Panel/Panel.HMI，配套文件同时输出到 dist/Panel/sd0/media/movie.video。用户将 sd0/ 中的内容按相同目录结构复制到 SD 卡或上位机虚拟 SD；工具不自动修改用户的运行环境。只写 `path="sd0/existing.video"` 时，仅记录运行时需求，不读取、生成或检查构建机同名文件；用户须提供已转换的运行媒体，不能把 AVI／GIF 改后缀代替编码。路径字符串还可指向设备支持的 ram/ 等位置；自动部署输出目前仍限 sd0/。

runtime_files 记录当前启用的初始路径及最终页面／控件 ID，kind 为 external_animation／external_video／external_audio，generated 表示工程是否包含对应输出。from=0 中保留的路径不计作该控件的当前文件需求；显式声明的外部资产即使暂未使用也会输出，其 users 可以为空。同一路径被不同媒体类型或数据记录占用会报错；输出路径还会检查大小写和父子冲突。多文件发布沿用配套文件先写、HMI最后替换的流程，不承诺整包事务。

.hmo 的资源条件从图片专用规则扩展为 ResourceRule，路径也携带预期媒体类别；编译、目标检查和链接共用。链接器只读取自包含.hmo/.hma，不需要原始GIF／AVI／WAV或FFmpeg。外部路径绑定不代表已经验证运行时播放、试听或文件存在，相关行为按实际运行环境验收。

批量生成三类播放源的验收工程：

```powershell
python examples/prepare_external_playback_acceptance.py --ffmpeg-dir path/to/ffmpeg/bin
```

输出在 `target/acceptance-external-playback/`，MediaOut0／MediaOut90 为 X543 的0°／90°工程，每份9页，合计18个组合。每份均包含三类内部播放、外部播放保留vid、直接路径且不提供vid；每份实际执行17条独立作业比较HMI和三个配套文件。所有输入素材由代码生成，无需已有HMI或字库；生成视频需要FFmpeg，打开验收文件无需本工具后端。先将输出sd0/中的ext0／ext90目录复制到上位机虚拟SD根目录，再核对各页并分别编译一次；完整路径及预期见输出README。脚本不自动修改虚拟SD，已有输出目录也不会覆盖。


## 已编码文件、原生资产包与原始编码复用

五类资产都可以通过 input 指定入口。省略时仍为 source；`source` 路径相对于当前资产描述文件。source 是需要处理的原始输入，original 只用于有条件复用，不能在 source 丢失时充当替代品。

| 输入方式 | 权威输入与行为 |
|---|---|
| source | 图片、ZI、GIF、受支持视频、WAV；按描述中的参数编码或转换 |
| encoded | 已知编码文件；校验签名、结构和目标兼容性后保留运行字节或重新封装，不重新压缩 |
| native | 本工具支持的.hma资产包；校验成员及摘要后保留包内资源，不要求原图或FFmpeg |
| source加original | source仍必需；摘要、有效请求记录和目标兼容性都一致时复用附件，否则按source重新处理 |

encoded当前支持JPEG分支的XI、已匹配目标方向的ZI、VIDEO、已格式化的PCM16 WAV，以及仅供external部署的运行GMOV。XI可作为内嵌图片或外部XI：内嵌时不伪造.is编辑源；XI没有可靠的原始质量参数，因此jpeg.quality记为null。VIDEO可保留音轨，backend记为null表示本次没有调用FFmpeg。encoded ZI不自动旋转，需要方向转换时使用source输入。

运行GMOV与编辑源不是同一种文件。encoded运行GMOV当前要求delivery.external，HMA只保存运行成员；内嵌动画仍使用GIF的source入口，或含运行资源及编辑源的native包。不能把缺少编辑源的运行文件默认为完整的可编辑动画工程。encoded输入不接受encoding、resize、clip、audio或original等处理参数，即使填写的数值与当前内容相同也不隐式重新编码。

```toml
schema = "hmi.asset/1"
kind = "image"
input = "encoded"
source = "logo.xi"
```

```toml
schema = "hmi.asset/1"
kind = "video"
input = "native"
source = "../.preserve/movie.hma"
```

native只接收当前可识别的资产包及编码变体，并核对类别、格式、方向和型号；不能用这个字段绕过目标限制。缺省保留包内内嵌／外部方式；允许为外部包显式改delivery.path，载荷和成员不变。native不切换内嵌／外部方式，若需要重新封装请使用相应source或encoded入口。包内有预览或编辑源，也不会因此被当作另一份隐式输入。

```toml
schema = "hmi.asset/1"
kind = "image"
source = "logo.png"
original = "../.preserve/logo.hma"
[encoding]
codec = "jpeg"
quality = 80
```

original适用于五类source资产。新的source编码结果在HMA中记录source_request，保存规范化请求及格式／编码／方向条件，不保存本机源路径、资产符号或整个页面目标摘要。改文件名但内容和请求不变可以继续复用；修改源内容、相关参数或目标条件时重新处理。图片codec=auto按目标展开，JPEG缺省质量按80比较；音频不把屏幕方向作为复用条件。编码后端的版本变化本身不会强制替换已确认的原始编码，也不会把复用结果描述为由新后端重新编码。

original附件必须存在且有效，类别也必须一致；源文件丢失或附件损坏会报错，不读取隐藏缓存回退。旧包没有source_request时，无法据此证明请求一致，改走正常source处理。original是显式依赖，复用结果的输入仍包含描述、source和附件三项；重新编码时也保留附件依赖。视频参数未变时可以不加载FFmpeg直接复用；发生变化而需要重新编码时，仍须提供可用后端。

hmi-media probe/encode的JSON结果增加input_action：source表示实际处理源文件，encoded表示封装编码文件，native表示保留原生包，original表示复用原始编码。普通命令行也会使用对应文案。HMA中的图片质量和视频后端允许为null，以表示输入中无法可靠恢复该信息或本次未使用该后端，不用猜测值填充。

生成三种媒体输入的批量验收工程：

```powershell
python examples/prepare_media_inputs_acceptance.py --font path/to/ui.zi --ffmpeg-dir path/to/ffmpeg/bin
```

输出位于 `target/acceptance-media-inputs/`，Inputs0／Inputs90分别为X543的0°／90°，各15页，合计30组合；每份按encoded／native／original三组对照图片、字库、动画、视频、音频。准备种子视频需要FFmpeg，随后统一构建及每份32条独立作业均在无可用FFmpeg的环境中完成，original组明确复用原始编码。先把输出sd0中的input0／input90目录复制到上位机虚拟SD根目录，再核对各页并每份编译一次；完整预期见输出README。仅使用指定独立ZI，其余素材由代码生成，不读已有HMI；已有输出目录不会覆盖。


## 自动图标与自定义文件类型图标

文件浏览器省略pic1／pic2时，编译器在HMO中声明受控图标依赖；驱动程序在显式资产之后追加实际需要的内置图标。多个浏览器共用同一套，单独显式指定一项只补另一项，两项都显式指定则不额外加入。check在内存中检查图标，不生成.build、PNG或源描述文件。plan将这些需求展开为独立hmi-media builtin作业，链接器仍只消费已经生成的HMO/HMA。

内置图标属于工具数据，随程序分发；默认是黄色文件夹和蓝白文件图标，尺寸64×64，保留透明度。保留符号使用__hmi_命名空间，不能在工程资产表或源码ref中占用。link-map会标记implicit=true及builtin标识；在显式资产末尾增加资源不会悄悄改写原有数值引用。更新控件默认配方后，旧target/HMO需从源码重建，分发版本号不因此变化。

自定义文件图标使用原生属性名vvs2，可写有序结构化数组：

```toml
[[objects]]
name = "fb0"
type = "file_browser"
properties = { x = 20, y = 20, w = 440, h = 200, font = { ref = "ui" }, vvs2 = [
  { pattern = "*.txt", pic = { ref = "text_icon" } },
  { pattern = "*.log", pic = { ref = "log_icon" } },
] }
```

也可写原生数值字符串，例如 `vvs2="*.txt/2*.log/3"`。两种形式都按当前工程解析：ref跟随最终符号和ID重排，id或原生串中的数值始终表示当前图片列表中的那个ID，不跟踪历史。映射只接受有效内嵌图片，不能为none、65535或外部资产；文件夹继续使用pic1，未匹配的文件继续使用pic2。

当前pattern支持*.后缀，后缀使用ASCII字母、数字、下划线、连字符或点；同一后缀不允许重复（按ASCII大小写不敏感检查）。不把它解释为本机路径或本机文件匹配表达式，也不会据此读取目录。省略vvs2或空数组表示没有自定义映射。

原生字符串容量为64字节，vvs2_m保留原生64；编译器以0作图片ID占位，HMO保存每项pattern及引用。链接器解析最终ID后生成字符串并再次核对容量，ID变成两位或更多位时也会计入。超限报错，不截断、不删除后缀，也不覆盖旧HMI。HMO中即使映射为空也必须保留重定位声明，避免绕过引用或容量检查。

自行组合工具时，可通过hmi-compile inspect查看implicit_assets，再生成所需图标包：

```powershell
hmi-media builtin browser-folder --target target.json -o folder.hma
hmi-media builtin browser-file --target target.json -o file.hma
```

独立链接描述将这些包绑定到inspect报告的保留符号，放在显式资产之后；建议直接执行hmi plan导出的jobs.json，保持相同顺序。链接器不会临时补编码，也不通过读取用户机器上的原HMI寻找图标。

生成图标批量验收工程：

```powershell
python examples/prepare_browser_icons_acceptance.py --font path/to/ui.zi
```

输出位于 `target/acceptance-browser-icons/`，Icons0／Icons90分别为X543的0°／90°，各五页，对照自动补齐、只显式指定一项、全部显式指定、符号映射和原生数值映射。图片顺序在90°反转，默认图标仍固定追加；每份实际执行16条独立作业，其中两条builtin作业生成共享图标，HMI与统一入口相同。先将输出sd0中的browser0／browser90复制到上位机虚拟SD根目录，再编译并在模拟器查看文件列表。页底左右箭头可循环切换；具体颜色、ID和示例文件见输出README。脚本仅需独立ZI，不读已有HMI或内部素材，也不自动写虚拟SD；已有输出目录不会覆盖。


## 拖动、不透明度和加载特效

24类可视控件共用drag、aph、effect，不需要为每个类型重复定义。类型是否开放这些属性由注册表显式声明；热区、变量、定时器、触摸捕捉、文件流和音频不开放。页面只开放effect，坐标和尺寸仍由目标固定，不因此允许拖动或设置页面aph。

| 属性 | 默认值及当前范围 | 含义 |
|---|---|---|
| drag | 默认0，0／1 | 是否允许拖动 |
| aph | 默认127，0～127 | 不透明度；0完全透明、127完全不透明，不等于背景sta=3 |
| effect | 默认0，0～8 | 加载特效；0立即显示，1～8为上／下／左／右／左上／右上／左下／右下飞入 |

X543按通用范围开放；T0／K0依照官方系列说明将三项固定为0／127／0。缺省配置与此前原生字段保持一致，显式设置不改写颜色、style、几何、图片编码、指针形状或事件。aph改变控件整体显示的不透明度，不会修改共享PNG的alpha或触发图片重新编码。

```toml
[[objects]]
name = "p0"
type = "picture"
properties = { x = 20, y = 20, w = 64, h = 64, pic = { ref = "tile" }, drag = 1, aph = 64, effect = 3 }
```

页面effect当前也开放0～8（X系列），但开机第一个页面不显示页面加载特效；需要观察时从另一个页面跳转进入。官方说明还指出：初始effect为0时，运行指令不能改成其他特效。当前本工具检查源码初值及目标条件，尚不完整分析运行脚本中改变effect的语义；静态画布也不能代替模拟器或硬件动态验收。

公共属性通过Definition.all_properties统一参与源码检查、目标反解、编译和链接防御性检查。原生布局仍保留相同字段顺序与宽度；未开放的类型继续使用原来的常量值。指针format／left／wid仍保持创建值0／0／2，符号或合法范围未确认时不按字节宽度猜测开放。

生成公共属性组合验收工程：

```powershell
python examples/prepare_presentation_acceptance.py --font path/to/ui.zi
```

输出位于 `target/acceptance-presentation/`，Visual0／Visual90分别为X543的0°／90°，各六页，包含黑色开机页、拖动、不透明度、八种控件特效、页面特效及组合配置。页底箭头循环切换，资源全部内嵌，不需要虚拟SD或FFmpeg；各编译一次后在模拟器核对动态表现。图片由代码生成，仅需独立ZI，不读已有HMI。每份实际执行14条独立作业，HMI与统一入口相同；已有目录不会覆盖，完整参数及预期见输出README。


## 已有工程导入与原始数据附件

统一入口与独立解析器共用实现：

```powershell
hmi inspect panel.HMI --json
hmi import panel.HMI -o panel-source
hmi-parse inspect panel.HMI --json
hmi-parse export panel.HMI -o panel-source
hmi build panel-source
```

当前支持ver21234、UTF-8、已有型号配置及匹配目标画布几何的工程，包括页面、变量、热区、定时器、复选框、单选框、单色进度条、单色指针／波形、无LOGO二维码、触摸捕捉、文件流及仅声明运行时外部路径的受支持控件。模型仍按HMI保存的型号代码识别，当前仅能匹配已内置配置，不把四个配置当作格式的型号上限。inspect可查看结构与资源；export可导入已识别的图片、ZI、动画、视频和音频变体，未知编码或损坏资源会指出具体成员，不丢弃后输出残缺工程。

导入目标必须不存在或为空目录。工具先检查完整输入并在同级暂存目录生成文件，成功后再放到目标目录，不覆盖已有工程。输入允许活动资源间有未使用区及删除历史，只要有效目录、活动区间和页面校验正确；新容器会重新排列有效内容，不保留删除项、无用填充或旧分配历史。

```toml
# hmi.toml及独立link.toml均可显式声明；路径相对于所在清单。
[original]
bundle = ".preserve/origin.hpo"
```

```toml
# 页面原始记录只定位附件，不是当前ID或稳定对象key。
schema = "hmi.page/1"
name = "home"
original_record = "pages/0"

[[objects]]
name = "counter"
type = "variable"
original_record = "pages/0/objects/1"
properties = { sta = 0, val = 42, txt = "newtxt", txt_maxl = 10, vscope = 0 }
```

导出会完整写出已公开属性，包括非活动值、各已知事件和全局脚本。页面与事件使用编号目录，路径不决定当前ID；sources及objects数组才决定顺序。已知属性始终以当前源码和缺省规则为准：删除val就恢复创建默认值，不能从附件取回旧值。original_record不参与名称、脚本或资产引用解析；重命名和重排不会自动改脚本。

HPO保留原始对象记录、页头与索引未知字、主工程头及额外活动资源，不嵌入整个原HMI。对象中尚未开放的常量字段、额外属性和未知记录会保留；已公开字段、名称、ID、几何派生值、事件代码则按当前源码生成。类型不一致、定位不存在、附件损坏或缺失会报错；未知事件槽、尚未接入的布局或几何差异也会指出具体限制，不假装已支持。

编译产物携带所需原始对象和页面数据，并声明附件摘要，独立链接不能遗漏或换成另一个附件。hmi-compile page可显式加`--original origin.hpo`；hmi plan会给页面作业添加该参数，并复制original.hpo供link.toml使用。持有计划中的HMO/HMA及HPO后，不需要原HMI或源码即可链接。原始附件是工程输入，不是可以随派生缓存清理的文件。

页面头、索引偏移／长度、当前名称、主引用表、资源目录和CRC按当前布局生成；目录项未知字随所属页面或额外资源保留。额外活动资源不会因为未被普通控件引用就删除，原始不识别的引用类型也随资源保留。重建核对的是有效内容及结构，不要求整个文件逐字节一致。媒体资产作为显式原生包导入，更多未识别的原始变体仍待接入。

生成首批导入验收工程：

```powershell
python examples/prepare_import_acceptance.py
```

输出位于 `target/acceptance-import/`，最终只需打开Import0／Import90两份X543工程，各五页。脚本先创建无媒体HMI，分别通过统一入口和独立解析器导入；未修改时对照全部活动资源，再编辑名称、数值、缺省属性和控件顺序后重建。每份七条独立作业的输出与统一入口一致。页底蓝色／绿色条块可辅助切页，核对后各编译一次；无需字库、FFmpeg或虚拟SD，详细预期见输出README。原始输入和未修改重建结果作为参考放在子目录，不是额外必验文件；已有输出目录不会覆盖。


## 已有工程的五类媒体导入

导入以主引用表确定各类逻辑ID，不能从资源文件名的数字推断ID。已有.i配对同名主干的.is，.gmov配对.gmovs；运行资源及已有编辑源原样进入.preserve/assets下的HMA。每个资产有普通assets/*.asset.toml描述，input为native，source显式指向原生包。图片、字体、动画、视频、音频的各自顺序保持，控件及非活动字段中的有效ID转为当前资产ref，自定义图标映射也转为结构化引用。

未修改重建不解码再编码，不猜测JPEG质量或原转换请求。能提取的图片编辑源、GIF、ZI、VIDEO、WAV另存assets/view供查看；这些文件不是native描述的权威输入，修改它们不会改变构建，也不会在HMA损坏时充当回退。要改媒体内容，需要显式切换source并提供相应参数；本批默认导出不推断是否具备original条件复用所需的原始请求。

导入包保留各成员的目录未知字，重建时随新的文件名写回。HPO将已交给资产包管理的原资源标为managed，不重复保存它们的载荷或恢复旧引用；用户删除控件和资产后，原附件不能把已删除资源重新带回来。额外未交由资产管理的活动资源仍按原始附件规则保留；与当前输出发生实际名称冲突时明确报错。

当前支持K/T的RGB565及X的JPEG图片、规定的UTF-8 ZI v6、已接入JPEG运行GMOV／GIF编辑源、VIDEO及其PCM音轨、格式化PCM16 WAV。标准RGB／RGBA／调色板等合法PNG头型及JPEG编辑源按包装、尺寸及结构校验后保留。原来没有.is或.gmovs就保持缺失：native包可保留现有内嵌运行GMOV，但不会为它伪造GIF、原始帧映射或转换参数；这不改变encoded运行GMOV只开放external的规则。

外部控件仅导出原始路径字符串；导入器不会去SD、ram或用户机器的虚拟SD寻找文件。隐藏字段若含当前引用表中不存在的ID，也会报出实际引用问题，不把它静默改为none。型号、方向和编码兼容性仍必须通过检查；未知编码分支及不支持的源变体会具体指出资源名。

生成媒体导入验收工程（输入使用此前已验收的Inputs两份媒体HMI）：

```powershell
python examples/prepare_import_media_acceptance.py --inputs path/to/Inputs0.HMI path/to/Inputs90.HMI
```

配套外部GMOV默认取输入HMI同级sd0目录，也可用--sd0明确指定。输出位于 `target/acceptance-import-media/`，最终MediaImport0／MediaImport90各15页。脚本先对照未修改重建的逻辑运行载荷、已有编辑源和目录未知字，再修改图片顺序、文字、坐标、循环和控件名；每份14个内嵌原生包、31条独立作业，HMI与统一入口相同，全部在无可用FFmpeg环境中完成。外部动画可沿用上一批已部署的input0／input90，详细预期见输出README。已有输出目录不会覆盖，roundtrip参考放在子目录，不是额外必验文件。

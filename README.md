# hmi-compiler

用 Rust 将陶晶驰 USART HMI 的 `ver21234` 工程导出为可读目录，编辑后回编为 `.HMI`。

当前版本支持已有工程的属性、事件脚本和全局代码修改。无修改回编逐字节还原；原始容器、删除记录、未知字段及未修改资源均保留。目标为 USART HMI 可打开的工程文件，不生成 `.tft`。

已通过自动化测试。**官方编辑器打开、编译和模拟器验收仍待人工执行**，不能以离线校验代替。

## 使用

项目使用 Rust 2024 edition；当前在 Windows、rustc 1.99 上验证。

```powershell
cargo build --release --locked

target/release/hmi-compiler.exe inspect project.HMI --pages
target/release/hmi-compiler.exe export project.HMI -o target/workspace

# 编辑 target/workspace 中的页面 TOML、事件脚本或 program.s
target/release/hmi-compiler.exe check target/workspace
target/release/hmi-compiler.exe build target/workspace -o target/rebuilt.HMI
target/release/hmi-compiler.exe diff project.HMI target/rebuilt.HMI
```

将 `project.HMI` 替换为自己的工程路径。开发时也可用 `cargo run --locked -- <命令>`。

| 命令 | 参数 |
|---|---|
| `inspect <FILE>` | `--pages`、`--resources`、`--json` |
| `export <FILE> -o <DIR>` | `--force`、`--json` |
| `check <DIR>` | `--deny-warnings`、`--json` |
| `build <DIR> -o <FILE>` | `--force`、`--deny-warnings`、`--json` |
| `diff <OLD.HMI> <NEW.HMI>` | `--json` |

退出码：`0` 成功，`1` 校验或执行失败，`2` 参数错误。`diff` 发现差异仍返回 `0`。

`inspect` 会尽可能输出损坏文件的统计和诊断；其余命令要求容器及工程严格有效。主备目录必须一致、CRC 正确，资源必须连续覆盖数据区。首版不自动修复错误目录。

默认不覆盖输出。`export --force` 重新生成工具管理的文件，保留用户额外添加的文件；已有工作目录必须具有有效的原始元数据。导出先在相邻临时目录中完成，失败时不会发布半个项目。`build` 在内存中构建并重新解析校验，最后通过临时文件发布输出；不允许覆盖项目输入文件。

## 可读项目格式 v1

```text
workspace/
├── project.toml
├── program.s
├── pages/
│   ├── p000_page0/
│   │   └── page.toml
│   └── p001_settings/
│       ├── page.toml
│       └── events/
│           └── o001_b0.up.s
├── assets/
│   ├── images/                 # 从活动 .is/.ib 提取的 PNG，只读
│   └── fonts/                  # 活动 .zi 原始字库，只读
└── .hmi/
    ├── manifest.toml           # 版本、摘要、页面/对象映射及生成文件清单
    └── container.bin           # 完整源容器快照
```

`.hmi` 是完整项目的一部分，应一起保存和版本管理。回编不依赖原 `.HMI` 文件的位置。素材封装及其他未知资源保存在快照中，不重复拆出全部原始数据。

目录名中的数字只是导出时生成的文件名。页面顺序以 `project.toml` 中的列表为准，对象顺序以页面内数组为准；首版禁止改变这些顺序。资源名中的数字不直接代表页面 ID。

工程描述示例：

```toml
format_version = 1
encoding = "utf-8"
pages = [
    "pages/p000_page0",
    "pages/p001_settings",
]
```

格式版本、编码和页面列表首版只读。修改 TOML 排版、注释和表内字段书写顺序不影响回编；未知 TOML 字段报错，避免把拼写错误静默忽略。

统一使用 TOML 1.1，控件采用多行内联属性表，以减少重复表头。项目仍处于格式调整阶段，暂不承诺历史导出格式的兼容性。

页面描述的结构如下，实际导出会保留更多属性，请在原文件上编辑，不要用此片段替换整页：

```toml
name = "settings"

[properties]
w = 480
h = 320

[events]
# 空事件没有文件。填写已有 load 槽位时，在此添加引用：
# load = "events/page.load.s"

[[objects]]
name = "b0"
type = "button"
properties = {
    x = 20, y = 20,
    w = 100, h = 40,

    txt = "确定",
    txt_maxl = 6,
}
events = { up = "events/o001_b0.up.s" }
```

页面自身对象单独表达为顶层 `properties` 和 `events`，不包含在 `objects` 数组中。控件 `id`、`objname`、`type` 不重复出现在属性表中；对象名称和类型由 `name`、`type` 展示。`endx/endy` 为派生字段，不导出为独立编辑入口。

控件属性按几何、文本、外观、状态分组，其余字段按名称排序；只调整排版，不删除属性。空事件映射省略。导出更新只作用于新导出的文件，不自动改写已有项目；重新导出时使用新目录，或明确使用 `--force`。

### 可以编辑的字段

仅允许修改原对象已有、字节宽度和类型已确认的字段：

| 字段 | 规则 |
|---|---|
| `txt` | UTF-8 文本，编码后的字节数不得超过 `txt_maxl` |
| `txt_maxl` | u16 字节容量，不自动扩大 |
| `x/y/w/h` | u16；修改几何时尺寸必须大于零，端点不能溢出；同步更新 `endx/endy` |
| `vscope`、`en` | 0 或 1 |
| `font` | 已有字库的引用顺序 ID |
| `pic/pic2/picc/picc2/pic1/picc1/bpic/ppic` | 已有图片的引用顺序 ID，允许 `65535` 表示无图片 |
| `bco/bco2/bco1/pco/pco2/borderc` | 原有两字节颜色值 |
| `tim` | 原有 u16 值 |
| `val` | 按已验证的控件类型分别处理，见下表 |

| 控件 | `val` 二进制表示 |
|---|---|
| 变量 `0x34` | i32，`-2147483648..2147483647` |
| 滑块 `0x01` | u16 |
| 按钮、进度条、双态按钮、单选框、复选框、触摸捕捉 | u8 |

其他已识别属性以可读数值展示但只读；未知或宽度不符的值显示为 `raw:<十六进制>`，仍只读。未在样本验证过的类型不开放猜测性的 `val` 写回。更改名称、类型、增删属性、增删控件、重排或增删页面均会报错。

属性读写规则以已验证的二进制布局和[陶晶驰官方文档](http://wiki.tjc1688.com/start/index.html)为依据。官方文档确定语义和数值范围，不用于推测未知二进制布局。

### 事件和全局代码

事件键对应原文件中的已有槽位：`down`、`up`、`load`、`loadend`、`unload`、`timer`、`slide` 等。工具按实际槽位校验，不为控件凭空创建事件。

- 事件路径相对于所属页面目录，必须使用 `.s` 后缀。
- 为已有空槽位添加引用和文件，可增加代码。
- 删除事件引用或将文件清空，表示清空该槽位；遗留但未引用的文件不参与构建。
- 保留引用但删除文件属于错误。
- 事件文件接受 LF、CRLF；保留空格、缩进、注释和空行，不裁剪文本。
- 空文件为零行；`\n` 为一条空行；`code\n` 为一条代码；`code\n\n` 为一条代码和一条空行。
- 事件导出使用 LF，未改变的行记录沿用原始数据。`program.s` 按文件实际字节写回，原始 CRLF 不自动转换。
- 不展开脚本中的 `\r` 等转义，也不自动修改具名或数字 ID 引用。

所有项目引用必须是普通相对路径；拒绝 `..`、绝对路径、反斜杠、符号链接及 Windows 目录联接。

### 校验与差异

`check` 会检查原始快照摘要、元数据映射、只读素材、属性范围和脚本文件。PNG 预览和 `.zi` 均不支持首版替换，修改后明确报错，不会被忽略。

脚本检查跳过字符串和注释，识别具名属性访问、常量页面/控件 ID，并对动态名称组访问报告无法静态确认。此检查为辅助警告，不是完整语法检查；方法调用、宏、系统变量和复杂表达式仍需官方编译器确认。`--deny-warnings` 可将警告视为失败。

`diff` 分别返回：

- `byte_identical`：整个文件是否相同。
- `active_resources_equal`：全部活动资源名称和内容是否相同。
- `container_changed`：容器文件字节是否发生变化。
- `changes`：页面、对象顺序、属性、事件、全局代码和原始资源/未知元数据的变化。

因此原生清理历史记录后的文件可能“文件不同，但活动资源相同”。

所有 JSON 输出采用固定信封：

```json
{
  "schema_version": 1,
  "ok": true,
  "result": {},
  "diagnostics": []
}
```

诊断具有 `severity`、`code`、`message`、`location`。脚本警告包含文件、行号及页面/对象上下文；执行错误包含输入路径和具体错误说明。命令参数错误在指定 `--json` 时也使用同一信封。

## 测试与人工验收

```powershell
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
cargo fmt --all -- --check
```

公开测试使用代码生成的合成数据，在临时目录中运行，不依赖外部工程文件。

验证真实工程时，可在本地执行导出、编辑、回编，再使用 USART HMI 打开、编译并检查模拟器行为。记录编辑器版本、生成文件摘要和验收结果；离线结构与 CRC 校验不能替代官方软件验收。

## 后续能力

页面和控件的结构编辑、图片转换、字库生成、其他工程编码、其他容器格式，以及从零创建工程均不在当前写回范围内。原始数据保真、可读目录和构建流程可以作为后续扩展基础。

## 许可证

本项目代码和文档采用 [MIT 许可证](LICENSE)。工具处理的用户工程及素材不属于本项目的授权内容，其权利归各自权利人所有。

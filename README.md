# HMI Compiler

将陶晶驰 USART HMI 工程导出为便于阅读和编辑的文件，再根据修改后的文件生成 `.HMI` 工程。

支持修改已有控件的属性、事件脚本和全局代码。生成的工程可继续使用 USART HMI 打开、编译和下载。

**注意：** 当前版本基于 USART HMI `v1.68.2` 及文件版本 `ver21234` 验证。使用请保留原始 HMI 文件，避免文件损坏导致项目无法打开。

[使用指南](#使用指南) · [从源代码构建](#从源代码构建) · [许可证](#许可证)

## 使用指南

### 快速开始

请从 Github Releases 中获取预构建的可执行程序；若要进行开发或编译，参见[从源代码构建](#从源代码构建)。

常用命令示例：

```powershell
# 本示例假定 hmi-compiler.exe 所在的目录已添加到 PATH 变量中
# 若未添加，请用可执行文件的具体路径代替 hmi-compiler，如：.\hmi-compiler.exe

# 1. 查看 HMI 的页面和校验信息
hmi-compiler inspect project.HMI

# 2. 解析 HMI 文件，并导出到 workspace 目录
hmi-compiler export project.HMI -o workspace

# 3. 对工程项目执行编译检查（不进行编译）
hmi-compiler check workspace

# 4. 编译并生成 HMI 文件
hmi-compiler build workspace -o output.HMI

# 5. 对比两个 HMI 文件
hmi-compiler diff project.HMI output.HMI
```

注：`hmi-compiler build` 仅生成 `.HMI` 文件，仍需在 USART HMI 中构建并编译出 `.TFT` 文件。

### 命令说明

| 命令 | 用途 |
|---|---|
| [`inspect`](#inspect) | 查看 `.HMI` 文件的工程信息、页面、资源及校验结果 |
| [`export`](#export) | 解析 `.HMI` 文件并导出为易读的项目 |
| [`check`](#check) | 进行编译检查，但不进行编译 |
| [`build`](#build) | 构建项目，编译出 `.HMI` 文件，此命令为增量构建 |
| [`rebuild`](#rebuild) | 构建项目，编译出 `.HMI` 文件，此命令为全量构建，将重新构建完整的项目 |
| [`clean`](#clean) | 清理构建缓存 |
| [`diff`](#diff) | 对比两个 `.HMI` 文件中的属性、脚本和资源变化 |

#### `inspect`

读取一个 `.HMI` 文件，显示文件版本、大小、页面与控件数量、事件数量、活动及已删除资源数量，并检查文件结构和校验结果。可用于在导出前确认工程是否能被识别，或排查文件损坏。

**语法及参数说明：**

```text
hmi-compiler inspect <FILE> [--pages] [--resources]
```

| 参数 | 说明 |
|---|---|
| `<FILE>` | 必填，`.HMI` 文件的路径 |
| `--pages` | 可选，列出页面 ID、页面名称、对应资源名和对象数量 |
| `--resources` | 可选，列出资源名称、文件内偏移、字节数及删除状态，包含已删除资源 |

遇到错误时，解析器仍会尽可能显示仍能读取的信息，并返回失败状态；此命令不会修复文件。

#### `export`

解析 `.HMI` 文件并导出为易读的项目。页面和控件属性将保存在每个页面的 TOML 文件中，事件代码将保存为独立脚本，重构建所需的原始数据也会一并保存。

**语法及参数说明：**

```text
hmi-compiler export <FILE> -o <DIR> [--force]
```

| 参数 | 说明 |
|---|---|
| `<FILE>` | 必填，`.HMI` 文件的路径 |
| `-o, --output <DIR>` | 必填，导出目录 |
| `--force` | 可选，允许导出到已有项目目录。**警告：** 此操作会覆盖或清理工具生成的文件，仅保留额外添加的文件，已有编辑可能被替换！ |

#### `check`

对项目执行构建检查。此命令不会进行构建和编译，仅对项目进行诊断，所以不会产生 `.HMI` 文件。
**注意：** 项目通过了此命令的检查不代表在 USART HMI 中一定能编译成功。

**语法及参数说明：**

```text
hmi-compiler check <DIR> [-Werror]
```

| 参数 | 说明 |
|---|---|
| `<DIR>` | 必填，必须为项目的根目录 |
| `-Werror` | 可选，将警告升级为错误。有警告时也会使检查失败 |

#### `build`

构建并编译项目，产生 `.HMI` 文件，此命令为增量构建。所有页面处理及最终校验通过后，才会更新目标文件，目标文件已经存在时会被覆盖。
**注意：** 项目构建成功不代表在 USART HMI 中一定能编译成功。

**语法及参数说明：**

```text
hmi-compiler build <DIR> -o <FILE> [flag]
```

| 参数 | 说明 |
|---|---|
| `<DIR>` | 必填，必须为项目根目录 |
| `-o, --output <FILE>` | 必填，生成的 `.HMI` 文件路径 |
| `flag` | 可选，警告控制标记，可用的标记见[警告控制](#警告控制) |

#### `rebuild`

构建并编译项目，产生 `.HMI` 文件。此命令为全量构建，会重新编译全部页面、检查工程并更新缓存。适合需要完整重建，或排查缓存相关问题时使用。

**语法及参数说明：**

```text
hmi-compiler rebuild <DIR> -o <FILE> [flag]
```

参数与 `build` 命令相同，参见 [`build`](#build) 。

#### `clean`

删除指定项目的构建缓存。

**语法及参数说明：**

```text
hmi-compiler clean <DIR>
```
| 参数 | 说明 |
|---|---|
| `<DIR>` | 必填，必须为项目根目录 |

#### `diff`

对比两个 `.HMI` 文件，报告页面、控件、属性、事件脚本和资源内容的变化。

```text
hmi-compiler diff <OLD> <NEW>
```

| 参数 | 说明 |
|---|---|
| `<OLD>` | 必填，作为比较基准的 `.HMI` 文件路径 |
| `<NEW>` | 必填，要与基准比较的 `.HMI` 文件路径 |

```powershell
hmi-compiler diff project.HMI rebuilt.HMI
```

### 编辑导出的项目

导出项目的目录结构如下：

```text
workspace/
├── project.toml             # 工程配置和页面列表，当前只读
├── program.s                # 全局代码，可编辑
├── pages/                   # 页面
│   └── p000_page0/
│       ├── page.toml        # 页面及控件属性
│       └── events/          # 该页面及控件的事件脚本
├── assets/
│   ├── images/              # 图片预览，当前只读
│   └── fonts/               # 字库文件，当前只读
├── .hmi/                    # 重新生成 HMI 所需的原始数据，请勿修改！
└── .build/                  # 构建时自动生成的缓存，可以通过 clean 命令清理
```

当前版本支持 `.HMI` 文件版本 `ver21234` 、UTF-8 编码的已有工程。图片和字库只能查看，暂不支持替换；也不支持从零创建工程。

#### 修改控件属性

在 `pages` 中找到对应页面，编辑 `page.toml`。每个 `[[objects]]` 表示一个控件，`name` 是名称，`type` 是类型，`properties` 为属性，`events` 为事件脚本的路径。

下面的例子展示了部分字段：

```toml
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

可修改的常用属性如下，具体以控件实际拥有的字段为准：

| 属性 | 用途与要求 |
|---|---|
| `txt` | 文字内容，按 UTF-8 编码后的字节数不得超过 `txt_maxl` |
| `txt_maxl` | `txt` 的字节容量，范围为 `[0, 65535]`  |
| `x/y/w/h` | 坐标和尺寸，尺寸必须大于零，计算出的端点不能超过 `65535` |
| `vscope` | 控件作用域：`0` 为私有，`1` 为全局 |
| `font` | 字库的 ID |
| `pic/pic2/picc/picc2` 等 | 图片的 ID，`65535` 表示无图片 |
| `bco/bco2/bco1/pco/pco2/borderc` | 背景、文字或边框颜色，使用原属性的整数表示 |
| `val` | 数值，按控件类型校验范围，变量控件支持有符号 32 位整数 |
| `tim/en` | 定时相关设置，`en` 只能为 `0` 或 `1` |

页面自身的属性和事件位于文件顶部的 `[properties]`、`[events]` 中。

其余只读字段应保留原值，显示为 `raw:...` 的字段也不要修改。当前版本暂不支持增删、重排或重命名页面与控件，也不支持改变控件类型或自行增删属性。

#### 修改事件和全局代码

直接编辑事件所引用的 `.s` 文件；路径相对于所属页面目录。常见事件键包括：

| 事件键 | 含义 |
|---|---|
| `down` / `up` | 触摸按下／弹起 |
| `load` / `loadend` | 页面初始化前／后 |
| `unload` | 页面离开 |
| `timer` | 定时触发 |
| `slide` | 滑块滑动 |

新增事件脚本时，需创建 `.s` 文件并在 `events` 中添加引用；删除引用或清空文件表示清空事件。编写脚本应使用 UTF-8 字符集。全局脚本在根目录的 `program.s` 中编辑。
**注意：** 请勿增加控件不支持的事件，大部分控件都只有按下/弹起事件。

### 通用选项

| 参数 | 说明 |
|---|---|
| `-h, --help` | 查看帮助，如 `hmi-compiler build --help` |
| `-V, --version` | 查看程序版本，仅用于主命令：`hmi-compiler --version` |
| `--json` | 以 JSON 输出执行结果，供脚本或其他程序读取 |
| `--color auto` | 默认值；在支持颜色的终端中启用颜色，重定向时关闭 |
| `--color always` | 强制启用颜色 |
| `--color never` | 关闭颜色 |

使用自动颜色模式时遵循 `NO_COLOR` 环境变量；显式指定 `always` 或 `never` 时，以命令参数为准。

#### 警告控制

警告默认显示，但不会阻止生成目标文件。可以按类别关闭警告，或将某类警告作为错误处理：

| 参数 | 说明 |
|---|---|
| `-W警告类别` | 启用指定类别 |
| `-Wno-警告类别` | 关闭指定类别 |
| `-Werror` | 将所有已启用的警告作为错误处理 |
| `-Wno-error` | 取消全局的“警告作为错误”设置 |
| `-Werror=类别` | 启用指定的警告类别，并将其作为错误处理 |
| `-Wno-error=类别` | 指定类别保持为警告，不会自动启用已关闭的类别 |
| `-Wall` / `-Wno-all` | 启用／关闭全部已实现的警告类别 |
| `-w` | 关闭全部警告 |

目前已实现的警告类别如下表所示：

| 类别 | 说明 |
|---|---|
| `script-dynamic` | 页面或控件 ID 由表达式计算，无法静态确认目标是否存在 |
| `script-page` | 引用的页面不存在 |
| `script-object` | 引用的控件不存在 |
| `script-property` | 控件没有被引用的属性 |
| `script-scope` | 跨页面访问私有控件 |
| `script-context` | 无法确定当前页面，或目标页面存在错误 |
| `script-page-target` | 无法静态确认跳转目标 |
| `property-resource` | 图片或字库 ID 无效 |
| `encoding` | 工程编码尚未支持 |
| `resource-layout` | 工程文件的资源布局尚未支持 |

示例：

```powershell
# 关闭动态 ID 无法确认的警告
hmi-compiler build workspace -o rebuilt.HMI -Wno-script-dynamic

# 将警告作为错误，但允许动态 ID 类别保持为警告
hmi-compiler build workspace -o rebuilt.HMI -Werror -Wno-error=script-dynamic
```

针对某个类别的设置优先于全局设置；同一项重复设置时，最后一次生效。`-w` 会关闭全部警告，即使同时使用了 `-Werror`。文件损坏、属性越界、文本超容量等属于错误，将始终阻止构建。

关闭 `encoding` 或 `resource-layout` 的提示，不会使程序获得处理这些格式的能力。未知的类别名称会作为参数错误报告。

#### 日志

构建按照页面顺序进行，若构建不成功，将显示诊断信息，例如：

```text
[ 50%] [ SUCCESS ] page0
[100%] [ WARNING ] settings.b0.events.up: [pages/p001_settings/events/o001_b0.up.s:12] 名称组使用了动态 ID，无法在静态检查阶段确定目标控件及属性是否有效 [-Wscript-dynamic]
```

使用 `--json` 时，只向标准输出（`stdout`）写入 JSON 结果，不显示普通日志。
JSON 中，`ok` 表示执行是否成功，`result` 包含命令结果，`diagnostics` 列出警告和错误，`schema_version` 表示输出格式的版本。

| 退出码 | 含义 |
|---|---|
| `0` | 命令执行成功，`diff` 发现差异也属于成功 |
| `1` | 文件、检查或构建失败 |
| `2` | 命令参数错误 |

## 从源代码构建

在开始构建前，必须安装 Rust 及其工具链，Rust 版本为 `1.89` 或者更新。不会安装 Rust？参见 [Rust 安装入门](https://rust-lang.org/learn/get-started/) 。

在项目根目录执行以下命令以编译 hmi-compiler：

```powershell
cargo build --release --locked
```

或者通过 Cargo 直接构建并运行：

```powershell
cargo run --release --locked -- inspect project.HMI --pages
```

需要检查源码修改时，可执行：

```powershell
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
cargo fmt --all -- --check
```

公开测试使用自动生成的工程数据，不需要另外提供 HMI 样本。

## 许可证

本项目代码和文档采用 [MIT 许可证](LICENSE)。

# 工程文件参考

[项目首页](../../../README.md) · [工程管理工具](../README.md) · [页面与事件](../../hmi-compiler/reference/pages.md) · [媒体输入](../../hmi-media/reference/inputs.md)

## 目录与文件

一个新建工程包含工程清单、全局代码和页面文件。媒体、事件脚本及原始附件按需添加：

```text
panel/
├── hmi.toml
├── program.s
├── pages/
│   └── home/
│       ├── page.toml
│       └── events/
│           └── confirm.up.s
├── assets/
│   ├── logo.asset.toml
│   └── images/logo.png
├── .preserve/                 # 导入工程的原始附件，属于工程输入
├── .build/                    # 构建产物和缓存
└── dist/                      # 最终HMI及配套文件
```

`hmi init` 创建的第一个页面位于 `pages/page0/page.toml`。上面的 `home` 是自定义目录示例；页面文件只要列入清单，就可以放在其他位置。

## 工程清单

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

[build]
output = "dist/Panel/Panel.HMI"

[pages]
sources = ["pages/home/page.toml", "pages/settings/page.toml"]

[assets]
items = [
    { key = "ui_font", source = "assets/ui_font.asset.toml" },
    { key = "logo", source = "assets/logo.asset.toml" },
]
```

| 字段 | 含义 |
|---|---|
| `schema` | 工程格式标识，填写 `hmi.project/1` |
| `name` | 工程名称，用于报告和默认输出路径；不替代页面名称 |
| `target.profile` | 已支持的目标型号，见[型号配置](../../../data/profiles/README.md) |
| `target.format` | HMI 文件格式，填写 `ver21234` |
| `target.encoding` | 工程编码，填写 `utf-8` |
| `target.orientation` | 屏幕方向：0、90、180 或 270 |
| `program.source` | 全局代码文件路径 |
| `build.output` | 可选的 HMI 输出路径；命令行 `-o` 优先于此项 |
| `pages.sources` | 页面文件路径数组，至少包含一个页面；顺序决定最终页面 ID |
| `assets.items` | 可选的资产数组；没有媒体时可以省略，或写为 `items = []` |
| `assets.items[].key` | 当前工程内唯一的资产符号，用于控件中的 `ref` 引用 |
| `assets.items[].source` | 资产描述文件路径，不是原图、字库或视频本身 |

路径基准如下：

- 命令行中的目录、输入和 `-o` 路径相对于当前工作目录。
- 工程清单中的路径相对于工程目录。
- 资产描述中的 `source` 和 `original` 相对于该描述文件。
- 页面中的事件脚本路径相对于页面文件所在目录。
- 控件中的 `sd0/`、`ram/` 等路径指向屏幕运行环境，见[设备路径与部署](../../hmi-media/reference/inputs.md#外部文件与设备路径)。

## 页面、控件和资产

页面声明自己的名称、属性、控件及事件。页面自身的对象 ID 为 0，普通控件从 1 开始，按 `objects` 的顺序编号。控件数组顺序也决定图层。

页面名称在工程内唯一。控件名称在所属页面内唯一，不能与该页面同名。名称使用 1～16 字节的 ASCII 字母、数字和下划线，以字母或下划线开头。

资产 `key` 使用字母或下划线开头的字母、数字、下划线或连字符；`__hmi_` 前缀留给内置资源。图片、字体、动画、视频和音频分别编号，各自按资产数组中的相对顺序分配 ID。外部文件不占对应的内嵌资源 ID。

页面、控件和资产引用都按当前工程解释。修改名称不会自动改写脚本；调整顺序后，脚本中的数字仍表示最终 ID。若该数字仍合法，检查器不会推测它原来指向哪个对象。

## 资源引用

```toml
[properties]
font = { ref = "ui_font" }
pic = { ref = "logo" }
```

| 表示方式 | 含义 |
|---|---|
| `{ ref = "logo" }` | 按当前资产符号解析，推荐用于源码工程 |
| `{ id = 0 }` | 使用对应类别最终列表中的数字 ID |
| `{ none = true }` | 明确为空；仅限允许空值的属性 |

三种表示方式互斥，不能使用裸整数。字体引用不接受空值，合法 ID 为 0～254。图片和媒体引用的空值条件由控件模式决定。外部路径的符号引用只接受 `ref`，不接受 `id` 或 `none`。

同一资产可以供多个控件共享。控件的坐标和宽高不会隐式修改共享图片；需要改变图片尺寸时，在资产描述中设置 `resize`。

## 全局代码和事件

`program.s` 保存全局代码，事件由页面文件引用独立的 `.s` 文件。脚本使用不带 BOM 的 UTF-8，不接受 NUL 字符；LF 和 CRLF 均可读取，中间的空行会保留。

工具保留脚本原文，不自动重构名称或数字引用。当前提供名称、ID、作用域及部分属性访问检查，完整范围见[页面编译器](../../hmi-compiler/README.md#检查范围)。

## 导入工程的原始附件

```toml
[original]
bundle = ".preserve/origin.hpo"
```

`.hpo` 保存重建所需的原始记录和未开放数据。导入的页面或控件可能带有 `original_record` 定位，用来读取附件中的原记录；它不是当前 ID，也不是用于跟踪重命名的对象标识。

导入的媒体通常以 `input = "native"` 指向 `.preserve/assets/` 中的 `.hma`。这些文件是明确声明的输入，应与工程清单、页面和源码一并保留。`.preserve` 不属于可删除的构建缓存。

`assets/view/` 是便于查看的副本，修改这些文件不会自动改变原生包输入。修改媒体前应调整资产描述，见[导入与编辑](../../hmi-parser/README.md#编辑导入的工程)。

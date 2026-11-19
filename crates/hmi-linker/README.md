# 链接器

[项目首页](../../README.md) · [页面编译器](../hmi-compiler/README.md) · [媒体编码器](../hmi-media/README.md) · [通用输出选项](../hmi-cli/reference/automation.md)

`hmi-link` 根据目标配置、有序页面目标、资产目标及必要原始附件分配最终 ID、检查工程引用、组织资源并重建 HMI。它不读取页面源码来补全目标，也不调用媒体转码后端。

## 命令

```text
hmi-link SOURCE -o OUTPUT [--map FILE] [-Werror]
hmi-link SOURCE --check [-Werror]
```

| 参数 | 含义 |
|---|---|
| `SOURCE` | `hmi.link/1` 链接清单 |
| `-o/--output` | HMI 输出路径；除 `--check` 外必填 |
| `--map` | 可选的链接映射输出路径 |
| `--check` | 在内存中完成链接和校验，不写文件；不能与 `-o/--map` 同用 |
| `-Werror` | 将警告作为错误；也接受 `--deny-warnings` |

```powershell
hmi-link build/link.toml --check
hmi-link build/link.toml -o output/Panel.HMI --map output/link-map.json
```

与统一入口不同，独立链接器只在明确指定 `--map` 时写映射；`--json` 的结果中也包含映射信息。

## 链接清单

下面的 `build/link.toml` 可配合[页面编译器示例](../hmi-compiler/README.md#命令)产生的文件使用：

```toml
schema = "hmi.link/1"
name = "Panel"
target = "target.json"
program = "program.hmo"

[pages]
objects = ["page0.hmo"]

[assets]
items = []
```

所有路径相对于链接清单。媒体目标可按当前符号列入：

```toml
[assets]
items = [
    { key = "ui_font", object = "assets/ui_font.hma" },
    { key = "logo", object = "assets/logo.hma" },
]
```

| 字段 | 含义 |
|---|---|
| `schema` | 填写 `hmi.link/1` |
| `name` | 工程显示名称，不覆盖目标内页面或控件名称 |
| `target` | 展开的有效目标配置 |
| `program` | 全局代码目标 `.hmo` |
| `pages.objects` | 非空页面目标数组，顺序决定最终页面 ID |
| `assets.items` | 资产符号及自包含 `.hma`；各类别分别编号 |
| `original.bundle` | 有原始记录依赖时必需的 `.hpo` 路径 |

`[assets]` 两个示例是替代配置，不应在同一文件中重复声明表。页面名称从 `.hmo` 读取，不能在清单中通过别名覆盖。

## 导入工程与原始附件

```toml
[original]
bundle = "original.hpo"
```

页面的原始附件摘要必须匹配。已有 `.hmo/.hma/.hpo` 和 `target.json` 后，可以移到其他目录独立链接；不需要原 HMI、PNG、ZI、GIF、AVI 或 WAV。

`hmi plan` 可生成相同格式的清单和独立工具参数，见[构建计划](../hmi-cli/reference/build.md#独立工具计划)。

## 输出与检查

链接按当前名称、数字 ID、资源类别和作用域判断引用，不推断重命名或重排之前的目标。当前静态脚本检查的边界见[页面编译器](../hmi-compiler/README.md#检查范围)。

HMI 之外的生成媒体按资产交付路径写到输出目录下，见[设备路径与部署](../hmi-media/reference/inputs.md#外部文件与设备路径)。运行时 `.data` 等需求只记录在映射中，不一定由工具生成。

路径冲突会在发布前检查；配套文件先写，HMI 最后原子替换。多文件发布失败可能已有部分配套文件写入，具体状态见诊断和 JSON 的 `outputs`。

## Rust 库

Cargo 包和库分别为 `hmi-linker`、`hmi_linker`。依赖公共模型、格式、分析及产物库，不依赖媒体转换后端。

```powershell
cargo doc -p hmi-linker --no-deps --open
```

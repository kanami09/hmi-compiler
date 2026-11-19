# 页面编译器

[项目首页](../../README.md) · [页面与事件](reference/pages.md) · [控件总览](reference/controls.md) · [通用输出选项](../hmi-cli/reference/automation.md)

`hmi-compile` 将页面或全局代码编译为 `.hmo`，供链接器使用。它检查页面结构和控件配置，保存脚本分析信息及尚待工程级解析的引用。

## 命令

```text
hmi-compile program SOURCE --target TARGET -o OUTPUT
hmi-compile page SOURCE --target TARGET -o OUTPUT [--original BUNDLE]
hmi-compile inspect FILE
```

| 参数 | 含义 |
|---|---|
| `SOURCE` | `program` 的全局 `.s` 文件，或 `page` 的页面 TOML |
| `--target` | 当前工具生成的 `target.json`，必填 |
| `-o/--output` | `.hmo` 输出路径，必填 |
| `--original` | 页面声明原始记录时所需的 `.hpo` 附件 |
| `inspect FILE` | 校验并查看目标描述、控件属性、引用及隐式资产需求 |

```powershell
hmi target export TJC4827X543_011 -o build/target.json
hmi-compile program panel/program.s --target build/target.json -o build/program.hmo
hmi-compile page panel/pages/page0/page.toml --target build/target.json -o build/page0.hmo
hmi-compile inspect build/page0.hmo --json
```

输出不会覆盖本次输入。更新目标配置后，应使用新目标重新编译相关文件。

## 页面与控件配置

- [页面、事件、脚本与原始记录](reference/pages.md)
- [控件总览与公共属性](reference/controls.md)
- [文本与按钮](reference/controls-text.md)
- [数值、选择与进度](reference/controls-numeric.md)
- [指针、波形与二维码](reference/controls-graphics.md)
- [状态开关与文本选择](reference/controls-selectors.md)
- [文件浏览器、文件流与数据记录](reference/controls-files.md)
- [图片、背景与游标](reference/controls-images.md)
- [外部图片与媒体播放](reference/controls-playback.md)

资源通过当前资产符号或最终数字引用。页面、控件名称和脚本按当前工程解释，工具不会跟踪历史对象身份或自动改写引用。

## 检查范围

页面编译检查名称、重复声明、允许的属性、类型、范围、文本字节容量、几何、事件槽和目标型号限制。

工程级脚本检查主要在 `hmi check/build` 或独立链接时完成，包括受支持形式的名称、数字 ID、跨页作用域、变量模式和部分只读／编译配置属性访问。动态表达式可能只能给出警告。

目前没有完整的脚本语法、表达式类型、指令与控件方法参数检查。方法调用识别也有未覆盖的形式；独立编译成功不能视为完整工程脚本编译通过。最终仍须由 USART HMI 完成脚本和固件编译。

## 目标文件与独立链接

`.hmo` 不要求保留原页面和脚本文件才能链接，但页面声明的 `.hpo` 附件仍必须提供。字体、图片和媒体的最终 ID 由链接器按当前清单分配。

文件浏览器省略图标时，目标可能声明内置图片需求。统一入口和 `plan` 会自动处理；自行组合时可通过 `inspect --json` 的 `implicit_assets` 查看，并使用 `hmi-media builtin` 生成对应目标，见[媒体工具](../hmi-media/README.md#内置图标)。

## Rust 库

此目录的 Cargo 包名为 `hmi-page-compiler`，库名为 `hmi_page_compiler`。可通过共享库编译程序和页面，而无需启动子进程：

```powershell
cargo doc -p hmi-page-compiler --no-deps --open
```

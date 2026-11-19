# 工程管理工具

[项目首页](../../README.md) · [工程文件](reference/project.md) · [构建与缓存](reference/build.md) · [日志与自动化](reference/automation.md)

`hmi` 是统一的工程入口，负责新建、导入、检查和构建工程，并串联页面编译、媒体处理与链接。

## 快速使用

以下示例假定工具所在目录已加入 PATH；源码构建方法见[项目首页](../../README.md#从源代码构建)。

```powershell
hmi init panel --target TJC4827X543_011 --orientation 0
hmi check panel
hmi build panel
```

默认输出 `panel/dist/Panel/Panel.HMI` 和同目录的 `link-map.json`。更改页面、控件、脚本或资产后再次 `build` 即可。

已有 HMI 可先导入：

```powershell
hmi inspect source.HMI
hmi import source.HMI -o panel
hmi build panel -o output/Panel.HMI
```

导入目录必须不存在或为空。编辑方式与附件用途见[解析器](../hmi-parser/README.md)。

## 命令与参数

| 命令 | 用途 | 主要参数 |
|---|---|---|
| `init DIR` | 新建空白工程 | `--target PROFILE` 必填；`--orientation` 默认 0 |
| `inspect FILE` | 查看 HMI 的目标、页面、控件和活动资源 | 输入现有 HMI |
| `import FILE -o DIR` | 从 HMI 创建源码工程 | `-o/--output` 必填 |
| `check [DIR]` | 检查源码、媒体输入与工程引用，不写构建产物 | 支持 `-Werror` |
| `build [DIR]` | 增量构建并重建 HMI | `-o/--output`、`-j/--jobs`、`-Werror` |
| `rebuild [DIR]` | 跳过派生缓存，执行全量重建 | 与 `build` 相同 |
| `clean [DIR]` | 清理工具登记且未被修改的构建产物 | 保留源码、原始附件和最终输出 |
| `plan [DIR] -o DIR` | 导出独立工具执行计划 | `-o/--output` 必填，不生成 HMO/HMA/HMI |
| `target list` | 列出内置目标型号 | — |
| `target show PROFILE` | 查看展开后的目标配置 | `--orientation` 默认 0 |
| `target export PROFILE -o FILE` | 写出 `target.json` | `--orientation` 默认 0，`-o/--output` 必填 |

带 `[DIR]` 的命令省略目录时，从当前目录向上查找 `hmi.toml`。显式目录直接作为工程目录。

方向支持 0、90、180、270。`-j` 范围为 1～256，默认按 CPU 数量、最多 8 个任务，媒体最多 2 个并发。

```powershell
hmi build panel -j 4 -o output/Panel.HMI
hmi rebuild panel -j 1
hmi check panel -Werror
hmi target show TJC4827X543_011 --orientation 90
```

`check` 的媒体探测可能完整解码，检查成功不表示所有编码步骤或上位机脚本编译都已验证。当前静态检查范围见[页面编译器](../hmi-compiler/README.md#检查范围)。

## 工程编辑

| 需要完成的工作 | 参考 |
|---|---|
| 配置型号、页面顺序、资产与输出路径 | [工程文件](reference/project.md) |
| 编辑页面、控件、事件与全局代码 | [页面与事件](../hmi-compiler/reference/pages.md)、[控件总览](../hmi-compiler/reference/controls.md) |
| 添加或修改媒体 | [媒体编码器](../hmi-media/README.md) |
| 理解导入的原始包和查看副本 | [解析器](../hmi-parser/README.md) |
| 并行、增量缓存、全量重建、清理 | [构建与缓存](reference/build.md) |
| 解析 JSON、实时事件、诊断和退出码 | [日志与自动化](reference/automation.md) |

## 单独组合工具

```powershell
hmi plan panel -o plan
```

按 `jobs.json` 中的命令名和参数调用 `hmi-compile`、`hmi-media`，再使用 `hmi-link`。计划中的 `.hmo/.hma` 在任务执行前可以尚不存在。

统一构建入口直接调用相同的共享库。独立工具适合脚本组合、定位单个输入问题或分阶段保存产物，见[链接器](../hmi-linker/README.md)。

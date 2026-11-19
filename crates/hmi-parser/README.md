# HMI 解析器

[项目首页](../../README.md) · [工程文件](../hmi-cli/reference/project.md) · [媒体输入方式](../hmi-media/reference/inputs.md) · [通用输出选项](../hmi-cli/reference/automation.md)

`hmi-parse` 读取已有 HMI，查看工程信息，或导出为本项目的源码工程。统一入口的 `hmi inspect/import` 使用相同实现。

## 命令

```text
hmi-parse inspect FILE
hmi-parse export FILE -o DIR
```

```powershell
hmi-parse inspect source.HMI
hmi-parse inspect source.HMI --json
hmi-parse export source.HMI -o panel
hmi build panel -o output/Panel.HMI
```

`-o/--output` 为必填导出目录，必须不存在或为空；不会覆盖已有源码工程。此命令导出工程文件，输出 HMI 使用 `hmi build` 或 `hmi-link`。

## 支持范围

支持 `ver21234`、UTF-8、已内置型号配置及与目标方向画布相匹配的页面几何，并识别受支持的图片、ZI、动画、视频和音频变体。

未知编码或尚未支持的几何差异会明确报错。保留未知字段不等于能够编辑任意布局；导入也不会恢复删除历史或空闲区分配。

## 导出内容

| 文件 | 用途 |
|---|---|
| `hmi.toml` | 目标配置、程序、页面和资产清单 |
| `program.s` | 全局代码 |
| `pages/` | 页面、控件和事件脚本 |
| `assets/*.asset.toml` | 每项媒体的显式描述，通常使用 `input="native"` |
| `.preserve/origin.hpo` | 原始页面／控件记录和需要保留的未开放数据 |
| `.preserve/assets/*.hma` | 保留运行媒体和已有编辑源的原生资产包 |
| `assets/view/` | 可以提取的查看副本，不是默认构建输入 |

资源 ID 按当前引用关系转为资产符号，不依赖物理资源文件名中的数字。各媒体类别分别保持逻辑顺序。

外部控件的路径作为设备路径导出；解析器不会访问 SD 卡、`ram/` 或本机虚拟 SD，也不会替用户补齐外部文件。

## 编辑导入的工程

修改 `hmi.toml` 所列页面文件、事件和全局代码，再重建即可。名称、已公开属性、资源引用和脚本以当前源码为准；原始记录定位只负责保留数据，不跟踪重命名或恢复旧 ID。

不要删除工程声明的 `.preserve` 附件。`.hpo` 和原生 `.hma` 是输入，清理构建缓存不会删除它们。

修改媒体时，不能只改 `assets/view/`。应调整资产描述，使用 `source` 指向可编辑的原图、ZI、GIF、视频或 WAV，并明确处理参数；需要复用已有编码时，再按[original 条件](../hmi-media/reference/inputs.md#条件复用原始编码)声明附件。

重建会按当前工程组织资源和校验，保留需要的原始记录及活动资源，不承诺整文件逐字节保持。删除控件和资产后，原附件不会自动恢复已交由资产管理的旧资源。

## Rust 库

Cargo 包和库分别为 `hmi-parser`、`hmi_parser`，负责读取、检查和导出，不负责生成 TFT。

```powershell
cargo doc -p hmi-parser --no-deps --open
```

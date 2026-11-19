# HMI Compiler

用于陶晶驰 USART HMI 的工程工具链：将已有 `.HMI` 导入为便于人类和 AI 阅读、编辑的工程文件，也可以从零创建工程，再编译页面、处理媒体并重建 `.HMI`。

页面和控件使用 TOML 描述，事件与全局代码保存为独立脚本，媒体通过资产描述管理。生成的 HMI 可继续交由 USART HMI 上位机打开、编辑和编译为 `.TFT`。

[快速开始](#快速开始) · [文档导航](#文档导航) · [支持范围](#支持范围) · [从源代码构建](#从源代码构建) · [许可证](#许可证)

## 主要功能

- **新建与导入**：创建空白工程，或从受支持的已有 HMI 导出页面、控件、脚本和媒体。
- **页面编辑**：通过名称和属性描述 30 类控件，支持增加、删除、重命名与重排。
- **媒体处理**：处理图片、ZI 字库、GIF 动画、视频和 WAV 音频，支持内嵌及外部文件。
- **工程构建**：页面编译、媒体处理和链接可独立调用，也可由统一入口串联。
- **增量与并行**：按内容复用编译、媒体和链接结果，提供并发控制、全量重建与清理。
- **自动化接口**：提供结构化诊断、JSON 结果和实时 JSONL，便于脚本及 Agent 调用。

## 快速开始

当前工具链可从源码构建，需要 Rust 1.89 或更新版本。在仓库根目录执行：

```powershell
cargo build --workspace --bins --release --locked
```

五个程序位于 `target/release/`。下面先使用完整路径；若将工具目录加入 PATH，后续可直接调用 `hmi` 等命令。

### 从零创建

```powershell
.\target\release\hmi.exe init panel --target TJC4827X543_011 --orientation 0
.\target\release\hmi.exe check panel
.\target\release\hmi.exe build panel
```

生成的空白工程包含 `hmi.toml`、`program.s` 和 `pages/page0/page.toml`。默认输出为 `panel/dist/Panel/Panel.HMI`，同目录的 `link-map.json` 记录最终页面、控件和资源信息。

接下来可按[工程文件](crates/hmi-cli/reference/project.md)、[页面与事件](crates/hmi-compiler/reference/pages.md)和[控件参考](crates/hmi-compiler/reference/controls.md)添加界面内容。

### 导入已有 HMI

```powershell
.\target\release\hmi.exe inspect source.HMI
.\target\release\hmi.exe import source.HMI -o panel
.\target\release\hmi.exe build panel -o output/Panel.HMI
```

导入目录必须不存在或为空。根据 `hmi.toml` 找到页面文件，修改属性、事件或全局代码后重建即可。原始附件应随工程保留，媒体查看副本的修改方式见[解析器说明](crates/hmi-parser/README.md)。

输入 HMI 和 `.TFT` 是不同类型的文件。本工具重建 HMI 工程，不生成可下载固件；完整脚本和固件编译由上位机完成。

## 文档导航

### 工具使用

| 程序 | 用途 | 文档 |
|---|---|---|
| `hmi` | 工程新建、导入、检查、构建、清理和计划 | [工程管理工具](crates/hmi-cli/README.md) |
| `hmi-parse` | 读取 HMI，导出源码工程 | [解析器](crates/hmi-parser/README.md) |
| `hmi-compile` | 编译页面和全局代码为目标文件 | [页面编译器](crates/hmi-compiler/README.md) |
| `hmi-media` | 探测、编码、封装和查看媒体 | [媒体编码器](crates/hmi-media/README.md) |
| `hmi-link` | 检查目标并链接重建 HMI | [链接器](crates/hmi-linker/README.md) |

### 按任务查阅

| 任务 | 入口 |
|---|---|
| 配置工程、页面顺序与资产 | [工程文件参考](crates/hmi-cli/reference/project.md) |
| 编辑控件、事件和脚本 | [页面与事件](crates/hmi-compiler/reference/pages.md)、[控件总览](crates/hmi-compiler/reference/controls.md) |
| 修改媒体与交付方式 | [媒体输入与部署](crates/hmi-media/reference/inputs.md) |
| 配置视频、音频转换依赖 | [FFmpeg 后端](crates/hmi-media/reference/ffmpeg.md) |
| 使用并行、缓存和全量重建 | [构建与清理](crates/hmi-cli/reference/build.md) |
| 通过 Agent 或脚本使用 | [日志、JSON 和 JSONL](crates/hmi-cli/reference/automation.md) |
| 手动组合各阶段程序 | [链接清单与独立工具](crates/hmi-linker/README.md) |
| 查看型号和方向支持 | [目标型号配置](data/profiles/README.md) |

### 公共库

| Cargo 包 | 职责 | 文档 |
|---|---|---|
| `hmi-core` | 工程模型、目标能力、诊断、执行报告和通用规则 | [公共模型](crates/hmi-core/README.md) |
| `hmi-format` | HMI 容器及资源格式的读取、写入与校验 | [文件格式](crates/hmi-format/README.md) |
| `hmi-analysis` | 脚本分析及工程级引用检查 | [脚本分析](crates/hmi-analysis/README.md) |
| `hmi-artifact` | 自包含的页面、媒体及原始附件产物 | [产物模型](crates/hmi-artifact/README.md) |

工具包的共享库入口在各工具 README 中说明。页面编译器的 Cargo 包名为 `hmi-page-compiler`，目录为 `crates/hmi-compiler`。

## 工程文件简览

```text
panel/
├── hmi.toml             # 型号、页面、程序和资产清单
├── program.s            # 全局代码
├── pages/               # 页面TOML与事件脚本
├── assets/              # 媒体描述和源文件
├── .preserve/           # 导入工程需要的原始附件
├── .build/              # 工具生成的产物和缓存
└── dist/                # 最终HMI及配套文件
```

页面和控件使用当前名称，数字 ID 按最终顺序解释，不自动改写脚本。资产使用明确的符号引用；需要保留的原始数据由附件提供，不能用缓存替代缺失输入。

## 支持范围

支持 `ver21234`、UTF-8 工程和四个内置目标型号的 0°、90°、180°、270° 配置。具体型号、控件和媒体限制见[型号配置](data/profiles/README.md)及对应参考页；该列表表示本工具已接入的范围，不是 HMI 格式或硬件能力的上限。

当前脚本辅助检查主要覆盖名称、ID、作用域和部分属性访问规则，尚未覆盖完整语法、指令及控件方法参数。构建通过后仍需使用上位机完成完整编译。

图片、GIF 和字库处理不需要原厂媒体转换工具；视频及需要转换的音频使用兼容的 Windows x64 FFmpeg 后端。已有原生资产及独立链接不依赖该后端。

导入会明确报告不支持的编码、型号或几何差异。重建保留需要的活动资源和原始数据，不恢复删除历史，也不要求与输入整文件逐字节一致。

## 日志与自动化

以下示例假定工具在 PATH 中：

```powershell
hmi build panel -j 4
hmi -v build panel
hmi --json check panel
hmi --message-format jsonl build panel
```

人类进度与诊断写到 stderr，查询内容写到 stdout。`--json` 输出单个最终对象，JSONL 按行输出事件，以最终结果结束。协议版本独立于软件版本；字段、状态、退出码和 Schema 见[自动化接口](crates/hmi-cli/reference/automation.md)。

## 从源代码构建

安装 Rust 后，在仓库根目录构建全部工具：

```powershell
cargo build --workspace --bins --release --locked
```

也可以直接运行统一入口：

```powershell
cargo run --release --locked -- inspect source.HMI
```

开发检查命令：

```powershell
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
```

常规测试使用公开的合成素材，无需提供已有 HMI。部分 FFmpeg 用例需要配置兼容后端后另行运行。

## 许可证

项目代码与文档采用 [MIT 许可证](LICENSE)。第三方组件遵循各自许可。

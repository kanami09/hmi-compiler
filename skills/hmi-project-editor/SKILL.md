---
name: hmi-project-editor
description: 使用 hmi-compiler 检查、导出、编辑和重构建陶晶驰 USART HMI 的已有 .HMI 工程或已导出的 workspace，修改控件属性、事件和全局脚本，并核对工程差异。适用于 ver21234、UTF-8 工程；不用于从零创建工程、结构编辑或生成 .TFT 固件。
---

# HMI 工程编辑

将用户对已有 HMI 工程的修改落实到可编辑文件中，生成新的 `.HMI` 并报告检查结果。用户明确指定的路径和工作范围优先。需要文件读写和命令执行能力，不依赖特定 agent、MCP 或 IDE。

## 准备工具和输入

- 先定位用户指定的 `.HMI` 或包含 `project.toml`、`.hmi/` 的 workspace。输入缺失或页面、控件目标有歧义时，先检查现有文件；仍不能确定再询问。
- 读取 [工具准备](references/tooling.md)，优先使用用户指定或本地已有的工具。配套脚本需要 Python 3.9+，仅在指定 `--download` 时联网下载固定版本，不修改系统 PATH。已有工具时可直接调用 CLI，无需 Python。
- 本文中的 `hmi-compiler` 代表已经确认的可执行文件，实际调用使用它的完整路径并正确引用含空格的路径；PowerShell 调用路径变量时使用 `&`。

## 编辑流程

按任务选择所需操作；只查看、检查或对比时，执行对应命令即可。

1. 对 `.HMI` 执行 `inspect`，确认版本、页面和诊断。需要编辑时导出到新目录；已有 workspace 则直接使用，保留用户现有编辑。

   ```text
   hmi-compiler inspect source.HMI --pages --json
   hmi-compiler export source.HMI -o workspace --json
   ```

   不默认使用 `export --force`，它会覆盖或清理已有导出文件。保留原始 `.HMI`，默认将输出写到不同文件。若只有 workspace，`.hmi/container.bin` 是初始导出时的原始工程，可作差异基准；不得修改它。

2. 编辑前读取 [workspace 编辑规则](references/workspace.md)。按 `project.toml` 的页面列表定位 `page.toml`，再按文件内的页面名、控件名和事件引用定位目标。只做用户要求的内容变更。

3. 修改后运行检查，按 `diagnostics` 的文件、行号、页面、控件、属性或事件定位问题。修复与本次变更有关的错误并重新检查；保留、说明无法静态确认的警告，不通过关闭警告掩盖问题。

   ```text
   hmi-compiler check workspace --json
   ```

4. 检查通过后构建，并比较原始工程和结果。`build` 成功会覆盖指定的目标文件，应选择新的输出路径；用户已指定覆盖时遵从其要求。

   ```text
   hmi-compiler build workspace -o edited.HMI --json
   hmi-compiler diff source.HMI edited.HMI --json
   ```

   `diff` 出现差异仍返回成功；读取 `result.changes` 判断是否符合需求。容器字节变化和语义变化分别判断，不要求编辑后的输出逐字节等同原工程。发现未预期变化时检查原因，必要时修正后重建。

## 结果与限制

- 使用 `--json` 读取结果，检查进程退出码和 `ok`。当前 `schema_version` 为 `1`；遇到其他版本先核实接口。退出码 `0` 为成功，`1` 为文件、检查或构建失败，`2` 为参数错误。字段可能缺失，不能只依赖诊断文本匹配。
- `rebuild` 用于需要全量重建或排查缓存时；`clean` 仅清理 workspace 的 `.build` 缓存。普通修改默认用 `build`。
- 当前仅支持已有 `ver21234`、UTF-8 工程的内容编辑。结构变更、素材替换、字体生成和从零创建超出范围；遇到这些需求说明限制，不通过修改原始二进制绕过。
- 工具执行、下载或格式不支持时，报告具体原因；没有新证据时不重复尝试同一失败操作，不静默换版本。
- 交付时给出 workspace 和输出文件路径、变更摘要、检查及差异结论、剩余警告。区分工具检查通过与 USART HMI 官方软件验收通过：本工具输出 `.HMI`，`.TFT` 编译、模拟器和实机验证仍需官方软件。只有实际执行过的验证才能宣称通过。

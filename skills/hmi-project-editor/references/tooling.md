# 工具准备

本 skill 可独立复制安装，所有路径相对于 skill 所在目录，不要求旁边存在源码仓库。不依赖特定 agent 的安装器。固定工具版本为 `0.1.0`，下载源为 `kanami09/hmi-compiler` 的 GitHub Release `v0.1.0`。

## 已有工具或按需下载

有 Python 3.9+ 时，在任意工作目录调用脚本的完整路径：

```text
python <skill目录>/scripts/ensure_tool.py
```

脚本依次检查 `--executable` 或 `HMI_COMPILER_BIN`、PATH、缓存；检查版本后向 stdout 输出一个 JSON 对象。成功时 `ok` 为 `true`，`path` 为可执行文件绝对路径。失败时 `ok` 为 `false`，`error` 为原因，退出码为 `1`。参数错误退出码为 `2`。

```text
python <skill目录>/scripts/ensure_tool.py --executable <工具完整路径>
python <skill目录>/scripts/ensure_tool.py --cache-dir <可写缓存目录>
```

缺少工具且运行环境允许联网和写入缓存时，执行：

```text
python <skill目录>/scripts/ensure_tool.py --download --cache-dir <可写缓存目录>
```

自动下载当前只支持 Windows x64/x86，使用现有发布流程的 ZIP 和 `.zip.sha256` 文件，校验后仅安装可执行文件，验证版本并缓存；后续复用缓存。默认缓存位于用户缓存目录的 `hmi-compiler` 下，受限环境用 `--cache-dir` 指向任务允许写入的目录。不会修改 PATH、注册系统服务或自动升级。

固定版本下载失败时保留错误，不猜测替代 URL 或切换 latest。若对应 Release 尚未发布，需要先由维护者发布，或按下文从源码构建。校验文件与 ZIP 来自同一 GitHub Release，用于内容完整性校验，不是独立签名认证。

PowerShell 调用示例（把路径换成真实路径）：

```powershell
$tool_info = python 'C:/skills/hmi-project-editor/scripts/ensure_tool.py' --executable 'C:/tools/hmi-compiler.exe'
if ($LASTEXITCODE -ne 0) { throw $tool_info }
$tool = ($tool_info | ConvertFrom-Json).path
& $tool inspect 'C:/projects/panel.HMI' --pages --json
```

没有 Python 时直接执行已有工具的 `--version`，确认是 `hmi-compiler 0.1.0` 后调用；也可从下面的 Release 页面手动下载和校验。Python 是辅助脚本的依赖，不是 CLI 的依赖。版本不符时说明情况，核对对应版本能力后再操作，不静默替换用户指定的工具。

## 手动下载与源码构建

- Release：<https://github.com/kanami09/hmi-compiler/releases/tag/v0.1.0>
- Windows 包名：`hmi-compiler-v0.1.0-windows-x64.zip` / `hmi-compiler-v0.1.0-windows-x86.zip`。
- 完整性文件：同名包附加 `.sha256`；计算 ZIP 的 SHA-256，与文件内对应包的摘要核对后解压。

Linux、macOS、Windows ARM64 或缺少预编译包时，使用已有匹配工具或 Rust 1.89+ 从源码构建：

```text
git clone --branch v0.1.0 --depth 1 https://github.com/kanami09/hmi-compiler.git <源码目录>
```

进入源码目录后执行：

```text
cargo build --release --locked --bin hmi-compiler
```

使用 `target/release/hmi-compiler`（Windows 为 `.exe`）。已有源码检出时先核实版本，保留本地改动；不用另行克隆或切换分支覆盖正在进行的开发。源码构建不是本 skill 的预编译平台兼容性保证，实际构建失败时报告诊断。

维护者更新工具版本时，应同步修改 `scripts/ensure_tool.py` 的 `TOOL_VERSION`、本页的版本及包名，并重新核对 CLI、workspace 格式和编辑边界。

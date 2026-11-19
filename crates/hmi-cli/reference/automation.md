# 日志与自动化接口

[项目首页](../../../README.md) · [工程管理工具](../README.md) · [JSON 结果 Schema](../../hmi-core/schemas/cli-result-v2.schema.json) · [JSONL 事件 Schema](../../hmi-core/schemas/cli-events-v2.schema.json)

本页适用于 `hmi`、`hmi-parse`、`hmi-compile`、`hmi-media` 和 `hmi-link`。

## 通用选项

| 选项 | 行为 |
|---|---|
| `-h`、`--help` | 查看命令帮助 |
| `-V`、`--version` | 查看程序版本 |
| `-v`、`--verbose` | 显示任务明细、缓存命中、已知处理参数和单项耗时 |
| `-q`、`--quiet` | 隐藏普通进度和成功提示，保留诊断与查询结果 |
| `--color auto/always/never` | 控制人类日志颜色，默认 `auto` |
| `--json` | 输出单个最终 JSON 对象 |
| `--message-format human/json/jsonl` | 选择人类文本、最终 JSON 或实时 JSONL |

`-v` 与 `-q` 互斥；`--json` 与 `--message-format` 互斥。机器模式不混入人类日志或 ANSI 颜色，`-v/-q` 不改变最终结果的含义。

`hmi check/build/rebuild` 和 `hmi-link` 支持 `-Werror`，将警告作为错误处理。独立链接器也接受同义选项 `--deny-warnings`。当前不提供按类别关闭警告的 `-Wno-...` 等选项。

## 人类日志

进度、警告和错误写到 stderr；`inspect`、目标查询等命令的查询内容写到 stdout。默认汇总缓存复用，详细模式逐项展开。

下面是格式示例，任务、大小和耗时以实际执行为准：

```text
    构建 Panel（增量构建）
    检查 工程配置与输入文件
    编译 全局代码 program.s
    编译 页面 home
    编码 图片 logo
    检查 页面、脚本及资源引用
    链接 工程资源
    校验 HMI 结构与资源
    写入 dist/Panel/Panel.HMI 及配套文件
    完成 dist/Panel/Panel.HMI（1.84 MiB，用时 2.40 秒）
         全局代码：编译 1 项
         页面：编译 1 页，复用 5 页
         媒体：处理 1 项，复用 10 项
         链接：生成 1 项
```

并行任务的日志可能交错，最终顺序不以日志出现的先后决定。长任务约每 5 秒显示任务和已用时间；动画、视频和音频转换在有实际计数时显示解码帧数、编码帧数或采样帧数。未知总量不显示估算百分比。

诊断包含稳定代码及可用的位置。只有行信息时不伪造列位置；独立链接缺少源码时，可能仅提供目标文件中保存的逻辑源文件名。

## 最终 JSON

```powershell
hmi --json build panel -j 4
hmi --json check panel -Werror
hmi --json --help
hmi --json --version
```

普通成功、执行失败、参数错误、帮助和版本都使用 JSON。下面只展示一个简单查询结果的基本结构：

```json
{
  "schema_version": 2,
  "tool": { "name": "hmi", "version": "0.1.0" },
  "command": "version",
  "ok": true,
  "project": null,
  "result": { "version": "0.1.0" },
  "diagnostics": []
}
```

| 字段 | 含义 |
|---|---|
| `schema_version` | 协议结构版本，独立于工具分发版本，目前为 2 |
| `tool` | 执行程序名称和版本 |
| `command` | 命令路径，例如 `build` 或 `target.show` |
| `ok` | 本次命令是否成功 |
| `project` | 已识别的工程名称和绝对根目录；未建立工程上下文时为 `null` |
| `result` | 命令结果及已获得的执行信息；没有可报告结果时为 `null` |
| `diagnostics` | 本次收集的错误和警告；无法继续分析时会提前终止 |

构建结果的 `summary` 按 `program`、`pages`、`media`、`link` 分组。每组满足：

```text
total = processed + reused + failed + not_run
```

`processed` 是成功实际处理的数量；`reused` 仅表示构建缓存命中。媒体封装、原生包保留、原始编码复用都属于实际处理，不与缓存复用混为一类。`parallelism` 表示并发上限，`elapsed_ms` 为整数毫秒。

`outputs` 列出最终产物及状态：

| 状态 | 含义 |
|---|---|
| `written` | 本次成功写入 |
| `not_written` | 本次未成功发布；路径上可能仍有旧文件 |
| `unchanged` | 协议为确认已有产物并保留预留的状态，当前流程未使用 |

大小使用整数字节，摘要使用 SHA-256。未发布产物不以旧文件摘要冒充本次结果。清理结果还可包含 `removed_files`、`retained_files`。失败时应结合 `ok`、诊断及逐项状态判断结果。

构建机文件系统路径使用绝对路径；Windows 上可能包含长路径前缀。HMI 的 `sd0/` 等设备路径、资源名称和逻辑源文件名不属于本机文件路径。转换相对路径时应使用报告中的 `project.root` 作为工程基准。

诊断包括 `severity`、`code`、`message`、`location`，以及可用的 `stage`、`category`、`details` 和 `original_severity`。Agent 可通过代码、位置和细节判断问题，中文说明用于解释；不要依赖 JSON 字段排列顺序。

## 实时 JSONL

```powershell
hmi --message-format jsonl build panel -j 4
```

stdout 每行一个事件对象，包含 `schema_version`、`event` 和 `data`。事件包括命令开始、阶段开始、任务开始／动作变化／结束、进度、明细、产物写入、诊断和最终 `result`。

最后一个 `result` 事件的 `data` 使用与最终 JSON 相同的结果结构。任务 ID 仅在本次执行中有效，不是页面或控件的持久标识。

计数中的 `phase` 区分 `decode`、`encode` 和 `resample`；`unit` 为 `frames` 或 `sample_frames`。不同阶段分别计数，不能把解码帧数当作整个编码任务的完成比例。`total = null` 表示总量未知。

## 退出码

| 退出码 | 含义 |
|---|---|
| 0 | 执行成功，允许包含未被升级的警告 |
| 1 | 输入、检查、编码、构建或发布失败 |
| 2 | 命令参数错误 |

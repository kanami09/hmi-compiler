# 外部图片与媒体播放

[项目首页](../../../README.md) · [页面编译器](../README.md) · [控件总览](controls.md)

本页四类控件在内置配置中仅对 X543 开放。声明运行资源与配置播放器是两步：资产负责媒体编码和交付方式，控件负责位置、播放状态、循环和速度。

## 外部图片

```toml
[[objects]]
name = "exp0"
type = "external_picture"
properties = { x = 20, y = 20, w = 138, h = 106, path = { ref = "logo_out" } }
```

需要 `x/y/w/h/path`，支持 `down/up`，`vscope` 默认 0。`path` 可写已有运行文件的设备路径，或引用 `delivery.mode="external"` 的[图片资产](../../hmi-media/reference/images.md)。外部图片不作为普通内嵌图片编号。

## 动画与视频

```toml
[[objects]]
name = "gm0"
type = "animation"
properties = { x = 20, y = 20, w = 160, h = 80, vid = { ref = "loading" }, en = 0, loop = 1 }
events = { playend = "events/gm0.playend.s" }

[[objects]]
name = "v0"
type = "video"
properties = { x = 20, y = 20, w = 160, h = 80, from = 1, path = { ref = "movie_out" }, en = 0 }
```

两类需要 `x/y/w/h`，支持 `down/up/playend`。动画 `vid` 只引用动画，视频 `vid` 只引用视频；它们分别编号，不共用图片 ID。

## 音频

```toml
[[objects]]
name = "sound0"
type = "audio"
properties = { vid = { ref = "startup" }, en = 0 }
events = { playend = "events/sound0.playend.s" }
```

音频为非可视控件，不接受几何、字体或图片，仅有 `playend` 事件。视频自带的音轨不占独立音频资产 ID。全局作用域不自动保证跨页连续播放，实际行为由运行环境决定。

## 播放属性

| 属性 | 默认值与含义 |
|---|---|
| `vscope` | 默认 0，允许 0／1 |
| `from` | 默认 0 内嵌，1 外部文件 |
| `vid` | 对应媒体类别的内嵌引用；`from=0` 必须有效 |
| `path` | 外部设备路径或外部资产 `ref`；`from=1` 必须有效 |
| `en` | 默认 0；0 停止、1 播放、2 暂停 |
| `loop` | 默认 0；0 不循环、1 循环；不改变 `en` |
| `dis` | 播放速度百分比，10～1000，默认 100 |
| `tim` | u32 初始播放时间，默认 0 |
| `stim/qty` | 运行时只读时长／数量，不接受初始输入 |

`from=1` 时可省略 `vid`、写 `{ none = true }` 或保留有效的内嵌引用；`from=0` 中的非活动路径同样可以保留。运行时只读结果不会从资产预先填入。

```toml
[[objects]]
name = "v1"
type = "video"
properties = { x = 20, y = 20, w = 160, h = 80, from = 1, path = "sd0/media/existing.video" }
```

直接路径只表示运行需求，不会读取构建机同名文件。`ref` 则必须指向工程内对应类别的外部资产，其配套文件随 HMI 输出。内嵌及外部配置见[媒体输入与部署](../../hmi-media/reference/inputs.md#外部文件与设备路径)。

编码参数分别见[动画](../../hmi-media/reference/animation.md)、[视频](../../hmi-media/reference/video.md)、[音频](../../hmi-media/reference/audio.md)。修改控件几何不会隐式缩放共享媒体。

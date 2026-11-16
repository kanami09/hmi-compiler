# FFmpeg DLL 接口

此后端当前用于 Windows x64 视频输入，按需动态加载。普通图片、GIF、字库和独立链接器不加载 FFmpeg。

库目录由 `HMI_FFMPEG_DIR` 指定；未设置时使用可执行文件旁的 `ffmpeg` 子目录。需要同一 shared 构建的五个库：`avutil-61.dll`、`swresample-7.dll`、`avcodec-63.dll`、`avformat-63.dll`、`swscale-10.dll`。无声解码也需要 swresample，因为该构建的 avcodec 依赖它。不会启动 FFmpeg EXE、修改 PATH 或写入 HMI 安装目录。

`hmi-media backend --json` 可查看库目录、运行时版本和指纹。库指纹参与媒体描述和构建输入变化检查；DLL 也是显式输入，输出流程禁止覆盖它们。库版本检查先于原生结构访问，当前接受相同 ABI 主版本中不早于以下基线的版本：avutil 61.1.102、avcodec/avformat 63.1.102、swscale 10.1.102、swresample 7.1.102。

每个进程只加载一组库并共享句柄，防止同名依赖被不同构建混用；切换 SDK 目录需要启动新进程。这只是后端生命周期管理，不是工程产物的增量缓存。

`bindings.rs` 使用 bindgen 0.72.1，从 FFmpeg `n9.0.2-49-gc179411bbb-20261111` shared 包头文件生成，包含完整的选定结构及字段偏移／大小断言；不会将实验用的手写结构前缀套用到任意 DLL。二进制提供方为 [FFmpeg-Builds](https://github.com/FFmpeg-Builds/FFmpeg-Builds)，对应固定包 `ffmpeg-n9.0-latest-win64-lgpl-shared-9.0.zip` 的 SHA-256 为 `17059ded875218b0251977abab6aea21adb6937bb4fd2130c81ada5c89f879d4`。latest 内容可能更新，不能仅根据同名包认定内容相同。普通构建使用已生成文件，无需 Clang 或 FFmpeg SDK；再生绑定时必须重新核对对应头文件、最低版本和 ABI 断言。

使用 `python tools/regenerate_ffmpeg_bindings.py --include path/to/sdk/include --offline` 可以再生绑定，需要 Clang 和缓存的 bindgen 依赖。脚本先核对 headers.json 中的 SDK 头文件摘要，拒绝不匹配的版本；普通用户不需要执行。

原生对象由 RAII 封装释放，数据包和帧及时 unref，处理 send/receive 的 EAGAIN 与 EOF 刷新。媒体输入只允许本地 file 协议。FFmpeg 原生日志静音，失败返回码转为工具链诊断；颜色转换使用源矩阵和范围，当前仅支持 SDR BT.601／BT.709、方形像素和稳定画面尺寸。源旋转矩阵、隔行标记、HDR、透明视频及变帧率当前明确拒绝，不静默转换。

视频调度首版支持精确等间隔时间戳、源帧率为目标帧率整倍数及整帧截取。当前保留全部解码画面进行验证，RGBA 总量限制为 128 MiB；后续可在保持时间调度规则的前提下改为流式取帧。

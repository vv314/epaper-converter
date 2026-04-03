# epaper-converter

面向树莓派墨水屏相框的高性能图片转换工具，使用 Rust 实现，目标面板为 `Waveshare 7.3inch e-Paper E`（ACeP 六色）。

它负责把常见图片快速转换成墨水屏可直接消费的目标格式：固定分辨率、固定六色调色板，以及适合驱动侧使用的原始缓冲区。

## 项目目标

- 将通用图片转换为墨水屏目标格式，降低驱动侧处理成本。
- 用 Rust 替代 Python 热路径，提升树莓派上的处理性能。
- 为相框场景提供可脚本化的 CLI，便于批量生成预览图或显示缓冲区。

## 目标硬件

### 适用墨水屏规格

当前实现针对以下面板约束编写：

- 面板：`Waveshare 7.3inch e-Paper E`
- 分辨率：`800x480`
- 调色板：`Black / White / Red / Yellow / Blue / Green`
- 输出语义：每像素一个调色板索引值，适合后续驱动层直接消费

调色板索引映射如下：

| 索引 | 颜色   | RGB           |
| ---- | ------ | ------------- |
| `0`  | Black  | `0,0,0`       |
| `1`  | White  | `255,255,255` |
| `2`  | Red    | `255,0,0`     |
| `3`  | Yellow | `255,255,0`   |
| `4`  | Blue   | `0,0,255`     |
| `5`  | Green  | `0,255,0`     |

## 功能概览

- 固定输出到目标分辨率 `800x480`
- 使用 `Lanczos3` 做缩放
- 支持三种量化模式：快速模式、Floyd-Steinberg 抖动模式、自适应模式
- 支持三种缩放策略：`stretch`、`contain`、`cover`
- 支持按 EXIF 自动旋转照片方向
- 支持输出 `BMP`、`PNG`、`BIN` 或同时输出 `BMP + BIN`
- 支持检测图片是否已经符合墨水屏格式
- 支持基础 benchmark，方便比较不同模式耗时

## 支持的输入格式

当前编译配置启用了以下输入/输出编解码能力：

- `PNG`
- `JPEG`
- `BMP`
- `TGA`

## 构建

## Quick Start

```bash
cargo build --release
./target/release/epaper_converter convert input.jpg output.bin -f bin -d auto --resize-mode contain
```

如果要部署到树莓派，推荐使用静态交叉编译并直接上传：

```bash
cargo build-linux-arm64-musl
scp target/aarch64-unknown-linux-musl/release/epaper_converter pi@<raspberry-pi-ip>:/tmp/epaper_converter
```

### 本机构建

```bash
cargo build --release
```

产物路径：

```bash
target/release/epaper_converter
```

### 交叉编译到树莓派 arm64

README 推荐统一使用静态发布方式，避免目标机 `glibc` 版本差异带来的运行时问题：

```bash
cargo build-linux-arm64-musl
```

该别名映射到：

```bash
cargo zigbuild --release --target aarch64-unknown-linux-musl
```

适用范围：

- 产物为静态链接二进制，不依赖目标机 `glibc`
- 更适合树莓派相框这类“拷贝即运行”的部署方式
- 仍然要求目标系统是 **64 位** `aarch64` Linux
- 不适用于 32 位树莓派系统，如 `armv7` / `armhf`

产物路径：

```bash
target/aarch64-unknown-linux-musl/release/epaper_converter
```

可以直接通过 `scp` 上传到树莓派，例如：

```bash
scp target/aarch64-unknown-linux-musl/release/epaper_converter pi@<raspberry-pi-ip>:/tmp/epaper_converter
```

已在 `Raspberry Pi Zero 2 W Rev 1.0` + `Debian 12 (bookworm) aarch64` 上验证可执行。

如果你明确希望使用依赖系统运行时的动态链接版本，也可以使用：

```bash
cargo build-linux-arm64
```

但对当前这个项目，通常没有必要把它作为 README 的主路径展开说明。

### 构建依赖

上述交叉编译方式都依赖以下工具：

- `cargo-zigbuild`
- `zig`

## 命令行用法

### 查看帮助

```bash
cargo run -- --help
```

### 1) 转换图片

```bash
cargo run --release -- convert input.jpg output.bin -f bin -d floyd
```

常用参数：

- `-w, --width`：目标宽度，默认 `800`
- `-H, --height`：目标高度，默认 `480`
- `-d, --dither`：抖动模式，`fast` / `floyd` / `auto`，默认 `floyd`
- `--resize-mode`：缩放策略，`contain` / `cover` / `stretch`，默认 `contain`
- `--auto-rotate`：是否按 EXIF 自动旋转，默认 `true`
- `-f, --format`：输出格式，`bmp` / `bin` / `png` / `both`
- `-b, --benchmark`：打印加载、转换、保存耗时

示例：

```bash
./target/release/epaper_converter convert photo.jpg frame.bin -f bin -d floyd -b
```

相册照片通常更适合使用自动策略，并保留原始宽高比：

```bash
./target/release/epaper_converter convert photo.jpg frame.bin -f bin -d auto --resize-mode contain
```

如果只想快速得到预览图：

```bash
./target/release/epaper_converter convert photo.jpg preview.bmp -f bmp -d fast
```

如果希望铺满屏幕并接受中心裁剪：

```bash
./target/release/epaper_converter convert photo.jpg cover.bmp -f bmp --resize-mode cover
```

### 2) 检查图片是否已经符合墨水屏格式

```bash
./target/release/epaper_converter check preview.bmp --verbose
```

检查逻辑包括：

- 分辨率是否为 `800x480`
- 像素是否全部落在六色调色板内

退出码语义：

- `0`：已符合墨水屏格式
- `1`：需要重新转换
- `2`：输入文件或读取过程出错

### 3) 跑基准测试

```bash
./target/release/epaper_converter benchmark photo.jpg
```

它会输出：

- `fast` 模式耗时
- `floyd` 模式耗时
- 索引图转 RGB 预览图的耗时

## 输出格式说明

### `bin`

- 每个像素占 `1` 字节
- 字节值范围为 `0..5`
- 在 `800x480` 下，总大小固定为 `384000` 字节
- 适合驱动层直接读取并映射到面板颜色

### `bmp` / `png`

- 适合预览与人工验收
- 图像已经被量化到六色调色板

### `both`

当使用：

```bash
./target/release/epaper_converter convert photo.jpg output.any -f both
```

程序会输出：

- `output.bmp`
- `output.bin`

## 当前边界

- 当前实现固定面向 `800x480` 六色面板，不是通用多型号框架
- 缩放使用 `resize_exact`，会强制拉伸到目标尺寸，不做裁剪或留白
- 工具只负责“图片转换”，不负责直接通过 SPI 刷屏
- 若后续接入其它面板，通常需要同时调整分辨率、调色板与驱动侧协议

## 适合相框项目的接入方式

推荐把这个工具放在“图片准备阶段”而不是“刷屏阶段”：

1. 上层逻辑选择待展示图片
2. 调用 `epaper_converter convert ... -f bin`
3. 驱动层读取 `.bin` 并发送到墨水屏
4. 可选保留 `bmp/png` 作为调试预览输出

这样可以把“图像处理成本”和“硬件通信成本”分层，便于分别优化。

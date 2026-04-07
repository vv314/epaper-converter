# epaper-converter

面向树莓派墨水屏的高性能图片转换工具，基于 Rust 实现。默认针对 `Waveshare 7.3inch e-Paper E`（ACeP 六色）面板设计。

核心功能是将通用图片预处理为墨水屏驱动可直接写入的 Raw Buffer（固定分辨率及六色调色板映射），从而将图像计算负载从驱动中剥离。

## 特性与目标

- **驱动解耦**：将缩放与量化前置，驱动脚本仅需处理单纯的硬件通信。
- **高性能**：基于 Rust 开发，在低功耗 ARM 设备上提供远超 Python 的处理速度。
- **CLI 友好**：提供标准命令行接口，便于相框等自动化项目以脚本集成。

## 面板约束

- **型号**：`Waveshare 7.3inch e-Paper E`
- **分辨率**：`800x480`
- **调色板**：Black, White, Red, Yellow, Blue, Green
- **输出编码**：每像素 1 字节（值范围 `0..5`），无缝对接驱动数组。

| 索引 | 颜色   | RGB           |
| ---- | ------ | ------------- |
| `0`  | Black  | `0,0,0`       |
| `1`  | White  | `255,255,255` |
| `2`  | Red    | `255,0,0`     |
| `3`  | Yellow | `255,255,0`   |
| `4`  | Blue   | `0,0,255`     |
| `5`  | Green  | `0,255,0`     |

## 功能概览

- 使用 `Lanczos3` 进行高质量缩放。
- 支持四种显式半色调模式：`bayer`、`blue-noise`、`yliluoma`、`atkinson`。
- 支持三种缩放策略：`contain`（等比留白）、`cover`（中心裁剪铺满）、`stretch`（强制拉伸）。
- 默认读取 EXIF 信息校正图像方向。
- **输入支持**：`JPEG`, `PNG`, `BMP`。
- **输出支持**：
  - `BIN`：每像素 1 字节 Raw Buffer
  - `PACKED`：针对微雪驱动优化的 4-bit 紧凑编码，每字节存两像素，直接入参 `epd.display()`
  - `BMP` / `PNG`：六色量化预览图
  - `BOTH`：组合输出（`BIN` + `BMP`）

## 快速开始

```bash
cargo build --release
epaper_converter convert input.jpg output.bin -f bin --halftone bayer --resize-mode contain
```

对于树莓派部署，推荐使用静态交叉编译：

```bash
cargo build-linux-arm64-musl
scp target/aarch64-unknown-linux-musl/release/epaper_converter pi@<ip>:/usr/local/bin/epaper_converter
```

## 命令行用法

### 图片转换 (`convert`)

```bash
epaper_converter convert input.jpg output.bin -f bin --halftone atkinson
```

**核心参数**：

- `-w, --width` / `-H, --height`：目标分辨率（默认 `800x480`）。
- `-m, --halftone`：半色调模式（默认 `bayer`）。
  - `bayer`：规则阈值矩阵抖动，画面更干净、速度更快。**适合大多数墨水屏预览与常规照片**。
  - `blue-noise`：蓝噪声阈值纹理，颗粒更细腻、规律感更弱。**适合渐变和大面积平滑过渡**。
  - `yliluoma`：调色板感知的有序抖动，颜色混合更柔和。**适合需要兼顾层次与配色过渡的图像**。
  - `atkinson`：更克制的误差扩散，层次更锐利。**适合细节复杂、局部反差高的图像**。
- `--resize-mode`：缩放策略（`contain`, `cover`, `stretch`，默认 `contain`）。
- `--gamma`：可选 Gamma 校正参数，默认 `1.0`；`< 1.0` 提亮中间调，`> 1.0` 压暗中间调。
  - 建议从 `1.0` 起步：夜景、深色背景可尝试 `1.10 ~ 1.20`；高亮、浅底、叶片占比高的图片通常保持 `1.0` 更稳。
  - 不建议大幅偏离 `1.0`；若超过 `0.85 ~ 1.20`，容易导致高光发白或暗部堵塞。
- `-f, --format`：输出格式（`bmp`, `bin`, `packed`, `png`, `both`）。
- `-b, --benchmark`：打印处理耗时。

**常见场景**：

- **照片转换**（等比留白 + 稳定预览）：
  ```bash
  epaper_converter convert photo.jpg frame.bin -f bin --halftone bayer --resize-mode contain
  ```
- **夜景压暗一点**（保留暗部氛围）：
  ```bash
  epaper_converter convert photo.jpg night.png -f png --halftone atkinson --resize-mode cover --gamma 1.15
  ```
- **壁纸转换**（中心裁剪铺满）：
  ```bash
  epaper_converter convert photo.jpg cover.bmp -f bmp --resize-mode cover
  ```
- **快速生成预览图**：
  ```bash
  epaper_converter convert photo.jpg preview.bmp -f bmp --halftone bayer
  ```

### 格式检查 (`check`)

验证图像分辨率是否为 `800x480` 且所有像素均符合六色调色板约束。

```bash
epaper_converter check preview.bmp --verbose
```

**退出码**：

- `0`: 验证通过。
- `1`: 不符合面板约束，需重新转换。
- `2`: 文件读取错误。

### 性能基准测试 (`benchmark`)

评估当前设备上的转换耗时（包括 `bayer` / `atkinson` 模式及反向 RGB 生成）。

```bash
epaper_converter benchmark photo.jpg
```

## 产物说明

- **`bin`**：1 Byte / Pixel。大小固定为 `384,000` 字节（针对 800x480），逐字节存放像素索引（`0..5`）。
- **`packed`**：4 Bits / Pixel（紧凑编码）。大小固定为 `192,000` 字节，每 2 个像素压入 1 字节中。**如果使用微雪官方 Python/C 驱动的 `epd.display(buf)`，请选用此格式以消除驱动内转换开销。**
- **`bmp` / `png`**：用于调试或 Web 端预览，图像已被强行量化为六色。
- **`both`**：同时生成 `.bin` 与 `.bmp`，兼顾实机刷屏与后台预览。

## 架构最佳实践

- **流水线解耦**：推荐作为预处理节点独立运行（如相框系统选图后立即异步转换），驱动侧仅读取生成好的 `.bin` 执行单向 SPI 通信，避免在单线程驱动中阻塞计算。
- **硬件扩展性**：当前调色板及相关约束硬编码面向 Waveshare 7.3inch ACeP。若需适配其余面板，需修改源码中的调色板映射表与默认分辨率。

## 交叉编译指南

交叉编译至 ARM64 依赖 `cargo-zigbuild` 与 `zig` 编译器。目标系统必须为 64 位 (`aarch64`)，不支持 `armv7` 等 32 位系统。

```bash
# 静态链接（推荐，无 glibc 依赖）
cargo build-linux-arm64-musl
# 等效于：cargo zigbuild --release --target aarch64-unknown-linux-musl

# 动态链接
cargo zigbuild --release --target aarch64-unknown-linux-gnu
```

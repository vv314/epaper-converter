<p align="center">
  <img src="./docs/assets/logo.svg" alt="ePaper Converter">
</p>

# ePaper Converter

面向树莓派墨水屏的高性能图片转换工具，基于 Rust 实现。默认针对 `Waveshare 7.3inch e-Paper E`（ACeP 六色）面板设计。

核心功能是将通用图片预处理为墨水屏驱动可直接写入的 Raw Buffer（固定分辨率及六色调色板映射），从而将图像计算负载从驱动中剥离。

## 核心特性

- **极致性能**：基于 Rust 开发，在低功耗 ARM 设备上提供远超 Python 的处理速度
- **驱动解耦**：将繁重的图片预处理前置，驱动脚本仅需纯粹的 SPI 硬件通信
- **高质量图像算法**：
  - **自动校正**：基于 EXIF 的智能方向校正
  - **清晰缩放**：采用 `Lanczos3` 高质量重采样，支持 `contain`（留白）、`cover`（裁切铺满）、`stretch`（拉伸）
  - **丰富抖动模式**：内置 `bayer`、`blue-noise`、`yliluoma`、`atkinson`、`floyd-steinberg`、`clustered-dot` 六种半色调算法，适应不同场景
- **全能格式输入输出**：
  - **输入**：原生支持 `JPEG`、`PNG`、`BMP`
  - **输出**：支持生成直接刷屏的 `BIN`（1 Byte/Pixel）、专为微雪优化的 `PACKED` 紧凑内存（2 Pixels/Byte），以及用于 Web/调试预览的量化 `BMP`/`PNG`
- **CLI 友好**：提供标准且强大的命令行接口，极易集成于树莓派电子相框等自动化工作流

## 适用硬件

### 墨水屏

| 项目     | 规格                                                   |
| -------- | ------------------------------------------------------ |
| 面板型号 | `Waveshare 7.3inch e-Paper E`                          |
| 分辨率   | `800x480`                                              |
| 调色板   | `Black`、`White`、`Red`、`Yellow`、`Blue`、`Green`     |
| 输出编码 | 每像素 `1` 字节，像素值范围 `0..5`，可直接对接驱动数组 |

**调色板映射**：

| 索引 | 颜色   | RGB           |
| ---- | ------ | ------------- |
| `0`  | Black  | `0,0,0`       |
| `1`  | White  | `255,255,255` |
| `2`  | Red    | `255,0,0`     |
| `3`  | Yellow | `255,255,0`   |
| `4`  | Blue   | `0,0,255`     |
| `5`  | Green  | `0,255,0`     |

## 快速开始

```bash
cargo build --release
epaper_converter convert input.jpg output.bin -f bin --dither bayer --resize-mode contain
```

对于树莓派部署，推荐使用静态交叉编译：

```bash
cargo build-linux-arm64-musl
scp target/aarch64-unknown-linux-musl/release/epaper_converter pi@<ip>:/usr/local/bin/epaper_converter
```

## 命令行用法

### 图片转换 (`convert`)

```bash
epaper_converter convert input.jpg output.bin -f bin --dither atkinson
```

**参数表**：

| 参数              | 说明                                                                     | 默认值    |
| ----------------- | ------------------------------------------------------------------------ | --------- |
| `input`           | 输入图片路径。                                                           | -         |
| `output`          | 输出文件路径。                                                           | -         |
| `-w, --width`     | 目标宽度。                                                               | `800`     |
| `-H, --height`    | 目标高度。                                                               | `480`     |
| `-d, --dither`    | 抖动模式，可选 `bayer`、`blue-noise`、`yliluoma`、`atkinson`、`floyd-steinberg`、`clustered-dot`。 | `bayer`   |
| `--resize-mode`   | 缩放策略，可选 `contain`、`cover`、`stretch`。                           | `contain` |
| `--auto-rotate`   | 是否在缩放前应用 EXIF 自动旋转。                                         | `true`    |
| `--gamma`         | Gamma 校正参数；`< 1.0` 提亮中间调，`> 1.0` 压暗中间调。                 | `1.0`     |
| `-f, --format`    | 输出格式，可选 `bmp`、`bin`、`packed`、`png`、`both`。                   | `bmp`     |
| `-b, --benchmark` | 打印处理耗时。                                                           | `false`   |

**抖动模式说明**：

| 模式         | 特点                                     | 适用场景                               |
| ------------ | ---------------------------------------- | -------------------------------------- |
| `bayer`      | 规则阈值矩阵抖动，画面更干净、速度更快。 | 大多数墨水屏预览与常规照片             |
| `blue-noise` | 预计算的 V&C 蓝噪声阈值纹理，颗粒更细腻、规律感更弱。 | 渐变和大面积平滑过渡                   |
| `yliluoma`   | 调色板感知的有序抖动，颜色混合更柔和。   | 需要兼顾层次与配色过渡的图像           |
| `atkinson`   | 更克制的误差扩散，层次更锐利。           | 细节复杂、局部反差高的图像             |
| `floyd-steinberg` | 经典误差扩散工程对照模式，主要用于横向比较、调参与回归验收。 | 需要标准对照而非主推观感的场景 |
| `clustered-dot` | 成团网点式有序抖动，纹理更像印刷半色调。 | 海报、插画、大色块和希望纹理更稳定的图像 |

- `atkinson` 更偏最终观感输出；`floyd-steinberg` 更偏工程对照与算法验收，不作为主推视觉模式。

**Gamma 使用建议**：

- 建议从 `1.0` 起步：夜景、深色背景可尝试 `1.10 ~ 1.20`；高亮、浅底、叶片占比高的图片通常保持 `1.0` 更稳。
- 不建议大幅偏离 `1.0`；若超过 `0.85 ~ 1.20`，容易导致高光发白或暗部堵塞。

**常见场景**：

- **照片转换**（等比留白 + 稳定预览）：
  ```bash
  epaper_converter convert photo.jpg frame.bin -f bin --dither bayer --resize-mode contain
  ```
- **夜景压暗一点**（保留暗部氛围）：
  ```bash
  epaper_converter convert photo.jpg night.png -f png --dither atkinson --resize-mode cover --gamma 1.15
  ```
- **壁纸转换**（中心裁剪铺满）：
  ```bash
  epaper_converter convert photo.jpg cover.bmp -f bmp --resize-mode cover
  ```
- **快速生成预览图**：
  ```bash
  epaper_converter convert photo.jpg preview.bmp -f bmp --dither bayer
  ```

### 格式检查 (`check`)

验证图像分辨率是否为 `800x480` 且所有像素均符合六色调色板约束。

```bash
epaper_converter check preview.bmp --verbose
```

**参数表**：

| 参数            | 说明                             | 默认值  |
| --------------- | -------------------------------- | ------- |
| `input`         | 待检查的图片路径。               | -       |
| `-v, --verbose` | 输出详细信息。                   | `false` |
| `-q, --quiet`   | 静默模式，仅通过退出码表示结果。 | `false` |

**退出码**：

- `0`: 验证通过。
- `1`: 不符合面板约束，需重新转换。
- `2`: 文件读取错误。

### 性能基准测试 (`benchmark`)

评估当前设备上的转换耗时（包括 `bayer` / `blue-noise` / `yliluoma` / `atkinson` / `floyd-steinberg` / `clustered-dot` 模式及反向 RGB 生成）。

```bash
epaper_converter benchmark photo.jpg
```

**参数表**：

| 参数           | 说明                         | 默认值 |
| -------------- | ---------------------------- | ------ |
| `input`        | 用于基准测试的输入图片路径。 | -      |
| `-w, --width`  | 基准测试时使用的目标宽度。   | `800`  |
| `-H, --height` | 基准测试时使用的目标高度。   | `480`  |

## 产物说明

| 格式          | 每像素占用 | 输出大小 (800x480) | 说明                                                                                                                                |
| ------------- | ---------- | ------------------ | ----------------------------------------------------------------------------------------------------------------------------------- |
| `bin`         | `1 Byte`   | `384,000 字节`     | 逐字节存放像素索引（`0..5`）。                                                                                                      |
| `packed`      | `4 Bits`   | `192,000 字节`     | 紧凑编码，每 2 个像素压入 1 字节中。<br>**如果使用微雪官方 Python/C 驱动的 `epd.display(buf)`，请选用此格式以消除驱动内转换开销。** |
| `bmp` / `png` | -          | -                  | 用于调试或 Web 端预览，图像已被强行量化为六色。                                                                                     |
| `both`        | -          | -                  | 同时生成 `.bin` 与 `.bmp`，兼顾实机刷屏与后台预览。                                                                                 |

## 交叉编译指南

交叉编译至 ARM64 依赖 `cargo-zigbuild` 与 `zig` 编译器。目标系统必须为 64 位 (`aarch64`)，不支持 `armv7` 等 32 位系统。

```bash
# 静态链接（推荐，无 glibc 依赖）
cargo build-linux-arm64-musl
# 等效于：cargo zigbuild --release --target aarch64-unknown-linux-musl

# 动态链接
cargo zigbuild --release --target aarch64-unknown-linux-gnu
```

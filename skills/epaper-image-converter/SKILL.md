---
name: epaper-image-converter
description: Convert and display images for Waveshare 7.3inch e-Paper E by calling the bundled Rust binary epaper_converter.
---

# E-Paper Image Converter

这是一个面向 `Waveshare 7.3inch e-Paper E`（ACeP 六色）面板的树莓派 skill，提供图像处理、格式检查与刷屏显示能力。

## 当前组成

- `scripts/epaper_converter`：核心图像预处理 CLI 工具
- `scripts/show_on_screen.py`：负责将预处理后的数据推送到硬件的刷屏脚本
- `test_when_ready.sh`：基础自检脚本

## 目标硬件与数据格式

- **支持的输入图片格式**：`JPEG`, `PNG`, `BMP`
- **面板**：`Waveshare 7.3inch e-Paper E`
- **分辨率**：`800x480`
- **调色板**：`Black / White / Red / Yellow / Blue / Green`
- **`bin`**：1 字节/像素的 Raw Buffer，索引值 `0..5`。
- **`packed`**：4-bit/像素的紧凑缓冲编码（2 像素合成 1 字节），**可以直接喂给微雪驱动的 `epd.display()`**。

## 推荐工作流

在相框系统中，标准的使用链路分为“后台预转换”和“硬件级刷屏”两步。

### Step 1. 预处理图像 (转换并缓存)

通过 `epaper_converter` 将通用照片离线转换为墨水屏专用的驱动数据结构。

```bash
# 生成驱动可直接消费的 packed 紧凑编码文件，推荐使用 cover 策略铺满屏幕
./scripts/epaper_converter convert input.jpg output.packed -f packed --halftone atkinson --resize-mode cover

# 如果想要预览转换后的呈现效果，可以额外输出 bmp
./scripts/epaper_converter convert input.jpg preview.bmp -f bmp --halftone bayer --resize-mode contain
```

> **提示**：你可以通过检查命令验证某张图片是否已满足严格的墨水屏规格要求：
>
> ```bash
> ./scripts/epaper_converter check preview.bmp --verbose
> ```

### Step 2. 硬件刷屏 (推送到屏幕)

使用 `show_on_screen.py` 读取转换好的产物并点亮屏幕。

如果传入的是刚才生成的 `output.packed`，它将直接通过 SPI 透传，跳过所有重复解析耗时。
如果传入的是原始图片 `photo.jpg`，它也会在内部调用 `epaper_converter` 先转为 `packed` 再执行发送。

默认情况下，`show_on_screen.py` **不会先执行整屏 `Clear()`**，以避免多做一次完整刷新；如果你确实需要先清屏再显示，可以显式加上 `--clear`。

```bash
# 推荐：直接刷入上一步已准备好的 .packed 数据
./scripts/show_on_screen.py output.packed

# 快捷方式：由脚本代劳转换并铺满刷屏
./scripts/show_on_screen.py photo.jpg --halftone atkinson --resize-mode cover

# 如需恢复“先清屏再显示”的旧行为，可手动开启
./scripts/show_on_screen.py photo.jpg --halftone atkinson --resize-mode cover --clear
```

## `convert` 命令参数一览

执行 `./scripts/epaper_converter --help` 获取最新权威说明。

- `-w, --width`：目标宽度，默认 `800`
- `-H, --height`：目标高度，默认 `480`
- `-m, --halftone`：半色调算法。
  - `bayer`：规则阈值矩阵抖动，默认方案，干净稳定。
  - `blue-noise`：蓝噪声阈值纹理，更适合渐变和细腻哑光质感。
  - `atkinson`：更锐利的误差扩散，适合高复杂度图像。
  - `auto`：（默认选项）根据图像复杂度在 `bayer`、`blue-noise` 与 `atkinson` 间自动选择。
- `--resize-mode`：排版策略。`contain`（等比留白）, `cover`（裁剪填满）, `stretch`（无视比例拉伸）
- `--auto-rotate true|false`：是否按 EXIF 自动旋转
- `-f, --format`：输出文件格式。可选 `packed`, `bin`, `bmp`, `png`, `both`
- `-b, --benchmark`：开启耗时基准统计（加载/缩放/量化/输出），方便对比选型

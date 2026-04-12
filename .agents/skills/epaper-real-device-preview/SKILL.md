---
name: epaper-real-device-preview
description: 人工查看真实屏幕效果时触发，把本地生成的墨水屏图片或缓冲区发送到树莓派，并在 Waveshare 7.3inch e-Paper E 真机上显示。
---

# E-Paper Real Device Preview

这个 skill 用于把本地已经生成好的预览图、PNG 或 `.packed` 缓冲区，发送到树莓派 `zero2w`，并刷新到 `Waveshare 7.3inch e-Paper E` 真机上，方便做肉眼验收。

## Skill 文件清单

```text
.
├── SKILL.md
└── scripts/show_on_device.py
```

- `SKILL.md`：skill 说明文件，约定适用场景、输入方式和输出要求
- `scripts/show_on_device.py`：真机预览执行脚本，支持单图显示与 A/B 对比

## 适用场景

- 你已经在本地生成了某个算法产物，例如 `output/lam.cover.rotated-clustered-dot_initial.png`
- 你想确认某个 dithering / halftone 模式在真机上的实际纹理，而不是只看电脑预览图
- 你需要把同一张图依次刷到屏幕上，做人眼 A/B 对比

## 当前环境约定

- SSH 主机别名：`zero2w`
- 远程驱动路径：`/home/pi/RPi_Zero_PhotoPainter/7in3_e-Paper_E/python`
- 远程 Python：`python3`
- 远程已安装 `Pillow`
- 屏幕分辨率：`800x480`
- 若输入是 `PNG/BMP` 预览图，要求其像素已经严格落在六色调色板内，否则远程打包时会失败

## 内置脚本

- `scripts/show_on_device.py`：本地执行脚本，负责 `scp` 上传到 `zero2w`，并通过 `ssh` 在树莓派上调用 Waveshare 驱动刷屏
- 该脚本同时支持单图模式和 A/B 模式
- 单图模式传 `1` 个文件；A/B 模式传 `2` 个文件

## 推荐工作流

### 1. 先确认本地文件存在

优先检查目标文件是否已经生成，例如：

```bash
ls output/lam.cover.rotated-clustered-dot_initial.png
```

### 2. 优先直接使用内置脚本

单图显示：

```bash
python3 scripts/show_on_device.py output/lam.cover.rotated-clustered-dot_initial.png
```

A/B 对比：

```bash
python3 scripts/show_on_device.py \
  output/lam.cover.clustered-dot_dot38.png \
  output/lam.cover.rotated-clustered-dot_initial.png
```

如果想让 A/B 每张图停留更久：

```bash
python3 scripts/show_on_device.py \
  output/lam.cover.clustered-dot_dot38.png \
  output/lam.cover.rotated-clustered-dot_initial.png \
  --hold-seconds 15
```

### 3. 如需手动执行，仅保留简短兜底说明

- 上传可直接用 `scp <local-file> zero2w:/tmp/<remote-file>`
- 刷屏可通过 `ssh zero2w` 登录后，调用 `Waveshare 7.3inch e-Paper E` 对应 Python 驱动完成显示
- 除非脚本不可用，否则不要优先走手动流程

## AB 模式

当目标是做真机 A/B 对比时，这个 skill 应把“连续刷两张图并提醒用户观察差异”视为一等场景，而不是两次互不相关的单图显示。

### 适用方式

- A、B 必须来自同一原图、同一分辨率、同一 resize mode、同一 gamma
- 只允许一个关键变量变化，例如 `dither` 模式、`clustered-dot` 与 `rotated-clustered-dot`、或同一算法的不同参数版本
- 两张图都优先使用已经生成好的 `PNG` 或 `.packed` 产物，避免远程再次跑转换引入额外变量

### 推荐命名

- A 图：`output/lam.cover.clustered-dot_dot38.png`
- B 图：`output/lam.cover.rotated-clustered-dot_initial.png`

### 操作顺序

1. 先显示 A 图
2. 提醒用户观察 5~15 秒，关注颗粒感、横竖纹、暗部脏感、彩边、色块稳定性
3. 再显示 B 图
4. 回报两次刷屏是否都成功，并明确 A/B 对应的文件

### 推荐命令范式

优先用脚本：

```bash
python3 scripts/show_on_device.py \
  output/lam.cover.clustered-dot_dot38.png \
  output/lam.cover.rotated-clustered-dot_initial.png
```

如果脚本不可用，再手动上传 `A/B` 两张图到树莓派临时目录，并在 `zero2w` 上依次调用驱动显示；这里不再内联维护手动脚本细节。

### 回报要求

执行 AB 模式时，输出中应明确：

- A 图路径
- B 图路径
- 显示顺序
- 每张图是否成功显示
- 建议用户重点观察的差异点

## 建议的操作顺序

- 优先刷已经量化好的 `PNG` 或 `.packed`，避免在树莓派上再次运行转换算法，减少变量干扰
- 做 A/B 对比时，保持同一张原图、同一尺寸、同一 gamma，仅切换 dithering 模式
- 每次刷屏后记录主观感受，例如：颗粒感、条纹感、色块稳定性、暗部脏感、彩色边缘是否更明显

## 常见失败原因

- `ssh zero2w` 失败：本机 SSH 别名或网络不可达
- `FileNotFoundError`：本地输出文件还没生成，或传输路径写错
- `non-palette pixel found`：传过去的 `PNG` 不是严格六色图，不能直接按调色板映射成 packed
- `invalid packed size`：`.packed` 文件尺寸不是 `192000` 字节
- 驱动导入失败：远程 Waveshare 驱动路径不在 `/home/pi/RPi_Zero_PhotoPainter/7in3_e-Paper_E/python`

## 输出要求

执行这个 skill 时，应至少回报：

- 使用的本地输入文件路径
- 远程落盘路径
- 刷屏命令是否执行成功
- 如可获取，补充远程文件大小、图像尺寸与驱动返回信息

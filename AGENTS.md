# 项目协作约定

## 开发约定

- 项目结构应尽量符合 Rust 社区最佳实践，优先采用社区常见的目录组织、命名方式和测试布局。

## 测试约定

### 生成规则

- 开发测试场景下，测试图片统一从 `tests/fixtures/` 读取。
- 生成结果统一输出到项目根目录下的 `output/` 目录。
- 缩放/裁剪模式固定使用 `cover`。
- 为了能让 LLM 读取图片内容，输出格式固定为 `png`。
- 文件命名格式固定为 `{原名}.cover.{算法}.png`。
- 若通过测试 harness 批量生成产物，默认跑 `bayer`、`blue-noise`、`atkinson`、`burkes`、`yliluoma`，用于自动化批量对比。

### 命名示例

- `gradient.cover.fast.png`
- `gradient.cover.floyd.png`
- `gradient.cover.blue-noise.png`

### 产物管理

- `tests/fixtures/` 用于保存可复用的测试输入资源，可以稳定入库。
- `output/` 用于保存开发调试和算法对比产物，默认视为验证输出目录。
- 重新生成一批对比图前，可先清空 `output/`，避免历史结果干扰判断。
- 若需长期保留某轮算法对比结果，应通过明确的 `算法迭代标识` 区分，而不是覆盖旧文件。
- 测试 harness 已支持并行生成，适合批量扫描参数；若仅验证单个改动，优先缩小夹具和参数范围，避免无意义地跑满整套组合。

## 算法调优

- 详细规范参考 `docs/algorithm-tuning.md`。
- 当任务涉及算法、量化、抖动、颜色映射、缩放策略、gamma 或 harness 调优时，优先先读 `docs/algorithm-tuning.md` 再执行。
- 日常最小约定保持不变：输入夹具使用 `tests/fixtures/`，调优产物输出到 `output/`，缩放模式固定为 `cover`，优先配合 `palette-report` 做占比分析。

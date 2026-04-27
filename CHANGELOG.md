# 更新日志

**[English](./CHANGELOG_en.md) | 简体中文**

本文档记录本项目所有显著的变更。

格式遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
版本号遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

## [Unreleased]

## [0.3.1] - 2026-04-28

### 新增

- **子命令拼写纠错**：`ncm2mp3 inf song.ncm` 现在会自动推断为
  `info`，`comp bash` 推断为 `completion`，等等。`wath` / `covar`
  这类替换型错字会显式给出"你是不是想输入 `watch`？"提示。
  实现上结合了 clap 的 `infer_subcommands(true)`（前缀匹配）和基于
  Levenshtein 编辑距离的建议器（替换型错字）。中英文双语提示。
  依赖轻量的 `strsim` crate。
- **分发清单**位于 `dist/`：Scoop 清单（`dist/scoop/ncm2mp3.json`）
  和 Homebrew formula（`dist/homebrew/ncm2mp3.rb`），让 Windows /
  macOS / Linux 用户能一行命令安装。
- **自动同步工作流**（`.github/workflows/update-dist.yml`）：每次
  Release 发布后自动从 `.sha256` 兄弟文件抓取 6 份哈希，连同新版
  本号一起填入两份清单，并开 PR 等你 review。从此发版无需手动
  复制粘贴 hash。
- **Release 压缩包现在附带 `.sha256` 兄弟文件**（更新了
  `release.yml` 让它生成）。既给自动同步工作流用，也方便用户用
  `sha256sum -c` 验证手动下载的完整性。

### 变更

- **crates.io 上的 crate 名**：
  - `ncm-core` → `ncm2mp3-core`（`ncm-core` 名字被另一个无关项目
    占了）。源码不变 —— `ncm-cli` 通过 Cargo 的 `package = "..."`
    把依赖别名回 `ncm-core`。
  - `ncm-cli` → `ncm2mp3`（这样 `cargo install ncm2mp3` 就能直接
    跑，而且和用户实际敲的二进制名一致）。
- **README 徽章**：crates.io 版本号、docs.rs、下载量、许可证、CI
  状态——五个徽章已加在两份 README 顶部。

### 测试

- **`pipeline.rs` 新增 28 个单元测试**（之前 0 个）：glob 排除、
  格式过滤、`--from-file` 解析、冲突解决（skip/overwrite/rename
  + 索引探测）、输出路径在 模板 × 文件夹模式 × 扩展名替换 各种
  组合下的解析。总测试数从 51 提升到 79。

## [0.3.0] - 2026-04-23

### 新增

- `--exclude <GLOB>` —— 跳过匹配 glob 模式的文件（可重复指定）。
  在目录扫描和去重之后应用。底层用 `globset` crate；模式匹配整条
  路径字符串。
- `--limit N` —— 限制处理的文件数量。适合在大型音乐库上先小规模
  试跑（"先用 5 个文件验证模板对不对，再放开整库"）。
- 模板占位符扩充：
  - `{bitrate_k}` —— kbps 整数后带 "k" 后缀，例如 `320k`。
  - `{duration}` —— 总秒数（整数）。
  - `{duration_mmss}` —— `mm:ss` 格式。
- `--color=auto|always|never` 强制启用或禁用每个文件状态行的
  ANSI 颜色。识别 `NO_COLOR` 环境变量（设置后强制 never）。

### 分发

- `dist/` 下发布了 Scoop 清单和 Homebrew formula，让 Windows /
  macOS 用户能一条命令安装。

## [0.2.0] - 2026-04-23

### 新增

- `--from-file <list.txt>` 从文本文件读取额外的输入路径（一行
  一个，`#` 开头的行和空行被忽略）。
- `--on-conflict skip|overwrite|rename` 替代了原来的布尔开关
  `--overwrite`，给出三选一策略。`rename` 模式会在输出 stem 后
  追加 `-1`、`-2`…直到找到一个空闲路径。`--overwrite` 保留为
  别名以兼容旧用法。
- `cover` 子命令 —— 不解密音频，只把封面图导出为 `stem.jpg` /
  `stem.png`。支持目录输入 + `-r` 递归。
- `watch` 子命令 —— 监听目录，新出现或修改的 `.ncm` 文件自动
  解密。复用 decrypt 流水线的所有参数（template / output /
  format / folder / on-conflict / no-tag）。1 秒去抖，避免编辑器
  风格的中间临时写入触发多次解密。
- 运行统计加到 summary 行：累计耗时 + 按格式分布
  （`Done: 10 ok, 0 skipped, 0 failed (1m23s) — MP3:8 FLAC:2`）。
- 配置文件支持：TOML 格式，路径在 `~/.config/ncm2mp3/config.toml`
  （Windows 上是 `%APPDATA%\ncm2mp3\config.toml`）。优先级：
  **CLI 参数 > 配置文件 > 内置默认值**。可通过 `--config <path>`
  指定路径，或 `--no-config` 完全禁用加载。
- `docs/ARCHITECTURE.md` —— NCM 字节级格式参考、流密码数学
  推导、模块布局。
- `docs/demo.tape` —— 可复现的 vhs 脚本，用来重新生成 README 中
  的 demo GIF。
- 项目治理文件：`CHANGELOG.md`、`CONTRIBUTING.md`、`SECURITY.md`、
  `.github/ISSUE_TEMPLATE/*`、`.github/PULL_REQUEST_TEMPLATE.md`。
- `ncm-core` 所有公开 API 都加了完整的 rustdoc 注释，并启用
  `#![warn(missing_docs)]`，让 CI 拒绝未来未文档化的新增 API。

### 变更

- `Cli::input` 改为 `Option<PathBuf>`，以容纳 `--from-file` 作为
  唯一输入源的场景。
- `AudioFormat` 加上 `#[derive(Hash)]`，使其可以作为
  按格式统计的 `HashMap` 的 key。
- 一次性升级了 7 个跨大版本落后的依赖，分两轮：thiserror 1→2、
  console 0.15→0.16、aes 0.8→0.9、cipher 0.4→0.5、ecb 0.1→0.2、
  lofty 0.21→0.24、dirs 5→6、notify 6→8、notify-debouncer-mini
  0.4→0.7、toml 0.8→1。

## [0.1.0] - 2026-04-22

首个发布版本。

### 新增

- **`ncm-core` 库** —— 纯解密原语，可作为独立 crate 使用：
  - NCM 二进制格式解析器（Magic → RC4 密钥 → metadata → 封面 → 音频）
  - 自定义流密码（标准 RC4 KSA + NCM 特有的 PRGA，支持按偏移流式处理）
  - AES-128-ECB + PKCS7 包装器（用于密钥和 metadata 段）
  - 基于魔数的音频格式自动检测（MP3、FLAC、M4A、WAV、Ogg）
  - 封面 MIME 检测（JPEG、PNG）
  - `NcmDecoder` 流式编排器，开头嗅探用于格式识别
- **`ncm2mp3` CLI 二进制**：
  - 默认 `decrypt` 模式（不带子命令时隐式触发）
  - `info <INPUT>` —— 仅查看 metadata，不解密音频
  - `completion <SHELL>` —— 生成 bash/zsh/fish/powershell/elvish 补全脚本
  - `-t/--template` 文件名模板，占位符 `{artist} {album} {title} {format} {bitrate}`
  - `-F/--folder` 每首歌一个独立文件夹 + 单独的 `cover.jpg`/`cover.png`
  - `-r/--recursive` 目录递归遍历
  - `--format` 按内部音频格式过滤
  - `--no-tag` 跳过 ID3/Vorbis 标签写入
  - `--overwrite` / `--dry-run`
  - `-j/--jobs` 并行 worker 数（默认 CPU 核心数）
  - `-v`/`-vv` 日志详细度
  - `-L/--lang en|zh` 界面语言，也可通过 `NCM2MP3_LANG` 环境变量设置；
    自动从 `LANG`/`LC_ALL` 检测
- **国际化** —— help 文本、运行时消息、错误信息全部覆盖英文和简体中文
- **跨平台发布** —— GitHub Actions 在 tag push 时自动构建 6 个目标三元组：
  - `x86_64-unknown-linux-gnu` / `aarch64-unknown-linux-gnu`
  - `x86_64-apple-darwin` / `aarch64-apple-darwin`
  - `x86_64-pc-windows-msvc` / `aarch64-pc-windows-msvc`
- **CI** —— 每次 push 和 PR 在 Ubuntu/macOS/Windows 上跑 fmt + clippy + test
- **文档** —— 双语 `README.md`（中文）+ `README_en.md`（英文）
- **许可证** —— Apache-2.0

[Unreleased]: https://github.com/Johnserf-Seed/ncm2mp3/compare/v0.3.1...HEAD
[0.3.1]: https://github.com/Johnserf-Seed/ncm2mp3/releases/tag/v0.3.1
[0.3.0]: https://github.com/Johnserf-Seed/ncm2mp3/releases/tag/v0.3.0
[0.2.0]: https://github.com/Johnserf-Seed/ncm2mp3/releases/tag/v0.2.0
[0.1.0]: https://github.com/Johnserf-Seed/ncm2mp3/releases/tag/v0.1.0

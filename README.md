# ncm2mp3

**[English](./README_en.md) | 简体中文**

一个用 Rust 写的命令行工具，把网易云音乐的 `.ncm` 加密文件还原成原始音频格式（MP3 / FLAC / M4A …），带标签、带封面、并行批处理、中英文界面。

```console
$ ncm2mp3 info "范玮琪,张韶涵 - 如果的事.ncm"
文件: 范玮琪,张韶涵 - 如果的事.ncm
大小: 9.44 MB
格式: MP3
码率: 320 kbps
时长: 03:48
标题: 如果的事
艺术家: 范玮琪, 张韶涵
专辑: Faces Of FanFan
封面: PNG, 736.62 KB

$ ncm2mp3 "范玮琪,张韶涵 - 如果的事.ncm"
✓ 范玮琪,张韶涵 - 如果的事.ncm -> 范玮琪,张韶涵 - 如果的事.mp3
完成：1 成功，0 跳过，0 失败
```

## 功能特性

- **保留原始音频**：NCM 内部已是真实的 MP3/FLAC/M4A 等格式，本工具仅剥离加密层，**不涉及真正的音频转码**
- **自动检测格式**：通过音频前几字节的魔数识别真实格式，不信任可能过时的元数据声明
- **标签 & 封面**：自动写入 ID3v2（MP3）或 Vorbis Comment（FLAC）标签，内嵌封面图片
- **文件名模板**：`--template "{artist}/{album}/{title}"` 灵活命名，不指定时保留原始文件名
- **每首歌独立文件夹**：`--folder` 模式下，封面额外导出为 `cover.jpg` / `cover.png`
- **并行批处理**：`-j N` 控制 worker 数（默认 CPU 核心数），`-r` 递归目录
- **中英双语**：自动检测系统语言，或用 `-L zh` / `NCM2MP3_LANG=zh` 切换
- **干跑预览**：`--dry-run` 只预览将产生的输出，不写盘
- **纯 Rust + 零外部依赖**：一个 1.4 MB 的静态可执行文件，无需 ffmpeg 或其他工具

## 安装

### 从源码编译

需要 [Rust 工具链](https://rustup.rs/)（1.75+）。

```bash
git clone https://github.com/JohnserfSeed/ncm2mp3.git
cd ncm2mp3
cargo build --release
# 可执行文件在 target/release/ncm2mp3(.exe)
```

### 装到系统 PATH

```bash
cargo install --path crates/ncm-cli
# 之后可在任何目录直接运行 ncm2mp3
```

## 使用

### 基础用法

```bash
# 单文件，保留原始文件名
ncm2mp3 song.ncm

# 指定输出目录
ncm2mp3 song.ncm -o ./output

# 自定义文件名模板
ncm2mp3 song.ncm -t "{artist} - {title}"

# 按艺术家/专辑/标题 层级分目录
ncm2mp3 song.ncm -t "{artist}/{album}/{title}"

# 每首歌独立文件夹 + 单独封面文件
ncm2mp3 song.ncm -F
```

### 批量处理

```bash
# 处理整个目录
ncm2mp3 ./ncm_library

# 递归子目录
ncm2mp3 ./ncm_library -r

# 指定并行数（默认 CPU 核心数）
ncm2mp3 ./ncm_library -r -j 8

# 只处理 FLAC 格式的 NCM
ncm2mp3 ./ncm_library -r --format flac

# 覆盖已存在的输出文件
ncm2mp3 ./ncm_library -r --overwrite
```

### 查看元数据

```bash
# 显示单文件信息（不解密）
ncm2mp3 info song.ncm

# 显示目录下所有 NCM 的信息
ncm2mp3 info ./ncm_library -r
```

### 干跑预览

```bash
ncm2mp3 ./ncm_library -r -t "{artist}/{album}/{title}" --dry-run
```

只打印每个文件会生成的输出路径，不写盘。

### 语言设置

```bash
# 命令行参数
ncm2mp3 -L zh song.ncm
ncm2mp3 --lang en song.ncm

# 环境变量（持久化）
export NCM2MP3_LANG=zh    # bash / zsh
$env:NCM2MP3_LANG = "zh"  # PowerShell

# 自动从系统 LANG / LC_ALL 检测
# 中文 Windows/Linux 默认即为中文界面
```

## 文件名模板占位符

| 占位符 | 说明 | 示例 |
|---|---|---|
| `{title}` | 歌名 | `如果的事` |
| `{artist}` | 艺术家（多个用 `, ` 连接） | `范玮琪, 张韶涵` |
| `{album}` | 专辑名 | `Faces Of FanFan` |
| `{format}` | 音频格式扩展名 | `mp3` / `flac` |
| `{bitrate}` | 码率（kbps） | `320` |

模板里的 `/` 或 `\` 会被视为**目录分隔符**。非法字符（`< > : " / \ | ? *` 等）在每个占位符填充后会被替换为 `_`。

## 完整命令行参数

```
Usage: ncm2mp3 [OPTIONS] <INPUT>
       ncm2mp3 info <INPUT>

参数:
  <INPUT>                   .ncm 文件或目录

选项:
  -o, --output <目录>       输出目录（默认：输入文件所在目录）
  -t, --template <模板>     文件名模板（默认：保留原文件名）
  -r, --recursive           递归目录
      --format <格式,...>   只处理指定内部格式（mp3 / flac / m4a …）
      --no-tag              不写标签，也不嵌入封面
  -F, --folder              每首歌独立文件夹 + 外置 cover
  -j, --jobs <N>            并行 worker 数（默认 CPU 核心数）
      --overwrite           覆盖已有输出文件
      --dry-run             仅预览，不写盘
  -v, --verbose             -v 显示 info 日志，-vv 显示 debug
  -L, --lang <en|zh>        界面语言（默认自动检测）
  -h, --help                显示帮助
  -V, --version             显示版本
```

## 支持的格式

解密后保留 NCM 内部的真实格式，自动识别：

| 魔数 | 格式 | 扩展名 |
|---|---|---|
| `ID3` / `0xFFFB` / `0xFFFA` | MP3 | `.mp3` |
| `fLaC` | FLAC | `.flac` |
| `ftyp` (offset 4) | M4A / AAC | `.m4a` |
| `RIFF...WAVE` | WAV | `.wav` |
| `OggS` | Ogg Vorbis | `.ogg` |

## 项目结构

这是一个 Cargo workspace，由两个 crate 组成：

- **`ncm-core`** — 纯解密库。实现 NCM 二进制格式解析、NCM 自定义流加密、AES-128-ECB 元数据解密、音频格式嗅探。**无 CLI 依赖**，可独立作为库使用。
- **`ncm-cli`** — 命令行前端。基于 `clap` 的子命令派发、`lofty` 写标签、`rayon` 并行、i18n。

## 从源码构建

```bash
# 开发构建（快）
cargo build

# 发布构建（优化后 1.4 MB）
cargo build --release

# 跑完整测试（40+ 个测试）
cargo test --workspace
```

## 实现原理

NCM 文件的二进制布局（按顺序）：

```
Magic 'CTENFDAM' (8B) + Gap (2B)
→ RC4 密钥长度 (4B LE) + RC4 密钥数据  [XOR 0x64 → AES-128-ECB(CORE_KEY) → strip "neteasecloudmusic"]
→ Metadata 长度 (4B LE) + Metadata 数据 [XOR 0x63 → strip "163 key(Don't modify):" → Base64 → AES-128-ECB(META_KEY) → strip "music:" → JSON]
→ CRC32 (4B) + Gap (5B)
→ Cover 长度 (4B LE) + Cover 原始字节（JPEG 或 PNG）
→ 音频数据（到文件末尾）[RC4 流加密：KSA 标准 RC4，PRGA 为 NCM 自定义公式]
```

关键在于 NCM 的流加密不是标准 RC4：KSA（密钥排布）仍然标准，但 PRGA（伪随机流生成）用的是 `S[(S[i]+S[(S[i]+i) & 0xff]) & 0xff]` 的独特查表公式，允许直接按偏移计算而非维护状态。这使得流式分块处理（不用一次性读入整首歌到内存）变得简单。

## 致谢

NCM 格式的逆向工程来自多年积累，特别感谢以下先行实现者：

- [anonymous5l/ncmdump](https://github.com/anonymous5l/ncmdump)（C++）
- [taurusxin/ncmdump](https://github.com/taurusxin/ncmdump)（Go）
- [nondanee/ncmdump](https://github.com/nondanee/ncmdump)（C）
- [iqiziqi/ncmdump.rs](https://github.com/iqiziqi/ncmdump.rs)（Rust 参考实现）

## 许可

MIT OR Apache-2.0，与主流 Rust 生态一致。仅用于**个人合法购买的音乐**解密存档；请勿用于传播版权音乐。

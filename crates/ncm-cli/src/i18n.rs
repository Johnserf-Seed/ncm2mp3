//! Localization for the CLI. Holds a `Strings` table for each supported
//! language and exposes a process-wide accessor populated once at startup.

use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    En,
    Zh,
}

/// Every user-visible string goes through this table so translations stay in
/// one place and the type system flags any missing localization.
pub struct Strings {
    // --- CLI description + argument help ---
    pub cli_about: &'static str,

    pub arg_input: &'static str,
    pub arg_output: &'static str,
    pub arg_template: &'static str,
    pub arg_recursive: &'static str,
    pub arg_format: &'static str,
    pub arg_no_tag: &'static str,
    pub arg_folder: &'static str,
    pub arg_jobs: &'static str,
    pub arg_overwrite: &'static str,
    pub arg_on_conflict: &'static str,
    pub arg_from_file: &'static str,
    pub arg_dry_run: &'static str,
    pub arg_verbose: &'static str,
    pub arg_lang: &'static str,

    // --- Subcommand help ---
    pub cmd_info_about: &'static str,
    pub arg_info_input: &'static str,
    pub cmd_cover_about: &'static str,
    pub arg_cover_input: &'static str,
    pub cmd_completion_about: &'static str,
    pub arg_completion_shell: &'static str,
    pub val_shell: &'static str,
    pub val_path: &'static str,
    pub val_conflict: &'static str,

    // --- `info` mode labels ---
    pub info_file: &'static str,
    pub info_size: &'static str,
    pub info_format: &'static str,
    pub info_bitrate: &'static str,
    pub info_duration: &'static str,
    pub info_title: &'static str,
    pub info_artist: &'static str,
    pub info_album: &'static str,
    pub info_cover: &'static str,
    pub info_none: &'static str,

    // --- Value names (shown in usage, e.g. `--output <DIR>`) ---
    pub val_input: &'static str,
    pub val_dir: &'static str,
    pub val_template: &'static str,
    pub val_fmt: &'static str,
    pub val_n: &'static str,
    pub val_lang: &'static str,

    // --- Runtime messages. Placeholders are substituted via str::replace. ---
    /// `{path}`
    pub msg_no_files: &'static str,
    /// `{count}`
    pub msg_queued: &'static str,
    /// `{fmt}` = audio format name
    pub msg_skipped_fmt: &'static str,
    /// `{path}`
    pub msg_skipped_exists: &'static str,
    pub msg_dry_run_prefix: &'static str,
    /// `{path}`
    pub msg_tagging_failed: &'static str,
    /// `{path}`
    pub msg_input_missing: &'static str,
    /// `{ok}`, `{skipped}`, `{failed}`
    pub msg_summary: &'static str,
    /// `{elapsed}` formatted like `1m23s`.
    pub msg_summary_elapsed: &'static str,
    /// `{breakdown}` is a space-joined list like `MP3:10 FLAC:3 M4A:1`.
    pub msg_summary_breakdown: &'static str,
    /// `{path}`
    pub msg_cover_written: &'static str,
    /// `{path}`
    pub msg_cover_no_cover: &'static str,
    /// `{path}`
    pub msg_cover_unknown_mime: &'static str,

    // --- Errors ---
    pub err_jobs_zero: &'static str,
    pub err_no_parent: &'static str,
    pub err_rename_exhausted: &'static str,
}

pub const EN: Strings = Strings {
    cli_about: "Decrypt Netease Cloud Music NCM files to their original audio format",

    arg_input: "Input .ncm file or a directory of .ncm files",
    arg_output: "Output directory (defaults to the input's parent directory)",
    arg_template: "Filename template. When omitted, the original input filename is preserved (only the extension changes). Placeholders: {artist} {album} {title} {format} {bitrate}. Forward slashes create subdirectories; the extension is appended automatically based on the detected audio format.",
    arg_recursive: "Recurse into subdirectories when INPUT is a directory",
    arg_format: "Only process files whose internal format matches (e.g. mp3,flac). Can be repeated or comma-separated.",
    arg_no_tag: "Do not write ID3/Vorbis tags or embed cover art",
    arg_folder: "Wrap each decrypted song in its own folder, and also drop the cover art as a separate cover.jpg/cover.png file inside it",
    arg_jobs: "Number of parallel workers (default: number of CPU cores)",
    arg_overwrite: "Overwrite existing output files instead of skipping them (alias for --on-conflict overwrite)",
    arg_on_conflict: "What to do when an output file already exists: skip (default) / overwrite / rename (append -1, -2, ... until free)",
    arg_from_file: "Read additional input paths from a text file (one path per line; # comments and blank lines ignored)",
    arg_dry_run: "Print what would be done without writing any output files",
    arg_verbose: "Increase log verbosity (-v for info, -vv for debug)",
    arg_lang: "UI language: en or zh (default: auto-detect from LANG/LC_ALL)",

    cmd_info_about: "Inspect NCM file metadata without decrypting the audio",
    arg_info_input: "Input .ncm file or a directory of .ncm files to inspect",
    cmd_cover_about: "Extract the embedded cover image (cover.jpg/cover.png) without decrypting the audio",
    arg_cover_input: "Input .ncm file or a directory of .ncm files whose covers should be extracted",
    cmd_completion_about: "Print a shell completion script to stdout (bash / zsh / fish / powershell / elvish)",
    arg_completion_shell: "Target shell",
    val_shell: "SHELL",
    val_path: "PATH",
    val_conflict: "STRATEGY",

    info_file: "File",
    info_size: "Size",
    info_format: "Format",
    info_bitrate: "Bitrate",
    info_duration: "Duration",
    info_title: "Title",
    info_artist: "Artist",
    info_album: "Album",
    info_cover: "Cover",
    info_none: "(none)",

    val_input: "INPUT",
    val_dir: "DIR",
    val_template: "TEMPLATE",
    val_fmt: "FMT",
    val_n: "N",
    val_lang: "LANG",

    msg_no_files: "no .ncm files found under {path}",
    msg_queued: "queued {count} file(s) for processing",
    msg_skipped_fmt: "internal format {fmt} not in --format filter",
    msg_skipped_exists: "output exists: {path}",
    msg_dry_run_prefix: "[dry-run]",
    msg_tagging_failed: "tagging failed for {path} (file still decrypted)",
    msg_input_missing: "input path does not exist: {path}",
    msg_summary: "Done: {ok} ok, {skipped} skipped, {failed} failed",
    msg_summary_elapsed: " ({elapsed})",
    msg_summary_breakdown: " — {breakdown}",
    msg_cover_written: "cover extracted -> {path}",
    msg_cover_no_cover: "no embedded cover in {path}",
    msg_cover_unknown_mime: "cover in {path} has unknown image type; skipping",

    err_jobs_zero: "--jobs must be >= 1",
    err_no_parent: "input has no parent directory",
    err_rename_exhausted: "rename strategy ran out of suffixes (1..9999) for {path}",
};

pub const ZH: Strings = Strings {
    cli_about: "解密网易云音乐 NCM 文件为其原始音频格式",

    arg_input: "输入的 .ncm 文件或包含 .ncm 文件的目录",
    arg_output: "输出目录（默认：输入文件所在目录）",
    arg_template: "文件名模板。未指定时保留输入文件的原始文件名（仅更换扩展名）。占位符：{artist} {album} {title} {format} {bitrate}。正斜杠会创建子目录，扩展名根据检测到的音频格式自动附加。",
    arg_recursive: "当 INPUT 为目录时递归处理子目录",
    arg_format: "仅处理内部格式匹配的文件（例如 mp3,flac）。可多次指定或用逗号分隔。",
    arg_no_tag: "不写入 ID3/Vorbis 标签，也不嵌入封面",
    arg_folder: "为每首歌创建独立的文件夹，并在文件夹内额外写入独立的 cover.jpg / cover.png 封面文件",
    arg_jobs: "并行 worker 数（默认：CPU 核心数）",
    arg_overwrite: "覆盖已存在的输出文件（等价于 --on-conflict overwrite）",
    arg_on_conflict: "输出文件已存在时的策略：skip（默认）/ overwrite / rename（自动追加 -1、-2 …）",
    arg_from_file: "从文本文件读取额外的输入路径（一行一个；以 # 开头的行和空行被忽略）",
    arg_dry_run: "仅打印将要执行的操作，不写入任何文件",
    arg_verbose: "增加日志详细度（-v 显示 info，-vv 显示 debug）",
    arg_lang: "界面语言：en 或 zh（默认：从 LANG/LC_ALL 自动检测）",

    cmd_info_about: "查看 NCM 文件的元数据信息，不解密音频",
    arg_info_input: "要检查的 .ncm 文件或包含 .ncm 文件的目录",
    cmd_cover_about: "导出内嵌的封面图片（cover.jpg / cover.png），不解密音频",
    arg_cover_input: "要导出封面的 .ncm 文件或包含 .ncm 文件的目录",
    cmd_completion_about: "输出 shell 补全脚本到 stdout（bash / zsh / fish / powershell / elvish）",
    arg_completion_shell: "目标 shell",
    val_shell: "SHELL",
    val_path: "路径",
    val_conflict: "策略",

    info_file: "文件",
    info_size: "大小",
    info_format: "格式",
    info_bitrate: "码率",
    info_duration: "时长",
    info_title: "标题",
    info_artist: "艺术家",
    info_album: "专辑",
    info_cover: "封面",
    info_none: "(无)",

    val_input: "输入",
    val_dir: "目录",
    val_template: "模板",
    val_fmt: "格式",
    val_n: "数量",
    val_lang: "语言",

    msg_no_files: "在 {path} 下未找到 .ncm 文件",
    msg_queued: "已加入队列：{count} 个文件",
    msg_skipped_fmt: "内部格式 {fmt} 不匹配 --format 过滤条件",
    msg_skipped_exists: "输出文件已存在：{path}",
    msg_dry_run_prefix: "[试运行]",
    msg_tagging_failed: "{path} 写入标签失败（音频已解密完成）",
    msg_input_missing: "输入路径不存在：{path}",
    msg_summary: "完成：{ok} 成功，{skipped} 跳过，{failed} 失败",
    msg_summary_elapsed: "（用时 {elapsed}）",
    msg_summary_breakdown: " —— {breakdown}",
    msg_cover_written: "已导出封面 -> {path}",
    msg_cover_no_cover: "{path} 不含内嵌封面",
    msg_cover_unknown_mime: "{path} 的封面图片类型未知，已跳过",

    err_jobs_zero: "--jobs 必须 >= 1",
    err_no_parent: "输入路径没有父目录",
    err_rename_exhausted: "rename 策略的后缀已用尽（1..9999）：{path}",
};

impl Lang {
    pub fn strings(self) -> &'static Strings {
        match self {
            Lang::En => &EN,
            Lang::Zh => &ZH,
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        let lower = raw.trim().to_ascii_lowercase();
        // Accept common spellings: en / english / zh / zh-cn / zh_CN / cn / chinese
        if lower == "en"
            || lower.starts_with("en-")
            || lower.starts_with("en_")
            || lower == "english"
        {
            return Some(Lang::En);
        }
        if lower == "zh"
            || lower.starts_with("zh-")
            || lower.starts_with("zh_")
            || lower == "cn"
            || lower == "chinese"
        {
            return Some(Lang::Zh);
        }
        None
    }

    /// Detect from environment. Honors (in order):
    ///   NCM2MP3_LANG, LC_ALL, LC_MESSAGES, LANG.
    /// Returns `En` when no hint is found.
    pub fn detect_from_env() -> Self {
        for var in ["NCM2MP3_LANG", "LC_ALL", "LC_MESSAGES", "LANG"] {
            if let Ok(val) = std::env::var(var) {
                if let Some(lang) = Self::parse(&val) {
                    return lang;
                }
                // Handle LANG values like "zh_CN.UTF-8" or "en_US.UTF-8" that
                // start with a known prefix even if the full string isn't a
                // pure code.
                let head = val.split(['.', '@']).next().unwrap_or("");
                if let Some(lang) = Self::parse(head) {
                    return lang;
                }
            }
        }
        Lang::En
    }
}

static STRINGS: OnceLock<&'static Strings> = OnceLock::new();

/// Install the chosen translation table for the process. Subsequent calls are
/// silently ignored so tests can init multiple times without panicking.
pub fn init(strings: &'static Strings) {
    let _ = STRINGS.set(strings);
}

/// Access the active translation table. Falls back to English if `init` was
/// never called (e.g. in unit tests).
pub fn t() -> &'static Strings {
    STRINGS.get().copied().unwrap_or(&EN)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_english_variants() {
        assert_eq!(Lang::parse("en"), Some(Lang::En));
        assert_eq!(Lang::parse("EN"), Some(Lang::En));
        assert_eq!(Lang::parse("en-US"), Some(Lang::En));
        assert_eq!(Lang::parse("english"), Some(Lang::En));
    }

    #[test]
    fn parse_chinese_variants() {
        assert_eq!(Lang::parse("zh"), Some(Lang::Zh));
        assert_eq!(Lang::parse("zh-CN"), Some(Lang::Zh));
        assert_eq!(Lang::parse("zh_CN"), Some(Lang::Zh));
        assert_eq!(Lang::parse("cn"), Some(Lang::Zh));
        assert_eq!(Lang::parse("chinese"), Some(Lang::Zh));
    }

    #[test]
    fn parse_rejects_unknown() {
        assert_eq!(Lang::parse("fr"), None);
        assert_eq!(Lang::parse(""), None);
        assert_eq!(Lang::parse("xyz"), None);
    }

    #[test]
    fn strings_differ_between_languages() {
        assert_ne!(EN.cli_about, ZH.cli_about);
        assert_ne!(EN.msg_summary, ZH.msg_summary);
    }
}

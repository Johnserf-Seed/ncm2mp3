//! Arguments that aren't valid UTF-8, such as a GBK-named file on Linux,
//! must reach clap instead of panicking in the `--lang` scan that runs
//! before it. Unix only, since that's where an argument is raw bytes.

#![cfg(unix)]

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::process::{Command, Output};

/// Run the built binary in an empty directory, with English messages and
/// no config file.
fn ncm2mp3(args: &[&OsStr]) -> Output {
    let dir = tempfile::tempdir().unwrap();
    Command::new(env!("CARGO_BIN_EXE_ncm2mp3"))
        .arg("--no-config")
        .args(args)
        .current_dir(dir.path())
        .env("NCM2MP3_LANG", "en")
        .output()
        .expect("failed to run ncm2mp3")
}

fn assert_no_panic(args: &[&OsStr]) {
    let out = ncm2mp3(args);
    let stderr = String::from_utf8_lossy(&out.stderr);
    // 101 is a Rust panic's exit code. The stderr check also catches a
    // `panic = "abort"` build, where a panic ends in SIGABRT instead.
    assert_ne!(out.status.code(), Some(101), "{args:?} panicked:\n{stderr}");
    assert!(!stderr.contains("panicked"), "{args:?} panicked:\n{stderr}");
}

#[test]
fn non_utf8_input_does_not_panic() {
    // 0xFF never occurs in UTF-8.
    assert_no_panic(&[OsStr::from_bytes(b"\xff.ncm")]);
}

#[test]
fn non_utf8_lang_value_does_not_panic() {
    let bad = OsStr::from_bytes(b"\xff");
    assert_no_panic(&[OsStr::new("--lang"), bad]);
    assert_no_panic(&[OsStr::new("-L"), bad]);
    assert_no_panic(&[OsStr::from_bytes(b"--lang=\xff")]);
}

#[test]
fn lang_after_a_non_utf8_argument_still_applies() {
    // Only the scan localizes `--help`. If it stopped at the bad argument,
    // both runs would fall back to `NCM2MP3_LANG=en` and print the same.
    let help = |lang: &str| {
        let bad = OsStr::from_bytes(b"\xff.ncm");
        let out = ncm2mp3(&[
            bad,
            OsStr::new("--lang"),
            OsStr::new(lang),
            OsStr::new("--help"),
        ]);
        assert!(out.status.success(), "{out:?}");
        String::from_utf8(out.stdout).unwrap()
    };
    assert_ne!(help("zh"), help("en"));
}

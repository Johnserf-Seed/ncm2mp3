//! How the first positional argument is dispatched: an existing path is
//! INPUT even when it's a prefix of a subcommand name, while bare words
//! that aren't paths still infer the subcommand. These run the real binary
//! so each case gets its own working directory.

use std::path::Path;
use std::process::{Command, Output};

/// Run the built binary in `cwd` with English messages.
fn ncm2mp3(cwd: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ncm2mp3"))
        .args(args)
        .current_dir(cwd)
        .env("NCM2MP3_LANG", "en")
        .output()
        .expect("failed to run ncm2mp3")
}

#[test]
fn existing_dir_named_like_a_subcommand_prefix_is_input() {
    let dir = tempfile::tempdir().unwrap();
    // Unique prefixes of `info`, `cover`, `watch`, `completion` and clap's
    // built-in `help`.
    for name in ["in", "cov", "w", "comp", "h"] {
        std::fs::create_dir(dir.path().join(name)).unwrap();
        // INPUT first, as in the original report, and after other options.
        for args in [
            [name, "-o", "out", "--no-config"],
            ["--no-config", "-o", "out", name],
        ] {
            let out = ncm2mp3(dir.path(), &args);
            let stderr = String::from_utf8_lossy(&out.stderr);
            assert!(
                out.status.success()
                    && stderr.contains(&format!("no .ncm files found under {name}")),
                "`ncm2mp3 {}` should decrypt the empty `{name}` folder:\n{stderr}",
                args.join(" "),
            );
        }
    }
}

#[test]
fn bare_prefix_that_is_not_a_path_infers_the_subcommand() {
    // Empty, so none of the words below names a path.
    let dir = tempfile::tempdir().unwrap();

    let out = ncm2mp3(dir.path(), &["comp", "bash"]);
    assert!(out.status.success(), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stdout).contains("_ncm2mp3()"));

    // `inf` becomes `info`, which then asks for its own INPUT.
    let out = ncm2mp3(dir.path(), &["inf"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "{stderr}");
    assert!(stderr.contains("info <INPUT>"), "{stderr}");

    // Not a prefix of anything, so main.rs suggests the closest name.
    let out = ncm2mp3(dir.path(), &["wath"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "{stderr}");
    assert!(stderr.contains("Did you mean `watch`?"), "{stderr}");
}

#[test]
fn exact_subcommand_name_wins_over_a_same_named_dir() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("info")).unwrap();

    let out = ncm2mp3(dir.path(), &["info"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "{stderr}");
    assert!(stderr.contains("info <INPUT>"), "{stderr}");
}

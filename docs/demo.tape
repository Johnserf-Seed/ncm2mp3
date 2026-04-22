# docs/demo.tape — vhs script to record docs/demo.gif
#
# Prerequisites (macOS / Linux):
#   1. `brew install vhs ffmpeg`        (macOS)
#   2. `cargo install --path crates/ncm-cli`
#      (puts `ncm2mp3` on PATH via ~/.cargo/bin)
#   3. Put a personally-owned `sample.ncm` at the project root.
#
# Regenerate:
#   vhs docs/demo.tape
#
# Output:
#   docs/demo.gif
#
# Windows note: vhs has known PTY issues on Windows (several tapes hang
# indefinitely on `Set Shell`). If you're on Windows, use ScreenToGif
# (https://www.screentogif.com/) to record the same sequence by hand.

Output docs/demo.gif

Set FontSize 16
Set Width  960
Set Height 540
Set Theme  "Dracula"
Set Padding 20

# Scene 1: inspect a file without decrypting
Type "ncm2mp3 info sample.ncm"
Sleep 400ms
Enter
Sleep 2500ms

# Scene 2: decrypt a single file, default filename preservation
Type "ncm2mp3 sample.ncm -o ./out"
Sleep 400ms
Enter
Sleep 2500ms

# Scene 3: folder mode — each song in its own folder + separate cover file
Type "ncm2mp3 sample.ncm -o ./out-folder -F"
Sleep 400ms
Enter
Sleep 2000ms
Type "ls out-folder/"
Sleep 300ms
Enter
Sleep 2500ms

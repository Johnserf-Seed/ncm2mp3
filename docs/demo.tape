# docs/demo.tape — vhs script to record docs/demo.gif
#
# Prerequisites:
#   1. Install vhs: https://github.com/charmbracelet/vhs
#      (macOS: `brew install vhs`, Windows/Linux: see releases page)
#   2. Build a release binary first: `cargo build --release`
#   3. Put a real (small, personally-owned) .ncm file at ./sample.ncm
#      — the recording needs a live file to exercise the full flow.
#
# Regenerate:
#   vhs docs/demo.tape
#
# Output: docs/demo.gif (referenced from README.md / README_en.md)

Output docs/demo.gif

Set FontSize 16
Set Width  960
Set Height 540
Set Theme  "Dracula"
Set Padding 20
Set Shell  "bash"

# Scene 1: inspect a file without decrypting
Type "./target/release/ncm2mp3 info sample.ncm"
Sleep 500ms
Enter
Sleep 2s

# Scene 2: decrypt a single file, default filename preservation
Type "./target/release/ncm2mp3 sample.ncm -o ./out"
Sleep 500ms
Enter
Sleep 2s

# Scene 3: folder mode — wraps each song with a separate cover file
Type "./target/release/ncm2mp3 sample.ncm -o ./out-folder -F"
Sleep 500ms
Enter
Sleep 1500ms
Type "ls ./out-folder/*/"
Enter
Sleep 2s

# Fin
Type "clear"
Enter
Sleep 500ms

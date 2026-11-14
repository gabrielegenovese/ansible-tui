# Regenerate the README recording from the repository root with:
#   cargo build --release
#   vhs docs/demo.tape
Output docs/demo.gif
Set Shell "bash"
Set FontSize 14
Set Width 1100
Set Height 700
Set Padding 20
Set Theme "Catppuccin Mocha"
Set TypingSpeed 40ms

Hide
Type "./target/release/ansible-tui demo playbook.yaml"
Enter
Sleep 2s
Show
Sleep 2s

# The `maintenance` play is a single action even though its imported tasks
# also carry pull/restart/prune tags.
Down
Sleep 400ms
Down
Sleep 1.5s

# Run the web deployment play.
Up
Sleep 400ms
Enter
Sleep 6s

# Browse the inventory and host_vars screens.
Type "2"
Sleep 1.5s
Type "3"
Sleep 1.5s
Type "1"
Sleep 1.5s
Type "q"
Sleep 500ms

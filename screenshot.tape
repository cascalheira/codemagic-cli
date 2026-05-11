# VHS tape — capture a screenshot of codemagic-cli TUI
Output screenshots/screenshot.png

Set Shell "bash"
Set Theme "Catppuccin Mocha"
Set FontSize 14
Set Width 1200
Set Height 700
Set Padding 20

# Launch the TUI — no config exists so onboarding screen appears
Type "CODEMAGIC_API_TOKEN='' ./target/release/codemagic-cli"
Sleep 200ms
Enter
Sleep 2s

Screenshot screenshots/screenshot.png

# Quit
Escape
Sleep 200ms

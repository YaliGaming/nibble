# Third-party software

Nibble's own code is under the MIT License (see `LICENSE`). It is written in
Rust and built from the crates listed in `Cargo.lock`, each under its own
open-source license (mostly MIT and Apache-2.0). The installed app also uses:

| What | Used for | License |
| --- | --- | --- |
| [Tauri](https://tauri.app/) and [wry](https://github.com/tauri-apps/wry) | windows, tray icon, global shortcut, dialogs | MIT or Apache-2.0 |
| Microsoft Edge WebView2 | drawing the pet and the panel; it is part of Windows 10 and 11 | Microsoft's own license |

Nibble is a community project. It is not made by, endorsed by, or affiliated
with Anthropic. "Claude" and "Claude Code" are trademarks of Anthropic, used
here only to say what the app works with.

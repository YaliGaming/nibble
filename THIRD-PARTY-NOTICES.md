# Third-party software

Nibble's own code is under the MIT License (see `LICENSE`) and has no runtime
dependencies from npm. The installed app does include two other projects:

| What | Used for | License | Where its license text is |
| --- | --- | --- | --- |
| [Electron](https://www.electronjs.org/) (with Chromium) | the app itself | MIT; Chromium's components under their own licenses | `LICENSE.electron.txt` and `LICENSES.chromium.html`, next to `Nibble.exe` |
| [Node.js](https://nodejs.org/) | running the Claude Code hook and the `/pet` command | MIT; bundled components under their own licenses | `resources/node/LICENSE` |

Nibble is a community project. It is not made by, endorsed by, or affiliated
with Anthropic. "Claude" and "Claude Code" are trademarks of Anthropic, used
here only to say what the app works with.

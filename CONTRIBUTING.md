# Contributing to Nibble

Thanks for wanting to help. Pets and translations are the easiest places to start, and neither needs you to read the app's code.

## Setting up

You need Windows and [Node.js](https://nodejs.org/) 22 or later.

```bash
npm install
```

```bash
npm start
```

To try the app without touching your own Claude Code settings, set two environment variables before starting it: `NIBBLE_HOME` (where the pet keeps its data, normally `~/.nibble-pet`) and `CLAUDE_CONFIG_DIR` (where it looks for Claude Code's settings, normally `~/.claude`). Point both at empty folders.

## Adding a pet

A pet is one JSON file of pixel art in `pets/`. The fastest way to make one is to let Claude draw it: type a description in the pet's settings and press Hatch, or run `/hatch-pet a sleepy owl` in Claude Code. The result lands in `~/.nibble-pet/pets/`. Tweak it by hand if you like, then copy it into `pets/` under a short lowercase name.

The format, in short (the full description is in [`skill/hatch-pet/SKILL.md`](skill/hatch-pet/SKILL.md)):

```json
{
  "format": 1,
  "name": "Ember",
  "palette": { "o": "#D97757", "k": "#2A1F1A" },
  "base": ["................", "....o......o....", "..."],
  "frames": { "blink": { "6": ".oooooooooooooo." } }
}
```

- `base` is the pet standing still: rows of characters, all the same length, 16x16 by default and at most 32x32. `.` is a transparent pixel; every other character is a key in `palette`.
- `frames` are variations of `base`, listing only the rows that change: `blink`, `walk1`, `walk2`, `happy`, `sad`, `alert`, `sleep`. All are optional.
- The app adds the outline and all movement itself.

The app checks a pet when it loads it and leaves out one that is not valid, so start the app and make sure yours shows up in the settings. A screenshot of it in a few states is a good thing to attach to the pull request.

A pet should be your own drawing. Please do not submit characters that belong to someone else.

## Adding a translation

Each language is one file in `locales/`, named by its language code (`fr.json`, `pt-BR.json`).

1. Copy `locales/en.json` to the new name.
2. Set `"$name"` to the language's name in that language ("Français"), and `"$dir"` to `"ltr"` or `"rtl"`.
3. Translate the values. Leave the keys, and anything in `{braces}`, as they are.

The language then appears in the settings by itself, and is picked automatically for people whose system is set to it. A string that is missing from a translation is shown in English, so a partial translation still works.

For a right-to-left language, also add its Chromium locale code to `electronLanguages` in `package.json`, so the native right-click menu opens in the right direction in the installed app.

Look at every screen in the new language before sending it: the panel, a session's page, and the settings.

## Changing the code

- `hook/pet-hook.js` and `cli/pet.js` are copied into `~/.nibble-pet` and run on their own by Claude Code, so they cannot `require` anything from the app. Both must always exit 0 and print nothing unexpected: a hook that fails or chatters gets in the way of someone's work.
- `lib/reducer.js` is the state machine. It is a pure function of a session's event log.
- `lib/connect.js` edits the user's `settings.json`. It must only ever add, change or remove entries that point at Nibble's own two scripts.
- Text shown to people lives in `locales/`, never in the code.
- No runtime dependencies. The app is small enough to read in an afternoon; please help keep it that way.

## Reporting a problem

Say which version of Windows and of Claude Code you have (`claude --version`), whether the session was in a terminal or in the Claude desktop app, and what the pet showed compared with what you expected. If it is about a status being wrong, the session's log in `~/.nibble-pet/events/` is the most useful thing to attach. Read it first: unless you turned off "Keep the text of prompts and answers", it contains the beginnings of your prompts.

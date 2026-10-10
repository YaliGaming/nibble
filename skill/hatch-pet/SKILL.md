---
name: hatch-pet
description: Create a custom pixel-art pet for the Nibble desktop companion. Use when the user asks to hatch, create, design or draw a new pet (for example "/hatch-pet a purple dragon").
---

# Hatch a pet

Nibble is a floating desktop companion that follows Claude Code sessions. A pet
is one small JSON file of pixel art. Design one from the user's description, save
it, and tell the user to pick it in the pet's Settings.

If the user gave no description, ask what the pet should look like before drawing.

## Pet format

A pet is a JSON object:

```json
{
  "format": 1,
  "name": "Ember",
  "palette": { "o": "#D97757", "d": "#B5573A", "c": "#F2A98C", "k": "#2A1F1A", "w": "#FFFFFF", "f": "#8F4630" },
  "base": [
    "................",
    "....o......o....",
    "...ooo....ooo...",
    "...oooooooooo...",
    "..oooooooooooo..",
    ".oooooooooooooo.",
    ".ooowkoooowkooo.",
    ".oookkooookkooo.",
    ".occoooooooocco.",
    ".ooooookkoooooo.",
    ".oooooooooooooo.",
    "..oooooooooooo..",
    "..dooooooooood..",
    "...dddddddddd...",
    "...ff......ff...",
    "................"
  ],
  "frames": {
    "blink": { "6": ".oooooooooooooo.", "7": ".oookkooookkooo." },
    "walk1": { "14": "...ff..........." },
    "walk2": { "14": "...........ff..." },
    "happy": { "6": ".oookkooookkooo.", "7": ".ookookookookoo." },
    "sad":   { "6": ".oooooooooooooo.", "7": ".oookkooookkooo.", "10": ".oooookookooooo." },
    "alert": { "5": ".oookkooookkooo." },
    "sleep": { "6": ".oooooooooooooo.", "7": ".oookkooookkooo." }
  }
}
```

Rules:

- `base` is the pet standing still, drawn as rows of characters, top row first.
  Every row must have exactly the same length. Use 16x16 unless the design needs
  more; the limit is 32x32.
- `.` (or a space) is a transparent pixel. Every other character must be a key in
  `palette`, and each palette key is exactly one character mapped to a `#rrggbb`
  colour. Keep the palette small, 4 to 8 colours.
- `frames` are variations of `base`. Each frame lists only the rows that differ,
  as `"rowNumber": "replacement row"` (rows count from 0, and a replacement row is
  the full row, the same length as the others). All frames are optional, but a
  pet with all seven feels alive:
  - `blink`: eyes closed for a moment while idle.
  - `walk1`, `walk2`: alternated while Claude is working. Move the feet, a tail,
    an antenna light.
  - `happy`: shown when a task finishes. Curved eyes, open mouth.
  - `sad`: shown when a task fails. Lowered eyes, frown, a sweat drop.
  - `alert`: shown when Claude needs the user's input. Wide eyes.
  - `sleep`: shown when nothing is running. Closed eyes.
- The app adds a dark one-pixel outline and all movement (bobbing, hopping,
  sparkles) by itself, so draw only the pet. Set `"outline": false` to turn the
  outline off.
- Leave row 0 and the last row mostly empty, and keep the pet centred, so it
  does not look cropped when it hops.

Drawing advice: decide the silhouette first and keep it symmetrical where the
creature is symmetrical. Give it two eyes at least 2 pixels wide and a readable
face, a slightly darker shade along the bottom edge for volume, and feet or a
base on the bottom row so it looks grounded. Before answering, count the
characters in every row and check that each one is the same length as row 0.

## Saving the pet

Write the JSON to the pet's custom folder, one file per pet, named after the pet
in lowercase with dashes:

- Windows: `%USERPROFILE%\.nibble-pet\pets\<name>.json`
- macOS and Linux: `~/.nibble-pet/pets/<name>.json`

Create the folder if it does not exist. Then tell the user to open the pet's
Settings, press Refresh, and choose the new pet.

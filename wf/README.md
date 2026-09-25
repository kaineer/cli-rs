# wf

Quick wezterm font/ligature switcher.

Copies preset fragments in `~/.config/wezterm/` so the live config picks them up:

| source | target |
| --- | --- |
| `ligature.enabled.lua` / `ligature.disabled.lua` | `ligature.lua` |
| `font.25.lua` / `font.16.lua` | `font.lua` |

## Build

```bash
cargo build --release
```

## Usage

```text
 $ wf ly           # turn ligatures on
 $ wf ln           # turn ligatures off
 $ wf ly 25        # ligatures on, then font size 25
 $ wf ln 16        # ligatures off, then font size 16
 $ wf lig on       # turn ligatures on
 $ wf lig off 25   # ligatures off, then font size 25
 $ wf 25           # set font size 25
 $ wf 16           # set font size 16
 $ wf              # set font to 16, set ligatures off
```

Ligature commands (`ly` / `ln` / `lig` / `ligature`) always set ligatures first.
If a second argument is present, they then set the font size.

### Ligatures

| command | effect |
| --- | --- |
| `ly` | on |
| `ln` | off |
| `lig on` / `lig yes` | on |
| `lig off` / `lig no` | off |

Aliases: `ligature` ≡ `lig`.

### Font size

| command | effect |
| --- | --- |
| `25` / `f 25` / `f large` / `f big` | size 25 |
| `16` / `f 16` / `f small` | size 16 |

Aliases: `font` ≡ `f`.

### Default

With no arguments (or an unknown command): ligatures off, font size 16.

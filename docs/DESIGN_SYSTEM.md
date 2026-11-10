# tnls — Design System

> **In this repo:** tokens live in `src/styles/tokens.css`, components in
> `src/styles/app.css` (both copied verbatim from the OpenDesign export), and
> the icon sprite in `src/components/Icon.tsx`. File names below that are not
> in this repo (`colors_and_type.css`, `preview/`, `ui_kits/`) refer to the
> OpenDesign export package, which is kept outside version control.
> `docs/design-system.html` is the rendered spec page.

> Category: Project Design System
> Surface: desktop application (Tauri webview) plus web surfaces
> Source project: "Thiết kế giao diện tnls" (3502feba-7065-4eed-91a6-84bd0577f1bc)

---

## 1. Product context & visual theme

**tnls** is a desktop SSH tunnel manager: it stores tunnel profiles, starts and
stops them from a UI, and keeps running in the system tray. It exists because
the alternative is a dozen terminal windows each holding one `ssh -L` command
that you have to remember, re-type, and troubleshoot blind.

That origin sets the visual brief. This is not a marketing surface. It is an
**instrument panel for someone who is already in a terminal**, and it must read
at a glance while the user's attention is on a build log, a database client, or
a stack trace on the other monitor.

Three qualities carry the atmosphere:

**Instrument, not app.** The UI borrows from measurement hardware: cold
graphite surfaces, hairline rules, tabular numerals, monospace for every value
that is data. Nothing glows. Nothing is playful. A status is a reading, not a
decoration.

**One accent, held back.** A single desaturated teal (`#2dd4bf`) carries
everything that means *live, focused, or actionable*: a running tunnel, a
focused input, a selected row, the one primary button. Because it is the only
saturated color on the screen, it functions as a signal rather than a brand
gesture. The moment a second accent appears, the first one stops meaning
anything.

**State is the content.** The product's whole value is answering "which tunnels
are up?" Seven runtime states each get a color, an icon, a label, and a row
rail — reinforced four ways, never color alone.

Both themes are authored independently. Light is paper-cool slate and teal on
white, not a dark theme inverted. Inverting produces dead mid-greys and
desaturated semantics; the source app does not do it and neither should any
surface built on this system.

---

## 2. Color

### 2.1 Surface ramp (dark — primary)

Cold graphite with a blue bias. `ground-0` is `#0c0f13`, deliberately not pure
black: a true black ground makes the hairlines disappear and forces brighter
text, which reads as glare on a dark-themed desktop.

| Token | Value | Role |
| --- | --- | --- |
| `--ground-0` | `#0c0f13` | Page / table body background |
| `--ground-1` | `#11151a` | Topbar, drawer, sticky header |
| `--ground-2` | `#171c22` | Bars, menus, toasts, elevated chips |
| `--ground-3` | `#1e242c` | Nested wells, kbd keys, segmented tracks |
| `--ground-4` | `#262d36` | Icon-button hover, deepest inset |
| `--ground-hover` | `#1b2129` | Row hover — sits *between* 1 and 2 |

Every surface step in the ramp is roughly 6–8 lightness points apart. That
spacing is what lets a drawer sit on top of a table without a heavy shadow.

### 2.2 Light ramp

| Token | Value | Role |
| --- | --- | --- |
| `--ground-0` | `#f4f6f8` | Page background |
| `--ground-1` | `#fbfcfd` | Topbar, drawer |
| `--ground-2` | `#ffffff` | Cards, menus, crispest surface |
| `--ground-3` | `#eef1f4` | Wells, keys |
| `--ground-4` | `#e3e8ed` | Deepest inset |

Light inverts the *direction* of the ramp as well as the values: in dark,
`ground-0` is the darkest and `ground-4` the lightest. In light, `ground-2`
(white) is the brightest and `ground-4` the darkest. Components never assume
"page is lighter than card" — they use the tokens.

### 2.3 Lines

Three weights only. A fourth would make every border a judgment call.

| Token | Dark | Light | Role |
| --- | --- | --- | --- |
| `--line-faint` | `#1f262e` | `#e6ebf0` | Table row separators, section dividers |
| `--line` | `#2a323d` | `#d5dde4` | Structural borders, input borders |
| `--line-strong` | `#3a444f` | `#b7c2cd` | Hover borders, checked controls, menu edges |

### 2.4 Text

Three levels, and no more. When a fourth "slightly dimmer" level gets added,
`text-2` and `text-3` stop being distinguishable and the hierarchy collapses.

| Token | Dark | Light | Role |
| --- | --- | --- | --- |
| `--text-1` | `#e8edf2` | `#101820` | Primary content, names, values |
| `--text-2` | `#a3afbd` | `#4a5a68` | Secondary labels, descriptions |
| `--text-3` | `#6f7d8c` | `#6d7d8b` | Column headers, timestamps, hints |
| `--text-inverse` | `#0c0f13` | `#ffffff` | Text on light fills |

`--text-3` on `--ground-0` in dark is ~4.6:1 and in light ~4.7:1 — it clears AA
for body copy at these sizes, which is why it is safe for column headers rather
than reserved for decoration.

### 2.5 Accent

| Token | Dark | Light | Role |
| --- | --- | --- | --- |
| `--accent` | `#2dd4bf` | `#0d9488` | Focus rings, checked controls, primary fill |
| `--accent-strong` | `#5eead4` | `#0f766e` | Primary button hover |
| `--accent-dim` | `#0f3f3c` | `#ccfbf1` | Selection wash |
| `--accent-text` | `#5eead4` | `#0f766e` | Accent used as text (never as fill) |
| `--on-accent` | `#04120f` | `#ffffff` | Text on accent fill |

`--on-accent` is a near-black with a teal bias in dark mode (`#04120f`), not
white. White on `#2dd4bf` is about 1.9:1 — the brightest fill in the system
therefore takes the darkest text.

### 2.6 Semantic tones

Each tone is a **triple**: foreground, tinted background, border. The borders
are never the plain foreground color at low alpha — they are separate,
pre-mixed values, so a badge edge is visible on both the tint and the page.

| Tone | Dark fg / bg / line | Light fg / bg / line |
| --- | --- | --- |
| **ok** (running) | `#34d399` / `#0d2e26` / `#1b5a4b` | `#059669` / `#e7f8f1` / `#a7e3cc` |
| **warm** (transitional) | `#fbbf24` / `#33280a` / `#6b520f` | `#b45309` / `#fdf3e0` / `#f0d4a4` |
| **warn** (degraded) | `#fb923c` / `#341d0b` / `#6d3c14` | `#c2410c` / `#fdeee4` / `#f3c9ae` |
| **danger** (failed) | `#f87171` / `#38171a` / `#6f2c31` | `#c02626` / `#fdeaea` / `#f2bcbc` |
| **info** (unknown) | `#7dd3fc` / `#0c2733` / `#1c4a5e` | `#0369a1` / `#e8f4fb` / `#b3d9ee` |

`--danger-solid` (`#b91c1c` in both themes) is the only filled danger surface —
destructive buttons. Everything else about failure uses the tinted triple.

### 2.7 The seven runtime states

This is the heart of the system. Every state maps to four slots — `fg`, `bg`,
`line`, `rail` — and those four slots are consumed by badges, row rails, and
log levels alike. A state therefore cannot look like one thing in the badge and
another thing on the row.

| State | Tone | Icon | Meaning |
| --- | --- | --- | --- |
| `stopped` | text-3 on ground-3 | `i-circle` | Not running, no error. Rail hidden. |
| `starting` | warm | `i-loader` (spinning) | Connecting; port not bound yet. |
| `running` | ok | `i-dot` (breathing) | Process alive, port bound. |
| `degraded` | warn | `i-alert` | Alive but not healthy — retrying, partial bind. |
| `stopping` | warm | `i-loader` (spinning) | Shutdown in progress. |
| `failed` | danger | `i-alert` | Exited with an error; error text in detail column. |
| `unknown` | info | `i-circle` | Backend unreachable — we genuinely do not know. |

`warm` and `warn` are deliberately *different* oranges (`#fbbf24` amber vs
`#fb923c` orange) because `starting`/`stopping` and `degraded` are different
problems with different user responses: wait, versus investigate. The source
brief flagged that these were previously conflated; the shipped tokens split
them.

### 2.8 Instrument surfaces

`--log-bg` (`#080b0e` dark, `#101820` light) and `--log-text` (`#b8c4d0` /
`#c8d4df`) are a separate pair because the log drawer is a *nested instrument*,
not a page. In light mode the log is still dark — a dark well on a light page,
like a terminal pane. Do not derive log colors from the page ramp.

---

## 3. Typography

### 3.1 Families

| Token | Stack | Job |
| --- | --- | --- |
| `--font-ui` | `"Söhne", "Söhne Buch", -apple-system, "Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif` | Every UI label, button, field, description |
| `--font-display` | `"Söhne Breit", "Söhne", "Segoe UI Variable Display", -apple-system, "Segoe UI", system-ui, sans-serif` | Wordmark, drawer titles, modal titles, empty-state titles |
| `--font-mono` | `"JetBrains Mono", "SF Mono", ui-monospace, "Cascadia Mono", "Roboto Mono", Menlo, Consolas, monospace` | Ports, addresses, PIDs, uptime, logs, SSH preview, shortcuts |

No font files ship with this package — the source project also shipped none,
relying on local Söhne installs with a system fallback. See §10 for the
licensed-font note.

**The mono rule:** if a value could be diffed against another value character
by character, it is mono. Ports, bind addresses, remote hosts, PIDs, uptimes,
timestamps, log lines, SSH command previews, keyboard shortcuts, and numerals
inside stat pills. Names, labels, and prose are never mono.

**Tabular numerals** (`font-variant-numeric: tabular-nums`) are set on every
mono data cell, every stat numeral, and every log line, so numbers do not
shimmy when they tick.

### 3.2 Scale

1.2 ratio. Nine steps, and the two that matter most are adjacent:

| Token | px | Weight | Use |
| --- | --- | --- | --- |
| `--text-2xs` | 11 | 550 | Caps labels, column headers, kbd keys, log level tags |
| `--text-xs` | 12 | 400–550 | Data cells, hints, helper text, log lines |
| `--text-sm` | 13 | 550 | Buttons, inputs, menus, drawer body copy |
| `--text-md` | 14 | 550 | Table name cells, empty-state description |
| `--text-base` | 15 | 400 | Document body — the reading size |
| `--text-lg` | 17 | 620 | Drawer and modal titles |
| `--text-xl` | 20 | 620 | Wordmark, empty-state title |
| `--text-2xl` | 24 | 620 | Section openers (rare) |
| `--text-3xl` | 30 | 620 | Largest display step; not used in the app UI |

A desktop tool runs one step "tighter" than a website: the app's reading size
is 15px, its data size 12–13px, and its heaviest heading 20px. Resist the urge
to scale the whole table up — density is the feature.

### 3.3 Leading and tracking

| Token | Value | Applies to |
| --- | --- | --- |
| `--leading-tight` | 1.2 | Display titles, name cells, segmented labels |
| `--leading-snug` | 1.4 | Sub-labels, hints, helper text, field errors |
| `--leading-body` | 1.55 | Document body, modal text, empty-state description |
| `--tracking-caps` | 0.07em | Uppercase caps labels — the tracking uppercase needs |
| `--tracking-label` | 0.02em | Sentence-case button and field labels |
| `--tracking-display` | -0.015em | Display type at 17px and above |

Uppercase text is *always* tracked (`--tracking-caps`) and *never* set in mono
with tabular figures — wide caps with tabular numerals reads as broken.

### 3.4 Weights

Söhne ships a variable axis; three stops are used.

| Token | Value | Name | Use |
| --- | --- | --- | --- |
| `--weight-read` | 400 | read | Body, hints, secondary prose |
| `--weight-emphasize` | 550 | emphasize | Buttons, field labels, table names, stat numerals |
| `--weight-announce` | 620 | announce | Display titles, primary buttons, section titles, log levels |

There is no 300 and no 700. A design that needs bold-700 for emphasis has a
hierarchy problem, not a weight problem.

---

## 4. Spacing, radius, and geometry

### 4.1 Spacing — a strict 4px rhythm

`--s-1` 4 · `--s-2` 8 · `--s-3` 12 · `--s-4` 16 · `--s-5` 20 · `--s-6` 24 ·
`--s-8` 32 · `--s-10` 40 · `--s-12` 48 · `--s-16` 64

The scale is not evenly populated (there is no `--s-7`, `--s-9`, `--s-11`).
Steps that got used repeatedly were kept; the rest were dropped so the scale
cannot drift. Anything not on this list is a bug.

Observed pairings worth reusing:

- Icon button padding: `--s-2` horizontal on a 34px hit area.
- Table cell: `--s-2` vertical / `--s-3` horizontal.
- Field stack: 6px between label and input, `--s-4` between fields.
- Drawer body: `--s-5` all round; drawer head/foot `--s-4` / `--s-5`.
- Section blocks inside a form: `--s-6` apart.

### 4.2 Radius

| Token | Value | Use |
| --- | --- | --- |
| `--r-xs` | 4px | Checkboxes, kbd keys, mode chips, small icon buttons |
| `--r-sm` | 6px | Buttons, inputs, selects, menu items, row action buttons |
| `--r-md` | 8px | Menus, toasts, segmented control, preview block, switch rows |
| `--r-lg` | 12px | Modal shell |
| `--r-pill` | 999px | Status badges, switches, followers, jump button |

Rule of thumb: **the radius scales with the container.** A 28px button never
takes a 12px radius; a 440px modal never takes 6px.

### 4.3 Fixed geometry

| Token | Value | What it is |
| --- | --- | --- |
| `--topbar-h` | 60px | Topbar height |
| `--bulkbar-h` | 56px | Sticky bulk bar height |
| `--row-h` | 48px | Table row height |
| `--drawer-w` | 520px | Form drawer width |
| `--log-drawer-w` | 680px | Log drawer width (wider: it is a terminal) |
| `--page-pad` | 24px | Page gutter — 16px at ≤960px, 12px at ≤760px, 32px at ≥1600px |
| `--icon-sm/md/lg` | 14 / 16 / 20px | Icon sizes; almost all UI icons are 13–16px |

Icon geometry is set per instance with `--od-icon-size`, defaulting to 14px.
Every inline SVG carries explicit width/height from that variable — without it
the UA box is 300×150 and icons explode.

---

## 5. Layout & composition

### 5.1 The app shell

A two-row grid, full viewport height, no page scroll:

```
┌──────────────────────────────────────────────────┐
│ topbar                                  60px     │  brand · stats · search · actions
├──────────────────────────────────────────────────┤
│ table toolbar                         ~44px      │  result count · spacer · shortcut hint
│ ┌──────────────────────────────────────────────┐ │
│ │ table (sticky thead, own scroll)             │ │  minmax(0, 1fr)
│ └──────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────┘
   bulk bar (absolute, slides up over the last row)
```

`grid-template-rows: var(--topbar-h) minmax(0, 1fr)` — the `minmax(0, 1fr)`
matters: without it a tall table forces the shell to grow and the whole window
scrolls instead of the table.

**Topbar composition:** brand at the left, then stats separated by a 1px
`--line` divider (which doubles as the visual break between identity and
telemetry), a flexible spacer, then search and actions at the right. Stats
disappear below 960px; the search field shrinks first, then wraps to its own
row below 760px.

**The wordmark** is the app name in display type with a 5px teal dot appended
via `::after` at `vertical-align: 0.42em`. That dot is the brand mark — there
is no logo file. Reproduce it rather than inventing a raster mark.

### 5.2 The table

`table-layout: fixed` with an explicit `<col>` for every column. Fixed layout
is what keeps the data columns from reflowing when a hostname gets long; every
cell then truncates internally instead of pushing its neighbours.

| Column | Width | Content |
| --- | --- | --- |
| check | 44px | Row checkbox |
| name | 20% | Name (550) over sub-label (12px, text-3) |
| mode | 104px | `-L` / `-R` / `-D` chip |
| local | 172px | Mono bind address + copy affordance |
| remote | 200px | Mono host:port + copy affordance |
| status | 132px | Badge |
| detail | auto | Mono readout, or error text |
| actions | 116px | Play/stop, logs, overflow |

**Row rail.** The first cell carries a 2px inset bar keyed to the runtime
state, inset 6px from top and bottom with a right-only radius. It is the
fastest perceptual channel in the whole UI — you scan the left edge, not the
badges.

**Sticky header.** `position: sticky; top: 0; z-index: 2` on `thead th`, with
`background: var(--ground-1)` and a bottom border. Only the header lifts; the
first column does *not* stick.

**Selection.** A selected row tint is
`color-mix(in oklab, var(--accent) 8%, var(--ground-0))` — a wash, not a fill.
`color-mix` in oklab keeps the tint neutral; mixing in sRGB at that alpha
produces a muddy green-grey.

**Row removal** animates out over `--dur-base` with `--ease-in`:
`opacity → 0` plus `translateX(-12px)`. Exit left, because that is the motion
of being pulled off a list.

### 5.3 Drawers

Form drawer 520px from the right; log drawer 680px. Both are
`grid-template-rows: auto minmax(0, 1fr) auto` — head, scrolling body, footer —
so the actions never scroll away.

Enter: `translateX(100%) → 0` over `--dur-drawer` (320ms) with
`--ease-standard` `cubic-bezier(0.32, 0.72, 0, 1)`. That long, flat-out curve
is what makes a 520px panel feel like it is being pushed rather than flung.
Backed by a full-screen scrim at `--overlay` with `backdrop-filter: blur(2px)`.

At ≤760px both drawers become `100vw`. They never become bottom sheets: on a
desktop tool the edge-anchored panel is the correct metaphor even when narrow.

### 5.4 Density and restraint

- No cards inside cards. Sections are separated by hairlines, not boxes.
- No drop shadows on static surfaces. Shadows belong to layers that float:
  drawer, modal, menu, toast, jump-to-bottom.
- One primary button per viewport. The rest are ghost, icon, or danger.
- Sibling regions never stack via absolute positioning. Absolute is reserved
  for the bulk bar (reserved padding compensates), the scrim, drawers, menus,
  modals, and anchored overlays.

### 5.5 Responsive

| Breakpoint | Change |
| --- | --- |
| ≥1600px | `--page-pad` → 32px; the detail column gets the extra width |
| ≤1180px | remote/local columns narrow; search → 220px; shortcut hint hidden |
| ≤960px | `--page-pad` → 16px; mode and detail columns drop; stats hidden |
| ≤760px | `--page-pad` → 12px; drawers full width; topbar wraps; remote column drops; form grid and segmented control collapse to one column; log level tags hidden |

Columns are dropped, never horizontally scrolled. A tunnel manager with a
sideways-scrolling table would be worse than a shorter table.

---

## 6. Components

### 6.1 Buttons

34px tall, `--r-sm`, `--text-sm`, weight 550, `--tracking-label`, transparent
border. Four variants:

| Class | Look | Use |
| --- | --- | --- |
| `.btn--primary` | Accent fill, `--on-accent` text, weight 620 | The one main action |
| `.btn--ghost` | `--line` border, `--text-2` | Secondary actions |
| `.btn--icon` | 34×34, no border, `--text-2` | Toolbar icons |
| `.btn--danger` | `--danger-solid` fill | Destructive confirm |
| `.btn--danger-ghost` | Danger text, no border | Delete affordance |

`.btn--sm` drops to 28px for in-row actions. Disabled is `opacity: 0.45` and
`cursor: not-allowed` — never a color change, because a greyed-out accent
button still reads as "the important one".

Hover never lightens text toward grey. Ghost buttons get `--ground-hover` and
move *up* to `--text-1`.

### 6.2 Status badge

Pill, 24px tall, `--r-pill`, 12px/550 text, with an icon + label + optional
retry count. Slot-driven: the element sets `--b-fg`, `--b-bg`, `--b-line` from
`data-state`, and the base `.badge` rule consumes them. Adding a state means
adding one `[data-state]` block, nothing else.

- **Running** icon breathes on a 2.4s loop, opacity 1 → 0.55. A quiet ring of
  attention, not a neon glow.
- **Starting / stopping** swap the icon for a spinner spinning at 780ms —
  and a *slower* 2s under `prefers-reduced-motion`, because a frozen spinner
  reads as a hung process.
- **State change flash** on `.is-changing`: scale 0.92 → 1.04 → 1 over 180ms.
  Transitions have to be perceptible or the user misses them entirely.
- **Retry chip** `.badge__retry` shows `×N` in mono, split off by a 1px border.

The badge is the *secondary* state channel. The rail is primary. Both must
agree.

### 6.3 Address cells (copy affordance)

Mono value at 12px, ellipsis-truncated, followed by a 22×22 copy button at
`opacity: 0` that appears on row hover, on focus-visible, and after a
successful copy. Copy success flips the icon color to `--ok` for a beat.

The rule: **never show a copy button for something already copied.** State the
button carries meaning only if its absence is also meaningful.

### 6.4 Mode chip

Uppercase 11px at `--tracking-caps`, 22px tall, `--r-xs`, `--ground-3` fill.
Carries a mono flag inline: `LOCAL -L`, `REMOTE -R`, `DYNAMIC -D`. The flag is
reset to `text-transform: none` and `letter-spacing: 0` so the flag stays
literal — it is the actual CLI argument.

### 6.5 Form controls

| Control | Height | Notes |
| --- | --- | --- |
| `.input`, `.select` | 36px | `--r-sm`, `--input-bg`, `--line` border |
| `.search__input` | 34px | Same treatment, icon at left, `kbd`/clear at right |
| `.check` | 16px | `--r-xs`, 1.5px border, custom tick drawn with borders |
| `.switch` | 34×20 | Pill track, 14px knob; accent when on |

Focus on any text field: border → `--accent`, background → `--input-bg-focus`,
plus `0 0 0 3px color-mix(in oklab, var(--accent) 16%, transparent)`.

The **search field is a compound instrument**: left icon that turns accent on
focus (`:has(.search__input:focus)`), right `kbd` hint that hides on focus or
on any content, and a right clear button that replaces it. Never show the `/`
hint and the clear button simultaneously.

**Segmented control** for forwarding mode: 3-up grid on a `--ground-3` track
with 3px padding, each option a stacked label + mono flag. Selected option
lifts to `--ground-1` with a `--line-strong` border and turns its flag to
`--accent-text`. Stacked label-over-flag means both are block elements — a
button child needs an explicit block display or the stack silently collapses.

**Advanced disclosure**: a full-width caps-label toggle carrying a rotating
chevron (90° over `--dur-base`). Collapsed by default; the advanced fields are
not part of the common path.

**SSH command preview** is the one "instrument read-out" in the form: a
`--log-bg` well with a header strip of three 7px dots and a mono command body
with tokenized colour — flags in `#7dd3fc`, target in `#fbbf24`, address in
`#5eead4`, options in `--text-2`, placeholders in `--text-3`. Those token
colours are fixed literals, not theme tokens, because the preview well is
always dark.

### 6.6 Menu

196px minimum, `--r-md`, `--ground-2`, `--shadow-pop`, `0.97 → 1` scale over
`--dur-fast`. Items 32px tall with optional leading icon, trailing mono
shortcut, separators as 1px `--line-faint`, and a caps-label group header.
`--danger` items tint their hover to `--danger-bg`.

### 6.7 Modal

`<dialog>`-based, 440px max, `--r-lg`, `--shadow-modal`, entering with
`translate(-50%, -48%) scale(0.97) → translate(-50%, -50%) scale(1)`. The 48%
start means it settles *downward* into place — a modal that only fades is
easy to miss. The head pairs a 34px danger-tinted icon tile with the title; the
footer sits on `--ground-2` to separate actions from the body.

### 6.8 Log drawer

The log is rendered as a genuine console:
`grid-template-columns: 68px 46px minmax(0, 1fr)` — time / level / message.

- Timestamps in `#6f7d8c`, tabular, fixed width so lines align.
- Level tags at 11px/620 with 0.04em tracking, colored per level.
- **ERROR rows** get `background: rgb(248 113 113 / 0.07)` plus a 2px inset
  red rail — the same rail vocabulary as table rows.
- Search matches get a teal wash; the actual matched token gets an amber mark.
- The level filter is a segmented mono strip where each pressed option adopts
  its own level colour.
- A `follow` toggle with a pulsing dot (1.8s) holds the scroll at the bottom;
  scrolling up reveals a floating "jump to bottom" pill.

Note that literal colours appear here (`#f87171`, `#fbbf24`, `#7dd3fc`,
`#6f7d8c`) rather than tokens: the log surface is always dark in both themes,
so page tokens would be wrong in light mode.

### 6.9 Toasts

Centered at the bottom, 440px max, `--r-md`, `--ground-2`, entering
`translateY(10px) → 0` over `--dur-base` and leaving over `--dur-fast` with
`--ease-in` — exits are 60–70% of entrances, and the asymmetry is what makes
them feel like they get out of the way. Tone icon colors: ok / danger / info /
warm.

### 6.10 Empty state

Centered column, max 520px: a 132×104 line illustration in `--line-strong`, a
display-type title, a 14px description capped at `46ch`, an inline mono
command hint, and ghost quick-actions. Two variants exist and must stay
distinct: **no tunnels at all** (onboarding) versus **no search matches**
(recovery — offer clearing the query, not creating a tunnel).

### 6.11 Keyboard

`/` focuses search, `Ctrl+N` creates a tunnel, `Esc` closes the topmost layer.
Shortcut hints use `<kbd>`: 11px mono, `--ground-3` fill, `--line` border,
`--r-xs`, 1px×5px padding. Inside the search field, the same hint reads as a
quiet placeholder until you type.

---

## 7. Motion & interaction

| Token | Value | Use |
| --- | --- | --- |
| `--dur-fast` | 120ms | Hover, focus, color, small transforms |
| `--dur-base` | 180ms | Scrim, toast, badge flash, chevron, row exit |
| `--dur-slow` | 260ms | Larger state emphasis |
| `--dur-drawer` | 320ms | Drawer slide |
| `--ease-out` | `cubic-bezier(0.16, 1, 0.3, 1)` | Entrances, hover, reveals |
| `--ease-in` | `cubic-bezier(0.5, 0, 0.84, 0)` | Exits, row removal |
| `--ease-standard` | `cubic-bezier(0.32, 0.72, 0, 1)` | Drawer travel, breathing loops |

Principles observed throughout the source:

- **Animate state, not decoration.** Every transition in the app expresses a
  change the user needs to perceive: a tunnel changing state, a drawer
  arriving, a row leaving, a copy landing.
- **Asymmetric exits.** Entrances get the long ease-out; exits get `--ease-in`
  at roughly 60–70% of the duration.
- **Loops are slow and shallow.** The running pulse is 2.4s at 32% opacity
  swing; the follow dot is 1.8s. Faster than that and the UI vibrates.
- **Reversible hovers.** Hover states change surface and lift text toward
  `--text-1`. Hovering never makes text dimmer or grey.
- **Reduced motion is honoured by value, not by removal.** Under
  `prefers-reduced-motion` all four duration tokens collapse to 1ms, the
  running pulse stops, and the spinner slows to 2s instead of freezing.

---

## 8. Voice & brand

**Register: a competent colleague, not a product.** The interface narrates
what the machine is doing in plain, specific language and never congratulates
the user.

- **Sentence case everywhere** — buttons, titles, labels, menu items. Caps only
  for the tracked micro-labels (column headers, section titles, eyebrows) where
  the casing is a typographic device, not a shout.
- **Terminal nouns stay literal.** "tunnel", "profile", "forward", "bind
  address", "ProxyJump", "identity file", "keep-alive". Do not rename domain
  terms into friendlier ones — the user knows what a ProxyJump is and a
  synonym costs them a mental lookup.
- **Active, present, lowercase-start after the first word.** `Create tunnel`,
  `Import from ~/.ssh/config`, `Start all visible`.
- **Transitions are described as processes, not as events.** "establishing
  channel." and "closing channels." — trailing period, lowercase, in mono,
  because it reads as an instrument read-out rather than a sentence.
- **Errors say what failed and what to do.** The detail column shows the real
  error for `failed` and the real reason for `unknown`, and keeps them
  distinguishable: `failed` is "the process said no", `unknown` is "we cannot
  reach the backend".
- **Counts are stated, not decorated.** `5 tunnels · 2 running`, with the
  numeral in mono and the live count in the ok color. No exclamation marks.
- **No exclamation marks, no emoji, no "Oops", no "Whoops".** The brand has no
  voice-over; it has a readout.

The wordmark is `tnls` in display type at 620 weight, tracked -0.015em, with
the teal dot. Always lowercase. Never set it in mono, never add a tagline
inline with it.

---

## 9. Anti-patterns

Do not do these when building with this system.

**Color**
- A second accent color. Teal is the only saturated hue; adding a purple, blue,
  or second green destroys its signal value.
- Deriving light mode by inverting dark mode.
- Pure black (`#000`) or pure white (`#fff`) page grounds.
- Warm beige, cream, or peach grounds. The system is cold by design.
- Gradients as background fills. There are none in the source and none belong.
- Semantic states signalled by color alone. Every state carries icon + label:
  the seven-state table is not decoration, it is the accessibility contract.
- Badges and log levels using page tokens. The log surface is always dark.
- A fourth text level. Three is the hierarchy.

**Type**
- Inter, Roboto, or Arial as display type. Söhne Breit leads; those faces are
  fallbacks only, and only for body text.
- Mono for names, labels, or prose. Mono is for data.
- Centered tabular numerals with proportional ones in the same column.
- Uppercase without `--tracking-caps`.
- Weight 700. Announce stops at 620.
- Any type size not on the scale.

**Layout**
- Cards inside cards. Hairlines separate; boxes do not.
- A colored left border on a card as a callout. The row rail is a state
  channel, not a decoration to be borrowed.
- Horizontal scrolling tables. Drop columns instead.
- Absolute-positioned sibling content regions. Absolute is for the bulk bar,
  scrim, drawers, menus, modals, and anchored overlays only.
- A radius that does not scale with its container.
- Centering the whole app. The shell is full-bleed and edge-to-edge.
- Page-level scroll. The shell is `100vh` grid; the table scrolls.

**Components**
- More than one primary button per viewport.
- An icon next to every heading.
- Multiple solid buttons for the same action in one view.
- A copy button always visible on every address. It appears on intent.
- Six or more action buttons in one table cell. Quick actions plus an overflow
  menu.
- Inline delete confirmation that reflows the row. Use the modal.
- Emoji as functional icons.

**Motion**
- Animated hovers that lighten text toward grey.
- Entrances and exits at the same duration.
- Loops faster than 1.5s or deeper than ~40% opacity swing.
- Motion that ignores `prefers-reduced-motion`.
- Skeleton loaders on operations that finish in under 300ms — the cheaper
  signal is a spinner in the badge or action button.

**Copy**
- Exclamation marks, "Oops", "Whoops", "Uh oh".
- Renaming SSH domain terms into friendlier synonyms.
- Cheerleading success toasts ("Great job!").
- Invented metrics or placeholder stats. Use honest, obviously-scoped values.

---

## 10. Known gaps and provenance

- **Fonts.** No font files ship in the source project or this package. Söhne
  and Söhne Breit are commercial (Klim Type Foundry) and JetBrains Mono is
  OFL — add licensed families to `fonts/` if you redistribute them. Until then
  the stacks fall back to `-apple-system` / `Segoe UI Variable` / system mono.
  The `fonts/` directory is intentionally absent rather than shipping
  substitutes that would misrepresent the metrics.
- **Raster assets.** None exist in the source: there is no logo file, no app
  icon, no tray image. The brand mark is the CSS dot on the wordmark and every
  glyph is the inline SVG sprite preserved in `build/icons.svg`. If app icons
  are later produced, they belong in `build/`.
- **The brief vs the shipped code.** `DESIGN_BRIEF.md` in the source project
  describes the *pre-redesign* state it was written to fix — Inter type, blue
  accent, no icons, no log coloring. The shipped `tokens.css` / `app.css` /
  `index.html` are the redesign and supersede the brief. Where they disagree,
  this document follows the code. The brief's UX criticism is preserved in
  `context/provenance.md` because it explains why the components are shaped as
  they are.
- **Source files are preserved, not regenerated.** `index.html`, `app.js`,
  `styles/app.css`, `styles/tokens.css`, `design-system.html`, and
  `DESIGN_BRIEF.md` remain in the project root exactly as copied.
# Steering history: step-by-step runs, directives and the AI guide

Stages 3 (cultures) and 4 (nations) can be run in one go, or step by step while you watch the map and steer. Steering is done with **directives**, issued by hand or by an LLM (the **AI guide**). Directives are saved with the world, so a steered history replays exactly without the model.

## Live runs

A live run is a `CultureSim` or `NationSim` held in memory by the session (`core/src/api.rs`, `Live`).

| API command | Arguments | What it does |
| --- | --- | --- |
| `sim_actions` | `stage` | The directive catalogue for a stage (name, description, JSON schema) |
| `sim_start` | `stage` (`cultures` \| `nations`) | Brings earlier steps up to date, starts a live run (replaying the directives already saved) |
| `sim_step` | `steps?`, `years?`, `months?`, `to?` (`era` \| `end`), `stop_at_choice?` | Advances one step (a generation, or the current step length), N steps, a number of years and months (the Stage 4 clock counts whole months), to the next era (the label changes) or to the end. With `stop_at_choice`, Stage 4 stops before an institution is born until its birthplace is chosen |
| `sim_state` | — | The world summary (below) and the map fields of the current position |
| `sim_directive` | `action`, `args`, `note?`, `by?` | Validates and queues a directive at the current position, and saves it in the `directives` override layer (undoable) |
| `sim_commit` | — | Runs to the end and keeps the result as the step's output. If the inputs are unchanged since the start, the live result is stored; otherwise the stage is replayed from scratch with the same directives (`replayed: true`) |
| `sim_cancel` | — | Ends the live run, leaving the stage as it was |

In the UI this is the panel that opens with the Cultures or Nations card: *Start step by step*, *Step*, *+N years*, *Next era*, *Play*, *To the end*, *Finish and keep*, *Restart*, *Stop*, with *World*, *Steer* and *AI guide* tabs. Undo ends a live run.

**The replay guarantee.** A directive records when it was issued (a generation for cultures; a year and month for nations, whose clock counts whole months). A run applies it at the start of the step whose time span contains that moment, ordered by time and then by issue order. Because the step-by-step run and the full run are the same code, a kept live run equals a fresh run with the same directives — this is tested (`live_steps_and_directives_match_a_fresh_run`).

**Choosing where institutions are born.** When the next institution's time has come, the Stage 4 summary carries `pending_institution` with its candidates. With *Ask me where institutions are born* checked (the default), the panel stops there, shows the candidates (gold stars on the map) with *Here* buttons and *Let chance decide*, and issues an `institution_birth` directive; stepping on, the institution is born there. Unchecked (and in full runs, the CLI and the guide's turns), chance picks among the candidates unless a choice was made earlier — `next_institution` in the summary lists the coming institution's candidates, so the guide can choose ahead.

**The world summary** (`sim_state`, also the guide's input) for Stage 4 contains: time and step length, start and end, world era and leading nation, nations per era, institutions, the next and any pending institution with candidates, empires, tags, population, ruled provinces, living nations (id, name, government, capital, culture, provinces, people, era, technology, treasury, income, integration, roads, railways, airports, regions), regions (population, main province, top owners), active effects, transport totals, city pins, the 20 largest cities, ruins, the last 20 events, applied directives and the queue. Stage 3's summary lists cultures, groups, settled provinces and events.

## Directives

Every directive has strict arguments (all listed, optional ones nullable, no extra keys; string choices are enumerated, and the Steer tab shows them as drop-down lists). `worldgen directive --list` prints the full schemas. Places are any mix of province, state and region ids. Effects with `years` last that long from the moment they apply.

### Stage 3 (cultures)

| Action | Arguments | Effect |
| --- | --- | --- |
| `growth` | places, `culture`?, `factor` 0.1–5, `years` | Capacity and growth ×factor (a fertile or a hard age) for places or one culture |
| `attraction` | places, `value` −1…1, `years` | Draws people to the places or drives them away |
| `isolate` | places, `years` | No contact or migration across the places' edge: the people inside drift into their own culture |
| `drift` | `culture`, `factor` 1–20, `years` | A culture changes fast (a schism): it tends to split |
| `contact` | `culture_a`, `culture_b`, `factor` 1–50, `years` | Two cultures meet as if alike: they tend to merge |
| `catastrophe` | places, `severity` 0–0.95 | A share of the people dies at once |
| `settle` | `province`, `bands` 1–50, `culture`? | A migration wave of an existing culture or a new people |

### Stage 4 (nations)

| Action | Arguments | Effect |
| --- | --- | --- |
| `aggression` | `nation`, `factor` 0–10, `years` | More or fewer expansion attempts |
| `expand_toward` | `nation`, `province`, `years` | Expansion prefers targets near the province |
| `war` | `nation`, `target`, `years` | Attacks the target first and harder (cost ×0.6, strength ×1.5) |
| `peace` | `nation`, `target`, `years` | Neither takes the other's land |
| `stability` | `nation`, `factor` 0.05–20, `years` | Breakup chance ÷factor |
| `split` | `nation`, `culture`? | The provinces of one culture (or the outlying part) secede now |
| `found` | `province`, `name`? | A new nation (a breakaway if the province is owned) |
| `union` | `nation`, `target` | The target's land, treasury and (if higher) technology join the nation |
| `transfer` | places, `nation` | A treaty hands the places over |
| `rename` | `nation`, `name` | New name |
| `railway` | `nation`, `from`, `to` | A railway line over its own land, paid from the treasury (debt allowed); needs the nation's industrial era |
| `port` | `nation`, `province` | A port in one of its coastal provinces (shipping era), paid from the treasury |
| `institution_birth` | `institution` 0–5, `province`? | Where an institution is born when it emerges (null: chance among the candidates) |
| `reform` | `nation`, `years` | Westernizing reforms: institutions spread three times as fast; costs a year's income and stability |
| `feudal_empire` | `nation`, `on`, `name`?, places | Makes the nation a feudal empire of kings and dukes over the places (or all its land), or dissolves it |
| `tag` | `scope` (world, state, nation), `id`?, `tag`, `on` | Sets or clears a tag (see [nations.md](nations.md#tags-and-feudal-empires)) |
| `build_road` | `nation`, `from`, `to`, `quality` 1–3 | Builds or upgrades a road (3 = highway, motor age only), debt allowed |
| `airport` | `nation`, `province` | An airport (air age only), paid from the treasury |
| `subsidy` | `nation`, `years` 0.1–100 | Adds that many years of income to the treasury |
| `tech` | `nation`, `years` −500…500 | Technology leaps ahead or falls back; its provinces embrace the institutions it passes (born in its capital if not yet born) |
| `pin_add` | `province`, `value` −1…1, `years`?, `label`? | A city pin: a boom town / metropolis (+) or an abandoned city (−) |
| `pin_move` | `pin`, `province` | Moves a pin |
| `pin_remove` | `pin` | Removes a pin |
| `move_capital` | `nation`, `province` | The new capital grows into its pull over the following decades |
| `catastrophe` | places, `severity` 0–0.95 | A share of the people dies at once |
| `note` | `text` | A line in the chronicle (both stages) |

A directive that cannot apply (a dead nation, a province not owned, a missing pin, the wrong era) is recorded with `applied: false` and changes nothing.

## The AI guide (`guide/`)

You describe the history you want ("a sea empire in the south that breaks into three kingdoms by 1800"); the guide plays turns:

1. The live run advances a stretch of time (*Years per turn*, default 50, or what the model asked for with `set_pace`).
2. The model gets one request with no conversation history: the system prompt (fixed), the user's goal, the stage and time, the limit on tool calls (*Max directives*, default 4, at most 12) and the world summary (trimmed: 30 nations or 24 cultures, 80 regions, the last 30 directives; colours and redundant columns removed).
3. Its tool calls (up to the limit) are validated and become directives (`by: "guide"`); invalid ones are listed with their error in the turn's result and change nothing. Its text becomes the chronicle line (a `note` directive). The request allows up to 16,000 output tokens; token counts are reported per turn.

The tools offered are exactly the directive catalogue of the stage plus `set_pace` (5–1,000 years), so the guide can do nothing a user cannot do by hand. The simulation, not the model, decides what follows.

### Providers

| Format | Endpoint | Used for |
| --- | --- | --- |
| Anthropic | `POST {base}/v1/messages` with `x-api-key` and `anthropic-version: 2023-06-01` | The Claude API (default, model `claude-opus-5-5`) and Anthropic-compatible gateways |
| OpenAI-compatible | `POST {base}/chat/completions` with a bearer key and function tools | OpenAI, the Vercel AI Gateway, OpenRouter, local servers (Ollama, LM Studio), other gateways |

With the Anthropic format the system prompt is sent as a cached block (`cache_control: ephemeral`), `output_config.effort` carries the effort setting for Claude models, and on `api.anthropic.com` with Opus, Fable or Sonnet 5.5 the request asks for server-side refusal fallback (`fallbacks: "default"`, beta header `server-side-fallback-2026-07-01`). HTTP goes through `ureq` with the system's certificates and the `HTTPS_PROXY` environment, bypassing the proxy for loopback addresses (local servers).

### Keys and settings

Settings (provider, base URL, model, effort, key) are stored per user — `%APPDATA%\FantasyWorldMaker\guide.json` on Windows, `~/.config/fantasy-world-maker/guide.json` elsewhere — never in a world, so sharing a world never shares a key. `ANTHROPIC_API_KEY`, `AI_GATEWAY_API_KEY`, `OPENAI_API_KEY` and `OPENROUTER_API_KEY` are read when no key is saved. API commands: `guide_presets`, `guide_config_get` (the key is masked), `guide_config_set`, `guide_test` (one short request), `guide_turn` (`goal`, `years`, `max_actions`).

From the command line: `worldgen guide-setup …` and `worldgen guide <project> --goal TEXT [--stage nations|cultures] [--years 50] [--max-turns 60] [--max-actions 4]`, which plays turns to the end, prints each chronicle line and keeps the result.

### Testing

`guide/tests/mock.rs` runs a mock provider on localhost in both formats and checks the request (key, headers, cached system prompt, effort, tools, no fallback outside the Claude API), that tool calls become directives (an out-of-range one refused), that the chronicle line is stored and that the steered run commits unchanged. Real providers were not exercised in automated tests (no credentials in CI).

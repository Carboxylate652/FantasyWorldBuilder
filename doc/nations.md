# Stage 4 — Nations and history

Source: `core/src/stages/nations.rs` (the simulation) and its modules `nations/transport.rs` (lines, travel costs, transport constants), `nations/ports.rs` (harbours and ports), `nations/institutions.rs` (institutions and reforms) and `nations/feudal.rs` (tags and feudal empires). Parameters are in `NationParams` ([parameters.md](parameters.md#nations-and-history-step-12)).

Stage 4 reads the finished Stage 2 province graph, the Stage 3 population and culture shares per province, and a few map fields (sea depth, winds, rivers and winter cold for harbours). It runs on a clock in years from *First polities* (`start_year`, default 800) to the *Start date* (`start_date`, default 1949). The simulation is a `NationSim` that advances one step at a time, so the full run and the step-by-step run are the same code.

## Time steps

Time is counted in whole months (an integer; month 0 of year *y* is *y* × 12), and the year is that count divided by 12. Steps get shorter toward the present, where history is denser: `months_per_step` (24 months) at first, `months_per_step_1` (12) from `step_year_1` (1400), and `months_per_step_2` (6) from `step_year_2` (1800). A step never crosses one of those years or the start date. Directives record the year and month they were issued, and apply in the step whose months contain it. Events are dated by their year. Rates are per year and scale with the step length, and road and railway projects, war summaries and progress reports run on their own timers, so results don't depend on how long the steps are (beyond the randomness of when things happen). The default run has about 1,000 steps.

## State kept per province and per nation

| Per province | Per nation |
| --- | --- |
| owner (0 = none), population, capacity (`cap`), culture shares and majority culture | name, colour, capital (and since when), ruling culture, founding year, end and fate, parent |
| devastation (0 to −1), attraction (−1 to +1), peak population, ruined / metropolis flags | aggression (drawn at founding, 0.6–1.4) |
| railway (0 none, 1 track, 2 station, 3 junction), extra capacity from rail | technology (in years) and era |
| integration with its owner (0–1) | treasury, income, upkeep, infrastructure upkeep, bankrupt-until time |
| harbour quality (0–1, fixed), port | feudal liege and rank (emperor, king, duke) |
| presence (0–1) of each of the six institutions | reforming until, and on whose model; next road and railway project times |

World-wide: roads (per border: quality and year), railway lines, airports, ports, city pins, institutions (birth year and place, and choices made by directives), tags, feudal empires, active effects from directives, the event log.

Capacity starts at max(6 × the Stage 3 population, what the land holds). Empty land settled by a nation gets 2% of its capacity (at least 200 people) of the ruler's culture.

## Order of one step

1. **Directives** dated inside this step's span are applied (see [steering.md](steering.md)).
2. Expired city pins are dropped.
3. **Institutions:** the next one is born if its time has come; emperors whose nation fell are succeeded; every born institution spreads.
4. **Technology** advances; eras follow the institutions (events for the first three nations into each era).
5. **Access** is recomputed for nations whose land, roads, railways, ports, airports, capital or era changed; then each province's **integration**.
6. **Economy:** income (with port trade), upkeep, treasury; bankruptcy; vassals' tribute.
7. **Ports** open in good harbours; nations far behind a neighbour may begin **reforms**.
8. **Attraction** of every province.
9. **Growth** toward capacity.
10. **Migration** within each nation.
11. Devastation heals; peaks, metropolises and ruins are logged.
12. **New polities** in unruled, populous provinces.
13. **Expansion** attempts (settlement, colonies, war).
14. **Breakups.**
15. **Assimilation** of culture shares.
16. **Building:** road projects (and airports in the air age) and railway projects whose time has come.
17. Time advances; every 50 years the conquests since the last summary are logged as **war summaries**.

## Formation

A province with no ruler and at least *People to found a polity* (60,000) founds a nation with chance `found_rate × dt × min(4, pop / 60,000)` per step, cut to 30% once the calendar passes the shipping year (the world is mostly claimed). The nation is named after the province and ruled by its majority culture. A breakaway (split or a `found` directive in owned land) is logged as an independence.

A new nation's technology and era are its parent's (a breakaway); a new polity starts at the median technology of living nations (or the calendar year if it is the first), and its era follows the institutions its land has embraced.

## Expansion and war

Each step a nation makes `expansion_rate × aggression × aggression directives × dt / 2` attempts (the fraction is a chance). An attempt scores every unowned or foreign neighbour of (up to 48 sampled) member provinces:

- value = √(people + 0.1 × capacity), ×3 in a *holy land* (state tag);
- cost = (1 + border cost / 300) × (1 + reach cost / *Reach*) × (1 + *Culture border weight* × cultural distance), where cultural distance is 0 (same culture), 0.5 (same group), 1 (other) or 0.3 (empty land); ×2 for land held by another nation (×0.6 at war with it); ×0.8 with gunpowder;
- score = value / cost × random(0.5–1.5), ×3/(1 + km/800) toward an *expand toward* target.

Border cost is the same travel cost as access (below), so roads make expansion cheaper. Reach cost is the province's access from the capital × 0.75 (straight distance where the capital has no route yet). Never targeted: land of a nation at peace with the attacker (directive), of a fellow member of its feudal empire, provinces of a *free state* (state tag), and barren ice. Aggression is ×1.5 for *expansionist* nations and ×1.5 for everyone with the *frequent wars* world tag.

**Colonies** sail from ports: a nation with ocean shipping and at least one port samples 12 coastal provinces anywhere within *Colony range* × (0.6 + 0.8 × its best harbour) of its ports (30% of attempts, 60% for a *merchant republic*): free land, or land of a nation (or feudal empire) under a quarter of its people. *Isolationist* nations and the *no overseas colonies* world tag found none.

**Resolution:** empty land is settled with chance 0.55 (0.8 with gunpowder). Held land is fought over: strength = people^0.8 × 1/(1 + reach cost/*Reach*) × productivity^0.4, where the defender counts the people of its whole feudal empire, ×1.3 in its own culture's province and ×1.5 if *eternal*; the attacker ×1.5 at war (directive). The attacker wins with probability att/(att + def). A conquered province loses *Sack* (3%, four times for a capital) of its people and gains *War devastation* (−0.25). Taking a capital moves the loser's capital to its most populous remaining province.

## Breakups

instability = 1.2 × foreign share of people + 0.4 × min(3, provinces/60) + 0.4 × min(3, mean reach cost / (2 × *Reach*)). Chance per step = `collapse_rate × dt/2 × instability² × era factor × debt factor × reform factor × calm / stability directives`, where the era factor is 1 (early), 0.7 (gunpowder) or 0.5 (industry), the debt factor 1.6 while bankrupt, the reform factor 1.5 while reforming, and calm 0.5 with the *stable realms* world tag. Members of a feudal empire and *eternal* nations never break apart. The provinces of the largest foreign culture (≥ 15% of people) secede, or else the part closer to the far edge than to the capital.

## Technology (per nation)

Technology is measured in years: a nation's general development. Each step:

- **Score** = 0.65 × rank in income per head + 0.35 × rank in population (ranks among living nations, 0 last … 1 first).
- **Target** = year + *Technology lead* − 220 × (1 − score)^1.3, raised to at least (best neighbour's technology − 80), never above year + *Technology lead*.
- Below its target a nation gains `dt × (1 + tech_spread × gap)` (catching up faster the further behind it is), above it 0.2 a year. Nothing advances past year + *Technology lead*, nor past the next era's year before the nation has entered it.

Technology sets productivity (below), ranks the candidates for an institution's birth, and decides how fast a nation takes up an institution (see absorption).

## Institutions and eras

Each era after the first opens with an **institution** (`nations/institutions.rs`), in the manner of Europa Universalis IV:

| Institution | Opens | Earliest birth | Era effects |
| --- | --- | --- | --- |
| gunpowder | gunpowder | 1450 | expansion ×0.8 cost, settlement 0.8, breakups ×0.7 |
| navigation | ocean shipping | 1500 | ports, colonies, sea lanes in access |
| industrialisation | industry | 1830 | capacity ×2, +0.9%/yr growth, railways, breakups ×0.5, productivity ×1.8 |
| synthetic fertilizer | fertilizer | 1909 | capacity × up to *Fertilizer boost* (1.6), phased in over 20 technology years; productivity ×1.15 |
| motorisation | motor age | 1920 | highways; productivity ×1.3 |
| aviation | air age | 1935 | airports and flights; productivity ×1.1 |

- **Birth.** Once the calendar reaches an institution's earliest year (the era-year parameters) and the institution before it has been born, it is born in one province, one per step. **Candidates** are provinces of nations already in the previous era, scored by (0.3 + the owner's technology rank) × ln(1 + people/10,000) × (0.5 + integration) × a fit for the idea — a good harbour for navigation (a port, or a harbour near the port threshold, required), coal (×1.6) and iron (×1.4) for industrialisation, fertile land for fertilizer, oil for motorisation, a city of 300,000 for aviation — ×3 in an *institution cradle* (state tag). The best *Birthplace candidates* (8) are offered, at most three per nation. The birthplace is the one chosen by an `institution_birth` directive (by the user in the live panel, or by the AI guide), or else drawn among the candidates with chance in proportion to their scores. A live run can stop before a birth to let the user choose (see [steering.md](steering.md)).
- **Spread.** Each province holds a presence (0–1) of each born institution. Every step it grows by `dt × institution_spread × absorption × multiplier × contact × (1 − presence)`, where **contact** is the strongest of:
  - a neighbouring province's presence × road contact (no road 0.5, track 0.65, paved 0.85, highway 1.0; impassable 0.25; strait 0.3);
  - coastal sailing to the nearest shores of other landmasses within 900 km: 0.2 × their presence;
  - the same railway line: 0.9 × the highest presence among its stations;
  - sea lanes between ports (each port and its 8 nearest ports within *Colony range*): 0.75 × their presence × its harbour quality (at least 0.3);
  - airports: the highest presence at any airport;
  - the nation's capital (or 0.6 × its liege's): 0.3 × its presence × the province's integration.
- **Absorption:** a nation takes up an idea only as far as its development reaches: clamp(1 − (institution year − technology)/60, 0.03, 1); unowned land 0.3. Rich nations, whose technology runs ahead, take up new ideas first; poor ones barely at all. The **multiplier** is 0.3 for *isolationist* nations and 3 while reforming (reforming provinces also get at least 0.5 contact); the world tags *slow institutions* and *fast institutions* halve or double the rate. Institutions every settled province has embraced stop being computed.
- **Eras.** A province has **embraced** an institution at presence 0.9. A nation enters an era once at least *Share of provinces for an era* (0.5) of its provinces have embraced its institution and it is in the previous era. Eras never go back. The world's era (shown in the step-by-step panel, used by *Next era*) is the most advanced nation's.
- **Reforms (westernization).** A nation (not isolationist, not reforming) two or more eras behind a neighbouring nation, with a year's income in its treasury, begins reforms with chance dt/60 per step: it pays a year's income and, for 40 years, institutions spread through it three times as fast while it is 1.5× likelier to break apart. A `reform` directive starts one at once.

The `tech` directive raises a nation's technology; its provinces embrace every born institution whose year the new technology passes, and an institution not yet born whose year it passes is born in its capital.

## Economy

- **Productivity** = 1 + max(0, (technology − 1000)/250) × the era multipliers above.
- **Income** = Σ people × integration × productivity × *Tax share* (0.08), ×(1 + 0.3 × harbour) in a port (trade; 0.6 for a *merchant republic*).
- **Upkeep** = 30% of income (army) + infrastructure + spending of a full treasury (half of anything above two years of income, each year).
- **Infrastructure upkeep** per year: roads 0.4 / 5 / 20 per km (track / paved / highway, split between the two provinces' owners), railways 25 per km and 250 per station (split over the provinces on the line), ports 400, airports 8,000 — all × the payer's productivity (labour costs) and the cost parameters.
- **Building**: roads 25 / 180 / 1,500 per km of construction length (km × (1 + barrier), twice over impassable land), upgrades pay the difference; railways 1,000 per km of construction length and 12,000 per new station; ports (by directive) 20,000; airports 300,000 — all × the builder's productivity.
- **Tribute:** vassals in a feudal empire pay a tenth of their income to their liege.
- Automatic projects need the money in hand and keep infrastructure upkeep under 40% of income. Directives may go into debt.
- **Bankruptcy:** a treasury below −max(2 × income, 50,000) closes the nation's newest railway lines (up to three) and lets its costliest roads decay a grade until its upkeep fits, writes off half the debt and makes it 1.6× likelier to break apart for ten years.
- A `union` merges treasuries (and keeps the higher technology).

## Access and integration

Each nation's access is the cheapest travel cost from its capital, in "km on foot", to its provinces and their neighbours (`transport::travel_costs`, a Dijkstra over provinces and line stations):

| Link | Cost |
| --- | --- |
| Land border | km × (1 + barrier); ×3 over impassable land; ÷ road speed (track 1.6, paved 2.6, highway 6) |
| Strait | 2 × width + 150 |
| Train | 12% of the ride's km between stations the nation owns |
| Boarding / leaving a line | half of *Line change penalty* (150) each — so every change of line at a junction costs the full penalty |
| Sea lane (ocean shipping) | from each of the nation's ports to its main port (the capital if a port, else its largest port): km × 0.6 × (1.25 − 0.5 × the two harbours' mean quality) + 200; coastal provinces without a port have no sea lane |
| Flight (air age) | between the nation's airports: km × 0.05 + 300 |

**Integration** = 1 / (1 + access / 1,500); 0.2 where the capital can't reach. It scales income, assimilation (× 0.5 + integration) and where migrants go (room × (0.5 + integration)); access replaces straight distance in expansion, war strength and breakup spread.

## Roads

Every *Years between road projects* (10, staggered by nation), a nation picks its largest cities (at least half the founding population; up to *Cities linked by rail* + 4) and, for the first whose road to the capital is worse than wanted, builds the best quality it can pay for, up to three projects at a time:

- wanted: a **highway** between cities of 300,000 in the motor age; a **paved road** to a city of 150,000 or in an industrial nation; else a **track**;
- route: cheapest to build over the nation's own land, reusing roads already good enough;
- roads whose two provinces have no owner decay one grade with chance dt/40 a step.

## Railway lines

Every *Years between railway projects* (8), an industrial nation links its largest city not yet on a railway in its land (the capital starts the network) along the cheapest construction route over its own land, if it can pay:

- stations at the ends, at cities (≥ the founding population), every five provinces and in dry land (water stops);
- a stretch that ends at an end of one of the nation's own lines (without overlapping it) **extends** that line; otherwise it is a **new line**, and the province where it meets the network becomes a station on the line it meets — a **junction** when two or more open lines stop there;
- per province: track draws 0.3 × *People per railway station*, a station 1×, a junction 1.5×; stations add +0.15 attraction (junction +0.2);
- bankruptcy closes lines; closed lines stay in the table with their closing year.

## Ports

`nations/ports.rs`. Every coastal land province has a fixed **harbour quality** (0–1), from its cells that touch the sea:

- calm winds: 1 − (mean annual wind speed − 2 m/s)/5;
- draft: (mean depth of the sea cells beside it − 20 m)/150 m;
- shelter: 1 − (share of its coastal cells' neighbours that are sea − 0.1)/0.3 (a bay rather than a cape);
- quality = (0.35 × winds + 0.3 × draft + 0.35 × shelter) × ice (0.3 if the coldest month is below −8 °C, 0.7 below −2 °C) + 0.2 at a river mouth, clamped to 0–1.

A coastal province becomes a **port**, with chance dt/15 × (0.5 + harbour) per step, once its owner is in the ocean-shipping era, its harbour reaches *Harbour needed for a port* (0.62; 0.15 less for a *merchant republic*) and it and half its land neighbours hold *People for a port* (80,000). The `port` directive builds one in any coastal province of a nation in the shipping era. Ports carry sea lanes, colonies, trade income, +0.1 attraction and institutions across the sea, and cost upkeep. Ports stay when their province changes hands.

## Airports

In the air age, when the treasury holds twice the cost and upkeep fits, a nation opens an airport at its capital and then at cities of 300,000, up to one per 40 provinces (at most eight). Airports add +0.1 attraction and flight links in access.

## Growth, migration and cities

- **Capacity** this step = (capacity + rail extra) × 2 (industrial owner) × fertilizer multiplier × pull(attraction − devastation), where pull(a) = 1 + 4a for a ≥ 0 and (1 + a)² below.
- **Growth:** logistic at *Population growth* (+ *Industrial growth* for industrial owners) × (1 + 0.5 × devastation); people leave land that holds no one (×0.9 a year).
- **Attraction** = devastation + station/junction/port/airport bonus + capital pull × grown × scale + city pins, clamped to ±1. A capital grows into *Capital pull* over *Years to grow a capital*, more for larger nations (scale 0.4–1 by log size).
- **Migration:** each year *Migration to cities* of the people in unattractive provinces (more from devastated or shunned ones, up to 5×) moves to the nation's attractive provinces, weighted by room × (0.5 + integration) and never beyond their room; those who find no room stay.
- **Devastation** heals by *Recovery per year*. A province passing 1,000,000 people is logged as a *metropolis*; one that falls below a quarter of its peak (a peak of at least three times the founding population) as *ruined*, cleared when it is back above 60%.

## Assimilation

Each year `assimilation × dt × (0.5 + integration)` of a ruled province's other cultures takes the ruler's culture; shares under 0.5% are dropped and the majority recomputed.

## Tags and feudal empires

`nations/feudal.rs`. The `tag` directive sets tags (on or off):

| Scope | Tag | Effect |
| --- | --- | --- |
| nation | isolationist | no colonies, institutions arrive at 0.3×, never reforms |
| nation | expansionist | aggression ×1.5 |
| nation | merchant_republic | ports at 0.15 lower harbour quality, port trade ×2, colonies twice as often |
| nation | eternal | never breaks apart; defends ×1.5 |
| state | holy_land | everyone covets it (expansion value ×3) |
| state | free_state | its provinces never change hands by war or settlement |
| state | institution_cradle | institutions prefer to be born here (candidate score ×3) |
| world | no_overseas_colonies | no colonies anywhere |
| world | slow_institutions / fast_institutions | institutions spread at half / double the rate |
| world | frequent_wars | aggression ×1.5 for every nation |
| world | stable_realms | breakups half as likely |

The `feudal_empire` directive (on: true) makes a nation a **feudal empire** over the given places (provinces, states, regions; its own land among them), or over all its land:

- The emperor keeps his capital region as his own domain; each other region of the imperial territory with at least two provinces becomes a **kingdom**, a vassal nation named after the region ("Kingdom of …").
- Each king keeps his capital state and enfeoffs a **duke** over every other state ("Duchy of …"); so does the emperor in his own region.
- Provinces are the **counties**; each county has two to four **baronies** (titles only, listed in `titles.csv`).
- Kings and dukes are nations with a liege: they keep the emperor's technology and era at creation, expand on their own, pay a tenth of their income to their liege, and get institutions from their liege's capital.
- Members never fight each other, never break apart, and defend each other (a member's defence counts the whole empire's people). The empire lasts in every era until a `feudal_empire` directive with on: false dissolves it (all members become independent). If the emperor's own nation falls, the largest member is **elected** emperor and the fallen emperor's vassals become his. Applying the directive again adds the places to the territory.
- Land of the emperor outside the given places is held outside the empire (like Habsburg lands outside the Holy Roman Empire); the `imperial` field marks the de jure territory.

## Events

`founded`, `independence`, `ended` (conquered or collapsed), `conquest` (a capital taken), `capital` (capital moved), `colony`, `union`, `renamed`, `war` (per half century: who took three or more provinces from whom, top 12), `institution` (a birth), `era` (first three nations into each era, special lines for the first industrial, fertilizer, motor and air nations), `reform`, `port` (the world's first and each nation's first, up to 12), `railway` (the first line and ~15% of the rest), `highway` (the first), `airport` (the first five), `bankruptcy`, `empire`, `emperor` (an election), `empire_dissolved`, `metropolis`, `ruined`.

## Outputs

- **Fields:** `owner`, `nation_culture`, `nation_population` (people per cell), `nation_attraction`, `railway` (0–3), `road` (best road touching the province, 0–3), `airport` (0/1), `port` (0 none, 1 good harbour, 2 port), `nation_era` (owner's era + 1, 0 = no owner), `institutions` (bit k: institution k embraced), `institution` (presence of the newest born institution), `realm` (top of the owner's feudal chain), `imperial` (1 in an empire's de jure territory).
- **Meta:** counts (nations, ruled share, population, conquests, independences, colonies, bankruptcies, reforms, metropolises, ruins), the world era and leader, nations per era, institutions, empires, tags, transport totals, applied directives, and the tables `nations`, `provinces`, `events`, `railways`, `stations`, `roads`, `airports`, `ports`, `titles`, `institutions`, `empires`, `cities` (top 100), `pins`, `ruins`.
- **Live summary** (`sim_state`) adds the current time and step length, `pending_institution` (an institution waiting for its birthplace, with candidates) and `next_institution` (the next one to come, its earliest year, any chosen birthplace and its current candidates).
- **Export:** see [export-import.md](export-import.md#stage-4-files).

## Typical result (default world: seed 3, level 6, default parameters)

About 5 s for 800–1949 (about 1,000 steps). 153 nations in 1949 (223 ever), 91% of land ruled, ~1,200 capitals taken, ~90 independences, 21 colonies, population ~110 M → ~420 M, 58 cities over a million, 69 in ruins, 10 reforms. Institutions are born at their earliest years (gunpowder 1450 … aviation 1935); the first nation enters the gunpowder era in 1457, ocean shipping 1504 (first port the same year), industry 1853, fertilizer 1915, the motor age 1928 and the air age 1941. In 1949: 6 nations in the air age, 24 motor, 31 fertilizer, 62 industry, 25 ocean shipping, 1 gunpowder, 4 early. 239 ports; about 450,000 km of track, 340,000 km of paved road, 12,000 km of highway; 105 railway lines (92,000 km, 413 stations, 62 junctions); 2 airports.

## Tuning notes

- The constants in the economy (costs, upkeep, 30% army, 40% infrastructure cap, two-year reserve) were tuned so that a paved road costs about a year of a mid-sized pre-industrial kingdom's income and treasuries hover around three years of income. They are judgement calls, not calibrated to data; *Tax share*, *Road cost* and *Railway and airport cost* scale them.
- The technology model went through two failed designs (a pure growth-rate model either converged everyone to the leader or let the whole world lag a century and a half). The target model above, with ranks, guarantees a spread of eras. *Technology lead* and *Technology catch-up* are the knobs; the 220-year maximum lag and the 80-year neighbour limit are constants in `advance_tech`.
- Institutions took several rounds: early ones must cross oceans before ports exist (hence coastal sailing), late ones must not sweep the world in a decade (hence absorption by development, with no technology jump on entering an era). `institution_spread`, the 60-year absorption window and the contact weights are the knobs; `SPEED` in `institutions.rs` can make later institutions faster.
- Harbour quality is relative to this generator's 28 km cells (depths are means over a cell); *Harbour needed for a port* is the knob for how many ports open (239 on the default world).
- When changing the model, bump the last entry of `MODEL_VERSION` in `core/src/stages/mod.rs`.

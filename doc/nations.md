# Stage 4 — Nations and history

Source: `core/src/stages/nations.rs` (the simulation) and `core/src/stages/nations/transport.rs` (lines, travel costs, transport constants). Parameters are in `NationParams` ([parameters.md](parameters.md#nations-and-history-step-12)).

Stage 4 reads the finished Stage 2 province graph and the Stage 3 population and culture shares per province. It runs on a clock in years from *First polities* (`start_year`, default 800) to the *Start date* (`start_date`, default 1949), `years_per_step` (2) years per step. The simulation is a `NationSim` that advances one step at a time, so the full run and the step-by-step run are the same code.

## State kept per province and per nation

| Per province | Per nation |
| --- | --- |
| owner (0 = none), population, capacity (`cap`), culture shares and majority culture | name, colour, capital (and since when), ruling culture, founding year, end and fate, parent |
| devastation (0 to −1), attraction (−1 to +1), peak population, ruined / metropolis flags | aggression (drawn at founding, 0.6–1.4) |
| railway (0 none, 1 track, 2 station, 3 junction), extra capacity from rail | technology (in years) and era |
| integration with its owner (0–1) | treasury, income, upkeep, infrastructure upkeep, bankrupt-until year |

World-wide: roads (per border: quality and year), railway lines, airports, city pins, active effects from directives, the event log.

Capacity starts at max(6 × the Stage 3 population, what the land holds). Empty land settled by a nation gets 2% of its capacity (at least 200 people) of the ruler's culture.

## Order of one step

1. **Directives** dated inside this step's years are applied (see [steering.md](steering.md)).
2. Expired city pins are dropped.
3. **Technology** advances; eras change (events for the first three nations into each era).
4. **Access** is recomputed for nations whose land, roads, railways, airports, capital or era changed; then each province's **integration**.
5. **Economy:** income, upkeep, treasury; bankruptcy.
6. **Attraction** of every province.
7. **Growth** toward capacity.
8. **Migration** within each nation.
9. Devastation heals; peaks, metropolises and ruins are logged.
10. **New polities** in unruled, populous provinces.
11. **Expansion** attempts (settlement, colonies, war).
12. **Breakups.**
13. **Assimilation** of culture shares.
14. **Building:** road projects (and airports in the air age), railway projects.
15. The year advances; every 50 years the conquests since the last summary are logged as **war summaries**.

## Formation

A province with no ruler and at least *People to found a polity* (60,000) founds a nation with chance `found_rate × dt × min(4, pop / 60,000)` per step, cut to 30% once the calendar passes the shipping year (the world is mostly claimed). The nation is named after the province and ruled by its majority culture. A breakaway (split or a `found` directive in owned land) is logged as an independence.

A new nation's technology is its parent's (a breakaway) or the median of living nations (a new polity), or the calendar year if it is the first.

## Expansion and war

Each step a nation makes `expansion_rate × aggression × aggression directives × dt / 2` attempts (the fraction is a chance). An attempt scores every unowned or foreign neighbour of (up to 48 sampled) member provinces:

- value = √(people + 0.1 × capacity);
- cost = (1 + border cost / 300) × (1 + reach cost / *Reach*) × (1 + *Culture border weight* × cultural distance), where cultural distance is 0 (same culture), 0.5 (same group), 1 (other) or 0.3 (empty land); ×2 for land held by another nation (×0.6 at war with it); ×0.8 with gunpowder;
- score = value / cost × random(0.5–1.5), ×3/(1 + km/800) toward an *expand toward* target.

Border cost is the same travel cost as access (below), so roads make expansion cheaper. Reach cost is the province's access from the capital × 0.75 (straight distance where the capital has no route yet). Nations at peace (directive) never take each other's land; barren ice is never claimed.

**Colonies:** a nation with ocean shipping and a coast also samples 12 coastal provinces anywhere within *Colony range* of its ports (30% of attempts): free land, or land of a nation under a quarter of its people.

**Resolution:** empty land is settled with chance 0.55 (0.8 with gunpowder). Held land is fought over: strength = people^0.8 × 1/(1 + reach cost/*Reach*) × productivity^0.4, the defender ×1.3 in its own culture's province, the attacker ×1.5 at war (directive). The attacker wins with probability att/(att + def). A conquered province loses *Sack* (3%, four times for a capital) of its people and gains *War devastation* (−0.25). Taking a capital moves the loser's capital to its most populous remaining province.

## Breakups

instability = 1.2 × foreign share of people + 0.4 × min(3, provinces/60) + 0.4 × min(3, mean reach cost / (2 × *Reach*)). Chance per step = `collapse_rate × dt/2 × instability² × era factor × debt factor / stability directives`, where the era factor is 1 (early), 0.7 (gunpowder) or 0.5 (industry) and the debt factor 1.6 while bankrupt. The provinces of the largest foreign culture (≥ 15% of people) secede, or else the part closer to the far edge than to the capital.

## Technology and eras (per nation)

Technology is measured in years. Each step:

- **Score** = 0.65 × rank in income per head + 0.35 × rank in population (ranks among living nations, 0 last … 1 first).
- **Target** = year + *Technology lead* − 220 × (1 − score)^1.3, raised to at least (best neighbour's technology − 80), never above year + *Technology lead*.
- Below its target a nation gains `dt × (1 + tech_spread × gap)` (catching up faster the further behind it is), above it 0.2 a year. Nothing advances past year + *Technology lead*. `tech` directives move it directly.

A nation's **era** is the number of era years its technology has passed:

| Era | Default year | Effects |
| --- | --- | --- |
| early | — | — |
| gunpowder | 1450 | expansion ×0.8 cost, settlement 0.8, breakups ×0.7 |
| ocean shipping | 1500 | colonies; sea lanes in access |
| industry | 1830 | capacity ×2, +0.9%/yr growth, railways, breakups ×0.5, productivity ×1.8 |
| fertilizer | 1909 | capacity × up to *Fertilizer boost* (1.6), phased in over 20 technology years; productivity ×1.15 |
| motor age | 1920 | highways; productivity ×1.3 |
| air age | 1935 | airports and flights; productivity ×1.1 |

The world's era (shown in the step-by-step panel, used by *Next era*) is the most advanced nation's.

## Economy

- **Productivity** = 1 + max(0, (technology − 1000)/250) × the era multipliers above.
- **Income** = Σ people × integration × productivity × *Tax share* (0.08).
- **Upkeep** = 30% of income (army) + infrastructure + spending of a full treasury (half of anything above two years of income, each year).
- **Infrastructure upkeep** per year: roads 0.4 / 5 / 20 per km (track / paved / highway, split between the two provinces' owners), railways 25 per km and 250 per station (split over the provinces on the line), airports 8,000 — all × the payer's productivity (labour costs) and the cost parameters.
- **Building**: roads 25 / 180 / 1,500 per km of construction length (km × (1 + barrier), twice over impassable land), upgrades pay the difference; railways 1,000 per km of construction length and 12,000 per new station; airports 300,000 — all × the builder's productivity.
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
| Sea lane (ocean shipping) | from each port to the main port (capital, else the largest port): km × 0.6 + 200 |
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

## Airports

In the air age, when the treasury holds twice the cost and upkeep fits, a nation opens an airport at its capital and then at cities of 300,000, up to one per 40 provinces (at most eight). Airports add +0.1 attraction and flight links in access.

## Growth, migration and cities

- **Capacity** this step = (capacity + rail extra) × 2 (industrial owner) × fertilizer multiplier × pull(attraction − devastation), where pull(a) = 1 + 4a for a ≥ 0 and (1 + a)² below.
- **Growth:** logistic at *Population growth* (+ *Industrial growth* for industrial owners) × (1 + 0.5 × devastation); people leave land that holds no one (×0.9 a year).
- **Attraction** = devastation + station/junction/airport bonus + capital pull × grown × scale + city pins, clamped to ±1. A capital grows into *Capital pull* over *Years to grow a capital*, more for larger nations (scale 0.4–1 by log size).
- **Migration:** each year *Migration to cities* of the people in unattractive provinces (more from devastated or shunned ones, up to 5×) moves to the nation's attractive provinces, weighted by room × (0.5 + integration) and never beyond their room; those who find no room stay.
- **Devastation** heals by *Recovery per year*. A province passing 1,000,000 people is logged as a *metropolis*; one that falls below a quarter of its peak (a peak of at least three times the founding population) as *ruined*, cleared when it is back above 60%.

## Assimilation

Each year `assimilation × dt × (0.5 + integration)` of a ruled province's other cultures takes the ruler's culture; shares under 0.5% are dropped and the majority recomputed.

## Events

`founded`, `independence`, `ended` (conquered or collapsed), `conquest` (a capital taken), `capital` (capital moved), `colony`, `union`, `renamed`, `war` (per half century: who took three or more provinces from whom, top 12), `era` (first three nations into each era, special lines for the first industrial, fertilizer, motor and air nations), `railway` (the first line and ~15% of the rest), `highway` (the first), `airport` (the first five), `bankruptcy`, `metropolis`, `ruined`.

## Outputs

- **Fields:** `owner`, `nation_culture`, `nation_population` (people per cell), `nation_attraction`, `railway` (0–3), `road` (best road touching the province, 0–3), `airport` (0/1), `nation_era` (owner's era + 1, 0 = no owner).
- **Meta:** counts (nations, ruled share, population, conquests, independences, colonies, bankruptcies, metropolises, ruins), the world era and leader, nations per era, transport totals, applied directives, and the tables `nations`, `provinces`, `events`, `railways`, `stations`, `roads`, `airports`, `cities` (top 100), `pins`, `ruins`.
- **Export:** see [export-import.md](export-import.md#stage-4-files).

## Typical result (default world: seed 3, level 6, default parameters)

About 3 s for 800–1949. 170 nations in 1949 (228 ever), 93% of land ruled, ~1,100 capitals taken, ~120 independences, 36 colonies, population ~110 M → ~440 M, 61 cities over a million, 59 in ruins. First industrial nation 1844, first fertilizer 1910, motor age 1920 (first highway 1926), air age 1934 (first airport 1936). In 1949: 16 nations in the air age, 5 motor, 12 fertilizer, 102 industry, 35 pre-industrial. About 440,000 km of track, 300,000 km of paved road, 23,000 km of highway; 139 railway lines (102,000 km, 493 stations, 88 junctions); 15 airports.

## Tuning notes

- The constants in the economy (costs, upkeep, 30% army, 40% infrastructure cap, two-year reserve) were tuned so that a paved road costs about a year of a mid-sized pre-industrial kingdom's income and treasuries hover around three years of income. They are judgement calls, not calibrated to data; *Tax share*, *Road cost* and *Railway and airport cost* scale them.
- The technology model went through two failed designs (a pure growth-rate model either converged everyone to the leader or let the whole world lag a century and a half). The target model above, with ranks, guarantees a spread of eras. *Technology lead* and *Technology catch-up* are the knobs; the 220-year maximum lag and the 80-year neighbour limit are constants in `advance_tech`.
- When changing the model, bump the last entry of `MODEL_VERSION` in `core/src/stages/mod.rs`.

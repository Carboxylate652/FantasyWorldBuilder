//! Directives: time-stamped steering actions for the Stage 3 and Stage 4
//! simulations ("a plague here in generation 120", "this nation turns
//! aggressive for 50 years").
//!
//! They are issued by hand or by the AI guide while a simulation runs step by
//! step, and stored in the project like the other override layers. Each one
//! records when it was issued (the generation for cultures, the year for
//! nations); a run applies it just before that generation or year, so
//! re-running a stage replays exactly the history that was steered. The same
//! list describes the actions as tools for the AI guide and as forms for the
//! UI, so both offer the same choices.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Directive {
    /// "cultures" (Stage 3) or "nations" (Stage 4).
    pub stage: String,
    /// When it applies: before this generation (cultures, counted from 0) or
    /// at this time in years (nations; fractional when steps are shorter than
    /// a year).
    pub at: f64,
    pub action: String,
    #[serde(default)]
    pub args: Value,
    /// Why: the reason given by the user or the guide, for the chronicle.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
    /// "user" or "guide".
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub by: String,
}

pub struct ActionSpec {
    pub stage: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    /// JSON schema of `args` (strict: every property listed in `required`,
    /// optional ones nullable).
    pub schema: Value,
}

fn ids(desc: &str) -> Value {
    json!({ "type": ["array", "null"], "items": { "type": "integer" }, "description": desc })
}

fn num(desc: &str, min: f64, max: f64) -> Value {
    json!({ "type": "number", "minimum": min, "maximum": max, "description": desc })
}

fn object(props: Value) -> Value {
    let required: Vec<String> = props.as_object().map(|o| o.keys().cloned().collect()).unwrap_or_default();
    json!({ "type": "object", "properties": props, "required": required, "additionalProperties": false })
}

/// Where an action applies: any mix of provinces, states and regions (ids from
/// the world summary).
fn place() -> [(&'static str, Value); 3] {
    [
        ("provinces", ids("Province ids")),
        ("states", ids("State ids: all their provinces")),
        ("regions", ids("Region ids: all their provinces")),
    ]
}

fn with_place(mut props: serde_json::Map<String, Value>) -> Value {
    for (k, v) in place() {
        props.insert(k.into(), v);
    }
    object(Value::Object(props))
}

fn map(v: Value) -> serde_json::Map<String, Value> {
    v.as_object().cloned().unwrap_or_default()
}

/// Every action, Stage 3 first.
pub fn actions() -> Vec<ActionSpec> {
    let years = || num("How long the effect lasts, in years", 1.0, 5000.0);
    vec![
        // ---------------------------------------------------------- Stage 3
        ActionSpec {
            stage: "cultures",
            name: "growth",
            description: "Make land hold more (factor > 1, a fertile age) or fewer people (factor < 1, a hard age) for a while, in the given places or for one culture's bands (culture id; places may then be empty).",
            schema: with_place(map(json!({ "culture": { "type": ["integer", "null"], "description": "Culture id, or null for everyone in the places" }, "factor": num("Carrying capacity and growth multiplier", 0.1, 5.0), "years": years() }))),
        },
        ActionSpec {
            stage: "cultures",
            name: "attraction",
            description: "Draw people to the places (value > 0: settlers and migrants prefer them, they hold more) or drive them away (value < 0) for a while.",
            schema: with_place(map(json!({ "value": num("−1 to +1", -1.0, 1.0), "years": years() }))),
        },
        ActionSpec {
            stage: "cultures",
            name: "isolate",
            description: "Cut the places off from the rest of the world for a while: no contact or migration across their edge, so the people inside drift apart into their own culture.",
            schema: with_place(map(json!({ "years": years() }))),
        },
        ActionSpec {
            stage: "cultures",
            name: "drift",
            description: "Make one culture change faster for a while (new customs, a schism): its bands drift apart, so it tends to split.",
            schema: object(json!({ "culture": { "type": "integer", "description": "Culture id" }, "factor": num("Drift multiplier", 1.0, 20.0), "years": years() })),
        },
        ActionSpec {
            stage: "cultures",
            name: "contact",
            description: "Bring two cultures together for a while (trade, intermarriage): their bands meet as if alike, so they tend to merge.",
            schema: object(json!({ "culture_a": { "type": "integer" }, "culture_b": { "type": "integer" }, "factor": num("Contact multiplier", 1.0, 50.0), "years": years() })),
        },
        ActionSpec {
            stage: "cultures",
            name: "catastrophe",
            description: "A plague, famine, flood or war kills a share of the people in the places at once.",
            schema: with_place(map(json!({ "severity": num("Share of people lost", 0.0, 0.95) }))),
        },
        ActionSpec {
            stage: "cultures",
            name: "settle",
            description: "A migration wave: new bands arrive in a province, of an existing culture (copying its customs) or of a new people (culture null).",
            schema: object(json!({ "province": { "type": "integer" }, "bands": num("Number of bands", 1.0, 50.0), "culture": { "type": ["integer", "null"], "description": "Culture id to copy, or null for a new people" } })),
        },
        // ---------------------------------------------------------- Stage 4
        ActionSpec {
            stage: "nations",
            name: "aggression",
            description: "Make a nation more (factor > 1) or less (factor < 1) expansionist for a while.",
            schema: object(json!({ "nation": { "type": "integer" }, "factor": num("Expansion multiplier", 0.0, 10.0), "years": years() })),
        },
        ActionSpec {
            stage: "nations",
            name: "expand_toward",
            description: "Point a nation's expansion at a place for a while: it prefers targets nearer to that province (colonies, a war goal).",
            schema: object(json!({ "nation": { "type": "integer" }, "province": { "type": "integer" }, "years": years() })),
        },
        ActionSpec {
            stage: "nations",
            name: "war",
            description: "A nation wages war on another for a while: it attacks that nation's provinces first and harder.",
            schema: object(json!({ "nation": { "type": "integer" }, "target": { "type": "integer" }, "years": years() })),
        },
        ActionSpec {
            stage: "nations",
            name: "peace",
            description: "Two nations keep the peace for a while: neither takes the other's land.",
            schema: object(json!({ "nation": { "type": "integer" }, "target": { "type": "integer" }, "years": years() })),
        },
        ActionSpec {
            stage: "nations",
            name: "stability",
            description: "Make a nation more stable (factor > 1: it holds together) or fragile (factor < 1: likelier to break apart) for a while.",
            schema: object(json!({ "nation": { "type": "integer" }, "factor": num("Stability multiplier", 0.05, 20.0), "years": years() })),
        },
        ActionSpec {
            stage: "nations",
            name: "split",
            description: "A nation breaks apart now: the provinces of one culture (culture id), or else its largest outlying part, form a new nation.",
            schema: object(json!({ "nation": { "type": "integer" }, "culture": { "type": ["integer", "null"], "description": "Culture id whose provinces secede, or null" } })),
        },
        ActionSpec {
            stage: "nations",
            name: "found",
            description: "A new nation is founded in a province now (a breakaway if the province is owned), optionally with a name.",
            schema: object(json!({ "province": { "type": "integer" }, "name": { "type": ["string", "null"] } })),
        },
        ActionSpec {
            stage: "nations",
            name: "union",
            description: "Two nations unite now: the second's land joins the first.",
            schema: object(json!({ "nation": { "type": "integer" }, "target": { "type": "integer", "description": "Nation that joins" } })),
        },
        ActionSpec {
            stage: "nations",
            name: "transfer",
            description: "A treaty hands the places to a nation now.",
            schema: with_place(map(json!({ "nation": { "type": "integer", "description": "Receiving nation" } }))),
        },
        ActionSpec {
            stage: "nations",
            name: "rename",
            description: "Give a nation a new name.",
            schema: object(json!({ "nation": { "type": "integer" }, "name": { "type": "string" } })),
        },
        ActionSpec {
            stage: "nations",
            name: "railway",
            description: "A nation builds a railway line between two of its provinces now (on its own land, cheapest route), paying for track and stations from its treasury (it may go into debt). Railways need the industrial era (the nation's own technology). Where the new line meets another, a junction station lets travellers change lines, at a time cost.",
            schema: object(json!({ "nation": { "type": "integer" }, "from": { "type": "integer", "description": "Province id" }, "to": { "type": "integer", "description": "Province id" } })),
        },
        ActionSpec {
            stage: "nations",
            name: "build_road",
            description: "A nation builds or upgrades a road between two of its provinces now (cheapest route over its own land), paying from its treasury (it may go into debt). Quality 1 track, 2 paved road, 3 highway (motor age only). Roads speed up trade, travel, armies and the spread of the ruler's culture, and cost upkeep every year, more the better they are.",
            schema: object(json!({ "nation": { "type": "integer" }, "from": { "type": "integer", "description": "Province id" }, "to": { "type": "integer", "description": "Province id" }, "quality": num("1 track, 2 paved road, 3 highway", 1.0, 3.0) })),
        },
        ActionSpec {
            stage: "nations",
            name: "airport",
            description: "A nation builds an airport in one of its provinces now (air age only), paying from its treasury. Airports link a nation's cities by air and cost upkeep.",
            schema: object(json!({ "nation": { "type": "integer" }, "province": { "type": "integer" } })),
        },
        ActionSpec {
            stage: "nations",
            name: "subsidy",
            description: "A windfall fills a nation's treasury now (silver mines, tribute, foreign loans, war reparations): the given number of years of its income.",
            schema: object(json!({ "nation": { "type": "integer" }, "years": num("Years of income", 0.1, 100.0) })),
        },
        ActionSpec {
            stage: "nations",
            name: "tech",
            description: "A nation's technology jumps ahead (a scientific revolution, foreign experts) by the given years now; a negative value sets it back (a dark age). Its provinces embrace every born institution whose era year the new technology passes, so it can enter later eras at once; an institution not yet born whose year it passes is born in its capital.",
            schema: object(json!({ "nation": { "type": "integer" }, "years": num("Technology years gained (negative: lost)", -500.0, 500.0) })),
        },
        ActionSpec {
            stage: "nations",
            name: "port",
            description: "A nation builds a port in one of its coastal provinces now (ocean shipping era), whatever its harbour: sea lanes, colonies and the spread of institutions run through ports.",
            schema: object(json!({ "nation": { "type": "integer" }, "province": { "type": "integer" } })),
        },
        ActionSpec {
            stage: "nations",
            name: "institution_birth",
            description: "Choose where an institution is born (0 gunpowder, 1 navigation, 2 industrialisation, 3 synthetic fertilizer, 4 motorisation, 5 aviation): a province, or null to let chance pick among the candidates. It applies when the institution emerges (at once if it is due). Institutions spread province by province over land, roads, railways, ports and airports; a nation enters the matching era once most of its provinces have embraced it.",
            schema: object(json!({ "institution": num("Institution index (0–5)", 0.0, 5.0), "province": { "type": ["integer", "null"], "description": "Birthplace province id, or null for chance" } })),
        },
        ActionSpec {
            stage: "nations",
            name: "reform",
            description: "A nation reforms on the model of more advanced nations (westernization) for some years: institutions spread through its provinces three times as fast, at the cost of a year's income and of stability while it lasts.",
            schema: object(json!({ "nation": { "type": "integer" }, "years": years() })),
        },
        ActionSpec {
            stage: "nations",
            name: "feudal_empire",
            description: "Make a nation a feudal empire (on: true): its emperor enfeoffs kings over the regions of the imperial territory (the given places, or all its land if none), the kings enfeoff dukes over states, and counties (provinces) and baronies complete the hierarchy. Members never fight each other, never break apart and defend each other; the empire lasts, whatever the era, until dissolved (on: false), and the title passes by election if the emperor's own land is lost. Land of the emperor outside the given places stays outside the empire.",
            schema: with_place(map(json!({ "nation": { "type": "integer" }, "on": { "type": "boolean" }, "name": { "type": ["string", "null"], "description": "Name of the empire, or null for one from the nation's" } }))),
        },
        ActionSpec {
            stage: "nations",
            name: "tag",
            description: "Add (on: true) or remove a special tag. Nation tags: isolationist (no colonies, institutions arrive slowly, never reforms), expansionist (aggression ×1.5), merchant_republic (ports anywhere passable, double trade from ports, more colonies), eternal (never breaks apart, defends ×1.5). State tags: holy_land (everyone covets it), free_state (its provinces can't change hands by war or settlement), institution_cradle (institutions prefer to be born here). World tags (id null): no_overseas_colonies, slow_institutions, fast_institutions, frequent_wars, stable_realms.",
            schema: object(json!({
                "scope": { "type": "string", "enum": ["world", "state", "nation"] },
                "id": { "type": ["integer", "null"], "description": "State or nation id; null for world tags" },
                "tag": { "type": "string", "enum": ["isolationist", "expansionist", "merchant_republic", "eternal", "holy_land", "free_state", "institution_cradle", "no_overseas_colonies", "slow_institutions", "fast_institutions", "frequent_wars", "stable_realms"] },
                "on": { "type": "boolean" },
            })),
        },
        ActionSpec {
            stage: "nations",
            name: "pin_add",
            description: "Place a city pin: from now on the province draws people (value > 0: a boom town, a new capital, a trade hub; up to +1 a metropolis) or loses them (value < 0: a city abandoned after war, plague or a dried-up mine; −1 empties it). Lasts the given years, or until moved or removed (years null). Returns a pin id shown in the state.",
            schema: object(json!({ "province": { "type": "integer" }, "value": num("−1 (abandoned) to +1 (metropolis)", -1.0, 1.0), "years": { "type": ["number", "null"], "minimum": 1.0, "maximum": 5000.0, "description": "How long it lasts, or null until removed" }, "label": { "type": ["string", "null"], "description": "What it stands for (\"royal capital\", \"silver rush\", \"burned in the war\")" } })),
        },
        ActionSpec {
            stage: "nations",
            name: "pin_move",
            description: "Move a city pin to another province (the court moves, trade shifts to a new port).",
            schema: object(json!({ "pin": { "type": "integer", "description": "Pin id" }, "province": { "type": "integer" } })),
        },
        ActionSpec {
            stage: "nations",
            name: "pin_remove",
            description: "Remove a city pin: the province goes back to its own fortunes.",
            schema: object(json!({ "pin": { "type": "integer", "description": "Pin id" } })),
        },
        ActionSpec {
            stage: "nations",
            name: "move_capital",
            description: "A nation moves its capital to one of its provinces; the new capital grows over the following decades, the old one loses its pull.",
            schema: object(json!({ "nation": { "type": "integer" }, "province": { "type": "integer" } })),
        },
        ActionSpec {
            stage: "nations",
            name: "catastrophe",
            description: "A plague, famine or war kills a share of the people in the places at once.",
            schema: with_place(map(json!({ "severity": num("Share of people lost", 0.0, 0.95) }))),
        },
        // ---------------------------------------------------------- both
        ActionSpec {
            stage: "both",
            name: "note",
            description: "Record a note in the chronicle (no effect on the simulation).",
            schema: object(json!({ "text": { "type": "string" } })),
        },
    ]
}

/// Actions available in a stage.
pub fn actions_for(stage: &str) -> Vec<ActionSpec> {
    actions().into_iter().filter(|a| a.stage == stage || a.stage == "both").collect()
}

/// Check a directive's stage, action and arguments against its schema (types,
/// required fields, ranges), so a bad one is refused before it is stored.
pub fn validate(d: &Directive) -> Result<(), String> {
    if d.stage != "cultures" && d.stage != "nations" {
        return Err(format!("unknown stage `{}`", d.stage));
    }
    let spec = actions_for(&d.stage).into_iter().find(|a| a.name == d.action).ok_or_else(|| format!("unknown {} action `{}`", d.stage, d.action))?;
    let args = d.args.as_object().ok_or("arguments must be an object")?;
    let props = spec.schema["properties"].as_object().unwrap();
    for k in args.keys() {
        if !props.contains_key(k) {
            return Err(format!("{}: unknown argument `{k}`", d.action));
        }
    }
    for (k, s) in props {
        let v = args.get(k).unwrap_or(&Value::Null);
        let types: Vec<&str> = match &s["type"] {
            Value::String(t) => vec![t.as_str()],
            Value::Array(a) => a.iter().filter_map(|t| t.as_str()).collect(),
            _ => vec![],
        };
        let ok = types.iter().any(|t| match *t {
            "null" => v.is_null(),
            "integer" => v.as_i64().is_some() || v.as_f64().is_some_and(|x| x.fract() == 0.0),
            "number" => v.is_number(),
            "string" => v.is_string(),
            "boolean" => v.is_boolean(),
            "array" => v.as_array().is_some_and(|a| a.iter().all(|x| x.as_i64().is_some() || x.as_f64().is_some_and(|f| f.fract() == 0.0))),
            _ => false,
        });
        if !ok {
            return Err(format!("{}: `{k}` should be {}", d.action, types.join(" or ")));
        }
        if let (Some(allowed), Some(v)) = (s["enum"].as_array(), v.as_str()) {
            if !allowed.iter().any(|a| a.as_str() == Some(v)) {
                return Err(format!("{}: `{k}` must be one of {}", d.action, allowed.iter().filter_map(|a| a.as_str()).collect::<Vec<_>>().join(", ")));
            }
        }
        if let Some(x) = v.as_f64() {
            if let Some(min) = s["minimum"].as_f64() {
                if x < min {
                    return Err(format!("{}: `{k}` must be at least {min}", d.action));
                }
            }
            if let Some(max) = s["maximum"].as_f64() {
                if x > max {
                    return Err(format!("{}: `{k}` must be at most {max}", d.action));
                }
            }
        }
    }
    Ok(())
}

/// Integer list argument (missing or null = empty).
pub fn arg_ids(args: &Value, key: &str) -> Vec<u64> {
    args[key].as_array().map(|a| a.iter().filter_map(|x| x.as_u64().or_else(|| x.as_f64().map(|f| f as u64))).collect()).unwrap_or_default()
}

pub fn arg_u64(args: &Value, key: &str) -> Option<u64> {
    args[key].as_u64().or_else(|| args[key].as_f64().filter(|f| *f >= 0.0).map(|f| f as u64))
}

pub fn arg_f64(args: &Value, key: &str, default: f64) -> f64 {
    args[key].as_f64().unwrap_or(default)
}

/// Directives of one stage in the order they apply (by time, then as issued).
pub fn of_stage<'a>(all: &'a [Directive], stage: &str) -> Vec<&'a Directive> {
    let mut v: Vec<(usize, &Directive)> = all.iter().enumerate().filter(|(_, d)| d.stage == stage).collect();
    v.sort_by(|(ka, a), (kb, b)| a.at.total_cmp(&b.at).then(ka.cmp(kb)));
    v.into_iter().map(|x| x.1).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(stage: &str, action: &str, args: Value) -> Directive {
        Directive { stage: stage.into(), at: 0.0, action: action.into(), args, note: String::new(), by: String::new() }
    }

    #[test]
    fn schemas_are_strict_and_validation_works() {
        for a in actions() {
            let req = a.schema["required"].as_array().unwrap().len();
            assert_eq!(req, a.schema["properties"].as_object().unwrap().len(), "{} lists every property as required", a.name);
            assert_eq!(a.schema["additionalProperties"], false);
        }
        assert!(validate(&d("cultures", "catastrophe", json!({ "provinces": [3, 4], "states": null, "regions": null, "severity": 0.5 }))).is_ok());
        assert!(validate(&d("cultures", "catastrophe", json!({ "provinces": [3], "severity": 2.0 }))).is_err(), "range");
        assert!(validate(&d("cultures", "war", json!({}))).is_err(), "nations action in Stage 3");
        assert!(validate(&d("nations", "rename", json!({ "nation": 2, "name": "Avalon" }))).is_ok());
        assert!(validate(&d("nations", "rename", json!({ "nation": "two", "name": "Avalon" }))).is_err(), "type");
        assert!(validate(&d("nations", "rename", json!({ "nation": 2, "name": "A", "extra": 1 }))).is_err(), "unknown argument");
        assert!(validate(&d("nations", "note", json!({ "text": "The long peace" }))).is_ok());
    }
}

//! AI-guided history.
//!
//! The user writes what they want to see ("a sea empire in the south that
//! falls apart into three kingdoms by 1800"). Turn by turn, the live Stage 3
//! or 4 simulation advances a stretch of time; the guide gets a summary of
//! the world and the user's goal, and answers with directives (the same
//! actions the user can issue by hand, offered as tools) and a line for the
//! chronicle. The directives are stored with the project, so the steered
//! history replays exactly; the model is never needed to re-run it.
//!
//! Providers:
//! - **Anthropic** — the Claude API (Messages API, raw HTTP: there is no
//!   official Rust SDK).
//! - **OpenAI-compatible** — any `/chat/completions` endpoint with tool
//!   calls: OpenAI, the Vercel AI Gateway, OpenRouter, a local server
//!   (Ollama, LM Studio), or another gateway.
//!
//! API keys are kept in the user's settings folder (or read from the usual
//! environment variables), never in a world project, so sharing a world never
//! shares a key.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use worldcore::api::{self, ProgressState, Reply, Session};

// ---------------------------------------------------------------- settings

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct GuideConfig {
    /// "anthropic" or "openai" (OpenAI-compatible chat completions).
    pub provider: String,
    pub base_url: String,
    pub model: String,
    /// Anthropic only: effort (low, medium, high, xhigh, max).
    pub effort: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub api_key: String,
}

impl Default for GuideConfig {
    fn default() -> Self {
        GuideConfig { provider: "anthropic".into(), base_url: "https://api.anthropic.com".into(), model: "claude-opus-5-5".into(), effort: "medium".into(), api_key: String::new() }
    }
}

/// Ready-made provider settings for the UI.
pub fn presets() -> Value {
    json!([
        { "id": "anthropic", "label": "Anthropic (Claude API)", "provider": "anthropic", "base_url": "https://api.anthropic.com", "model": "claude-opus-5-5", "key_env": "ANTHROPIC_API_KEY",
          "hint": "Key from console.anthropic.com. claude-opus-5-5 is the default; claude-sonnet-5-5 or claude-haiku-5-5 cost less per turn." },
        { "id": "vercel", "label": "Vercel AI Gateway", "provider": "openai", "base_url": "https://ai-gateway.vercel.sh/v1", "model": "anthropic/claude-opus-5-5", "key_env": "AI_GATEWAY_API_KEY",
          "hint": "OpenAI-compatible endpoint; models are named provider/model. Check the gateway's model list for exact ids." },
        { "id": "openai", "label": "OpenAI", "provider": "openai", "base_url": "https://api.openai.com/v1", "model": "", "key_env": "OPENAI_API_KEY",
          "hint": "Enter a model id from your OpenAI account that supports tool calls." },
        { "id": "openrouter", "label": "OpenRouter", "provider": "openai", "base_url": "https://openrouter.ai/api/v1", "model": "anthropic/claude-opus-5-5", "key_env": "OPENROUTER_API_KEY",
          "hint": "OpenAI-compatible; check OpenRouter's model list for exact ids." },
        { "id": "local", "label": "Local server (Ollama, LM Studio)", "provider": "openai", "base_url": "http://localhost:11434/v1", "model": "", "key_env": "",
          "hint": "Any local OpenAI-compatible server; pick a model that supports tool calls. No key needed." },
        { "id": "custom", "label": "Other gateway", "provider": "openai", "base_url": "", "model": "", "key_env": "",
          "hint": "Any OpenAI-compatible /chat/completions endpoint, or switch the format to Anthropic for an Anthropic-compatible one." }
    ])
}

/// The settings file: %APPDATA%\FantasyWorldMaker\guide.json on Windows,
/// ~/.config/fantasy-world-maker/guide.json elsewhere (FWM_CONFIG_DIR overrides).
pub fn config_path() -> PathBuf {
    if let Some(d) = std::env::var_os("FWM_CONFIG_DIR") {
        return PathBuf::from(d).join("guide.json");
    }
    if cfg!(windows) {
        if let Some(a) = std::env::var_os("APPDATA") {
            return PathBuf::from(a).join("FantasyWorldMaker").join("guide.json");
        }
    }
    let base = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config"))).unwrap_or_else(std::env::temp_dir);
    base.join("fantasy-world-maker").join("guide.json")
}

pub fn load_config() -> GuideConfig {
    std::fs::read_to_string(config_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn save_config(c: &GuideConfig) -> Result<(), String> {
    let p = config_path();
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
    }
    std::fs::write(&p, serde_json::to_string_pretty(c).unwrap()).map_err(|e| format!("{}: {e}", p.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// The key to use: the saved one, else the provider's usual environment variable.
fn api_key(c: &GuideConfig) -> (String, &'static str) {
    if !c.api_key.is_empty() {
        return (c.api_key.clone(), "settings");
    }
    let vars: &[&'static str] = if c.provider == "anthropic" {
        &["ANTHROPIC_API_KEY"]
    } else if c.base_url.contains("ai-gateway.vercel.sh") {
        &["AI_GATEWAY_API_KEY", "VERCEL_OIDC_TOKEN"]
    } else if c.base_url.contains("openrouter.ai") {
        &["OPENROUTER_API_KEY"]
    } else {
        &["OPENAI_API_KEY"]
    };
    for v in vars {
        if let Ok(k) = std::env::var(v) {
            if !k.is_empty() {
                return (k, v);
            }
        }
    }
    (String::new(), "")
}

/// Settings as shown to the UI: never the key itself.
fn public_config(c: &GuideConfig) -> Value {
    let (key, source) = api_key(c);
    json!({
        "provider": c.provider, "base_url": c.base_url, "model": c.model, "effort": c.effort,
        "has_key": !key.is_empty(), "key_source": source,
        "key_hint": if key.len() > 8 { format!("…{}", &key[key.len() - 4..]) } else { String::new() },
        "path": config_path().display().to_string(),
    })
}

// ---------------------------------------------------------------- LLM calls

/// A tool the model may call.
pub struct Tool {
    pub name: String,
    pub description: String,
    pub schema: Value,
}

#[derive(Debug, Default, Serialize)]
pub struct Answer {
    pub text: String,
    pub calls: Vec<(String, Value)>,
    pub stop: String,
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

/// HTTP client; the environment's proxy is used except for local servers.
fn agent(base: &str) -> ureq::Agent {
    let host = base.split("://").nth(1).unwrap_or(base).split(['/', ':']).next().unwrap_or("");
    let local = matches!(host, "localhost" | "127.0.0.1" | "[") || host.starts_with("127.") || base.contains("://[::1]");
    ureq::AgentBuilder::new().try_proxy_from_env(!local).timeout_connect(Duration::from_secs(15)).timeout_read(Duration::from_secs(600)).build()
}

fn http_error(e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(code, r) => {
            let body = r.into_string().unwrap_or_default();
            let msg = serde_json::from_str::<Value>(&body).ok().and_then(|v| v["error"]["message"].as_str().map(str::to_string).or_else(|| v["error"].as_str().map(str::to_string))).unwrap_or_else(|| body.chars().take(300).collect());
            match code {
                401 | 403 => format!("the provider refused the key ({code}): {msg}"),
                404 => format!("not found ({code}): check the base URL and model id. {msg}"),
                429 => format!("rate limited or out of credit (429): {msg}"),
                _ => format!("provider error {code}: {msg}"),
            }
        }
        ureq::Error::Transport(t) => format!("could not reach the provider: {t}"),
    }
}

/// Models that accept the Claude API's server-side refusal fallback.
fn takes_fallback(model: &str) -> bool {
    matches!(model, "claude-opus-5-5" | "claude-opus-5" | "claude-fable-5-1" | "claude-fable-5" | "claude-sonnet-5-5")
}

/// One request, no conversation history: the system prompt, one user
/// message and the tools. The answer's text and tool calls are returned; the
/// calls are not answered (each turn starts fresh from the world's state).
pub fn complete(c: &GuideConfig, system: &str, user: &str, tools: &[Tool], max_tokens: u32) -> Result<Answer, String> {
    let (key, _) = api_key(c);
    if c.model.trim().is_empty() {
        return Err("choose a model in the guide settings".into());
    }
    let base = c.base_url.trim_end_matches('/');
    if base.is_empty() {
        return Err("enter the provider's base URL in the guide settings".into());
    }
    if c.provider == "anthropic" {
        if key.is_empty() {
            return Err("no API key: add one in the guide settings or set ANTHROPIC_API_KEY".into());
        }
        let mut body = json!({
            "model": c.model,
            "max_tokens": max_tokens,
            // Stable prefix first (tools, then this system prompt), cached across turns.
            "system": [{ "type": "text", "text": system, "cache_control": { "type": "ephemeral" } }],
            "messages": [{ "role": "user", "content": user }],
            "tools": tools.iter().map(|t| json!({ "name": t.name, "description": t.description, "input_schema": t.schema })).collect::<Vec<_>>(),
            "tool_choice": { "type": "auto" },
        });
        if !c.effort.is_empty() && c.model.starts_with("claude-") && !c.model.starts_with("claude-haiku-4") {
            body["output_config"] = json!({ "effort": c.effort });
        }
        let official = base == "https://api.anthropic.com";
        let mut req = agent(base).post(&format!("{base}/v1/messages")).set("content-type", "application/json").set("x-api-key", &key).set("anthropic-version", "2023-06-01");
        if official && takes_fallback(&c.model) {
            body["fallbacks"] = json!("default");
            req = req.set("anthropic-beta", "server-side-fallback-2026-07-01");
        }
        let v: Value = req.send_json(body).map_err(http_error)?.into_json().map_err(|e| format!("unreadable answer: {e}"))?;
        let stop = v["stop_reason"].as_str().unwrap_or("").to_string();
        if stop == "refusal" {
            let why = v["stop_details"]["explanation"].as_str().or_else(|| v["stop_details"]["category"].as_str()).unwrap_or("no reason given");
            return Err(format!("the model declined this turn ({why}); try rewording the goal"));
        }
        let mut a = Answer { stop, model: v["model"].as_str().unwrap_or("").into(), input_tokens: v["usage"]["input_tokens"].as_u64().unwrap_or(0) + v["usage"]["cache_read_input_tokens"].as_u64().unwrap_or(0) + v["usage"]["cache_creation_input_tokens"].as_u64().unwrap_or(0), output_tokens: v["usage"]["output_tokens"].as_u64().unwrap_or(0), ..Default::default() };
        for b in v["content"].as_array().into_iter().flatten() {
            match b["type"].as_str() {
                Some("text") => {
                    if !a.text.is_empty() {
                        a.text.push('\n');
                    }
                    a.text.push_str(b["text"].as_str().unwrap_or(""));
                }
                Some("tool_use") => a.calls.push((b["name"].as_str().unwrap_or("").into(), b["input"].clone())),
                _ => {}
            }
        }
        Ok(a)
    } else {
        let body = json!({
            "model": c.model,
            "messages": [{ "role": "system", "content": system }, { "role": "user", "content": user }],
            "tools": tools.iter().map(|t| json!({ "type": "function", "function": { "name": t.name, "description": t.description, "parameters": t.schema } })).collect::<Vec<_>>(),
            "tool_choice": "auto",
        });
        let mut req = agent(base).post(&format!("{base}/chat/completions")).set("content-type", "application/json");
        if !key.is_empty() {
            req = req.set("authorization", &format!("Bearer {key}"));
        }
        let v: Value = req.send_json(body).map_err(http_error)?.into_json().map_err(|e| format!("unreadable answer: {e}"))?;
        let m = &v["choices"][0]["message"];
        let mut a = Answer {
            text: m["content"].as_str().unwrap_or("").to_string(),
            stop: v["choices"][0]["finish_reason"].as_str().unwrap_or("").into(),
            model: v["model"].as_str().unwrap_or("").into(),
            input_tokens: v["usage"]["prompt_tokens"].as_u64().unwrap_or(0),
            output_tokens: v["usage"]["completion_tokens"].as_u64().unwrap_or(0),
            ..Default::default()
        };
        for tc in m["tool_calls"].as_array().into_iter().flatten() {
            let f = &tc["function"];
            let args = match &f["arguments"] {
                Value::String(s) => serde_json::from_str(s).unwrap_or(Value::Null),
                other => other.clone(),
            };
            a.calls.push((f["name"].as_str().unwrap_or("").into(), args));
        }
        Ok(a)
    }
}

// ---------------------------------------------------------------- turns

pub const SYSTEM: &str = "You steer a world-history simulation for a fantasy map maker. The user describes the history they want to see. \
Each turn you get the state of the simulation: in Stage 3 (cultures), bands of people spread, meet and split into cultures over generations of about 25 years; \
in Stage 4 (nations), nations form, expand, fight, colonise and break apart, year by year up to the map's start date. \
Use the tools to nudge the simulation toward the user's goal. The tools change conditions (growth, isolation, wars, stability, unions, treaties); \
the simulation decides what follows, so prefer a few plausible nudges, aimed with care, over forcing outcomes, and act only when it serves the goal. \
Take ids (cultures, nations, provinces, regions) from the state; never invent them. Effects with a duration last that many years from now. \
After any tool calls, write one to three sentences for the chronicle: what happened since the last turn and what you set in motion. \
In Stage 4, cities rise and fall with history (capitals grow, sacked cities empty, stations draw people); \
city pins (pin_add, pin_move, pin_remove) let you make a boom town, a metropolis or an abandoned city wherever the story needs one, \
and move_capital moves a court. Keep the pins you placed in mind: move or remove them when the story moves on. \
Each era opens with an institution (gunpowder, navigation, industrialisation, synthetic fertilizer, motorisation, aviation) born in one province \
and spreading over land, roads, railways, ports and airports; a nation enters the era once most of its provinces have embraced it, rich nations first. \
The state lists the next institution and its candidate birthplaces: institution_birth chooses one (or null for chance) before it is born. \
Nations have treasuries that pay for armies, roads, railway lines, ports and airports; tech, reform, subsidy, build_road, railway, port and airport shape who modernises first. \
Tags (tag) give the world, states or nations special rules, and feudal_empire makes a nation an empire of kings and dukes that lasts until you dissolve it. \
You may choose how many years pass before the next turn with set_pace.";

fn pace_tool() -> Tool {
    Tool {
        name: "set_pace".into(),
        description: "Choose how many years pass before your next turn (shorter when events need close attention).".into(),
        schema: json!({ "type": "object", "properties": { "years": { "type": "number", "minimum": 5, "maximum": 1000 } }, "required": ["years"], "additionalProperties": false }),
    }
}

fn tools_for(stage: &str) -> Vec<Tool> {
    let mut t: Vec<Tool> = worldcore::directives::actions_for(stage).into_iter().map(|a| Tool { name: a.name.to_string(), description: a.description.to_string(), schema: a.schema }).collect();
    t.push(pace_tool());
    t
}

/// The world summary, trimmed to what the guide needs.
pub fn brief(summary: &Value) -> String {
    let mut s = summary.clone();
    let trim = |v: &mut Value, key: &str, n: usize| {
        if let Some(a) = v[key].as_array_mut() {
            a.truncate(n);
        }
    };
    trim(&mut s, "cultures", 24);
    trim(&mut s, "nations", 30);
    if let Some(regions) = s["regions"].as_array_mut() {
        regions.sort_by(|a, b| b["population"].as_f64().partial_cmp(&a["population"].as_f64()).unwrap_or(std::cmp::Ordering::Equal));
        regions.truncate(80);
    }
    // Earlier directives and notes, newest last: the chronicle so far.
    if let Some(d) = s["directives"].as_array_mut() {
        let keep = d.len().saturating_sub(30);
        d.drain(..keep);
    }
    if let Some(n) = s["nations"].as_array_mut() {
        for x in n.iter_mut() {
            if let Some(o) = x.as_object_mut() {
                for k in ["color", "era_index", "upkeep", "railway_provinces", "ended", "fate", "fate_other", "road_km", "regions"] {
                    o.remove(k);
                }
            }
        }
    }
    serde_json::to_string(&s).unwrap_or_default()
}

fn call(session: &Mutex<Session>, progress: &Arc<Mutex<ProgressState>>, cmd: &str, args: Value) -> Result<Value, String> {
    match api::handle(session, progress, cmd, args)? {
        Reply::Json(v) => Ok(v),
        Reply::Bytes(_) => Ok(Value::Null),
    }
}

/// One guided turn: advance `years` (unless 0), ask the model, apply its
/// directives and record its chronicle line as a note.
pub fn turn(session: &Mutex<Session>, progress: &Arc<Mutex<ProgressState>>, goal: &str, years: f64, max_actions: usize) -> Result<Value, String> {
    if goal.trim().is_empty() {
        return Err("describe the history you want to see first".into());
    }
    let cfg = load_config();
    let st = if years > 0.0 { call(session, progress, "sim_step", json!({ "years": years }))? } else { call(session, progress, "sim_state", json!({}))? };
    let sim = &st["sim"];
    if sim["done"] == true {
        return Ok(json!({ "done": true, "position": sim["position"], "narration": "", "actions": [] }));
    }
    let stage = sim["stage"].as_str().unwrap_or("cultures").to_string();
    let when = if stage == "nations" { format!("year {}, month {}, of {}–{}", sim["year"], sim["month"], sim["start_year"], sim["end_year"]) } else { format!("generation {} of {} (year {})", sim["tick"], sim["ticks"], sim["year"]) };
    let user = format!(
        "The history the user wants:\n{}\n\nNow: Stage {} ({when}). At most {max_actions} tool calls this turn.\n\nState of the world (JSON):\n{}",
        goal.trim(),
        if stage == "nations" { 4 } else { 3 },
        brief(sim)
    );
    if let Ok(mut p) = progress.lock() {
        *p = ProgressState { running: true, task: "guide".into(), step: stage.clone(), frac: 0.0, msg: format!("Asking {} about {when}", cfg.model) };
    }
    let answer = complete(&cfg, SYSTEM, &user, &tools_for(&stage), 16000);
    if let Ok(mut p) = progress.lock() {
        p.running = false;
    }
    let answer = answer?;
    let mut actions = Vec::new();
    let mut pace = None;
    for (name, input) in answer.calls.iter().take(max_actions.max(1)) {
        if name == "set_pace" {
            pace = input["years"].as_f64();
            continue;
        }
        let r = call(session, progress, "sim_directive", json!({ "action": name, "args": input, "by": "guide" }));
        actions.push(json!({ "action": name, "args": input, "ok": r.is_ok(), "error": r.err() }));
    }
    let narration = answer.text.trim().to_string();
    if !narration.is_empty() {
        let _ = call(session, progress, "sim_directive", json!({ "action": "note", "args": { "text": narration }, "by": "guide" }));
    }
    let st = call(session, progress, "sim_state", json!({}))?;
    Ok(json!({
        "done": st["sim"]["done"], "position": st["sim"]["position"], "stage": stage,
        "narration": narration, "actions": actions, "next_years": pace,
        "model": answer.model, "stop": answer.stop, "input_tokens": answer.input_tokens, "output_tokens": answer.output_tokens,
        "status": st,
    }))
}

/// UI commands: guide_presets, guide_config_get, guide_config_set,
/// guide_test, guide_turn. None for other commands.
pub fn handle(session: &Mutex<Session>, progress: &Arc<Mutex<ProgressState>>, cmd: &str, args: &Value) -> Option<Result<Value, String>> {
    Some(match cmd {
        "guide_presets" => Ok(presets()),
        "guide_config_get" => Ok(public_config(&load_config())),
        "guide_config_set" => (|| {
            // Fields left out keep their value; api_key "" removes the saved key.
            let mut c = load_config();
            for (k, slot) in [("provider", &mut c.provider), ("base_url", &mut c.base_url), ("model", &mut c.model), ("effort", &mut c.effort), ("api_key", &mut c.api_key)] {
                if let Some(v) = args[k].as_str() {
                    *slot = v.trim().to_string();
                }
            }
            if c.provider != "anthropic" && c.provider != "openai" {
                return Err("provider must be anthropic or openai".to_string());
            }
            save_config(&c)?;
            Ok(public_config(&c))
        })(),
        "guide_test" => (|| {
            let c = load_config();
            let a = complete(&c, "Answer in one short sentence.", "Say hello to a fantasy world maker.", &[pace_tool()], 2000)?;
            Ok(json!({ "ok": true, "model": a.model, "text": a.text, "input_tokens": a.input_tokens, "output_tokens": a.output_tokens }))
        })(),
        "guide_turn" => turn(
            session,
            progress,
            args["goal"].as_str().unwrap_or(""),
            args["years"].as_f64().unwrap_or(50.0).clamp(0.0, 2000.0),
            args["max_actions"].as_u64().unwrap_or(4).clamp(1, 12) as usize,
        ),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tools_and_brief() {
        let t = tools_for("nations");
        assert!(t.iter().any(|x| x.name == "war") && t.iter().any(|x| x.name == "set_pace") && t.iter().any(|x| x.name == "note"));
        assert!(!t.iter().any(|x| x.name == "drift"), "Stage 3 actions stay out of Stage 4");
        let s = json!({ "nations": (0..50).map(|i| json!({ "id": i, "color": [1, 2, 3] })).collect::<Vec<_>>(), "regions": [{ "population": 1.0 }, { "population": 5.0 }], "directives": [] });
        let b: Value = serde_json::from_str(&brief(&s)).unwrap();
        assert_eq!(b["nations"].as_array().unwrap().len(), 30);
        assert!(b["nations"][0].get("color").is_none());
        assert_eq!(b["regions"][0]["population"], 5.0, "most populous regions first");
    }

    #[test]
    fn settings_never_expose_the_key() {
        let c = GuideConfig { api_key: "sk-test-1234567890abcd".into(), ..Default::default() };
        let p = public_config(&c);
        assert!(!p.to_string().contains("sk-test"));
        assert_eq!(p["key_hint"], "…abcd");
        assert!(takes_fallback("claude-opus-5-5") && !takes_fallback("claude-haiku-5-5"));
    }
}

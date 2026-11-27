//! A guided turn against a mock provider (both API formats): the request is
//! well formed, and the model's tool calls become directives in the live run.

use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use worldcore::api::{handle, ProgressState, Reply, Session};
use worldcore::params::WorldParams;

type SeenRequest = (String, Vec<(String, String)>, Value);

fn serve(answers: Vec<Value>) -> (String, std::thread::JoinHandle<Vec<SeenRequest>>) {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let addr = format!("http://{}", server.server_addr().to_ip().unwrap());
    let h = std::thread::spawn(move || {
        let mut seen = Vec::new();
        for a in answers {
            let mut req = server.recv().unwrap();
            let mut body = String::new();
            req.as_reader().read_to_string(&mut body).unwrap();
            let headers = req.headers().iter().map(|h| (h.field.as_str().as_str().to_ascii_lowercase(), h.value.as_str().to_string())).collect();
            seen.push((req.url().to_string(), headers, serde_json::from_str(&body).unwrap_or(Value::Null)));
            let resp = tiny_http::Response::from_string(a.to_string()).with_header("content-type: application/json".parse::<tiny_http::Header>().unwrap());
            req.respond(resp).unwrap();
        }
        seen
    });
    (addr, h)
}

#[test]
fn guided_turns_with_both_formats() {
    let dir = std::env::temp_dir().join(format!("fwm-guide-test-{}", std::process::id()));
    std::env::set_var("FWM_CONFIG_DIR", &dir);
    let mut p = WorldParams::default();
    p.planet.seed = 5;
    p.planet.grid_level = 5;
    p.climate.climate_level = 5;
    p.cultures.ticks = 40;
    p.nations.start_year = 1500;
    let session = Mutex::new(Session::new(p));
    let progress = Arc::new(Mutex::new(ProgressState::default()));
    let api = |cmd: &str, args: Value| match handle(&session, &progress, cmd, args) {
        Ok(Reply::Json(v)) => v,
        Ok(_) => Value::Null,
        Err(e) => panic!("{cmd}: {e}"),
    };
    api("sim_start", json!({ "stage": "nations" }));
    let st2 = api("sim_step", json!({ "years": 100 }));
    let n1 = st2["sim"]["nations"][0]["id"].as_u64().expect("a nation");

    // Anthropic format: a tool call, a bad tool call, a pace and a chronicle line.
    let (addr, h) = serve(vec![json!({
        "model": "claude-opus-5-5", "stop_reason": "tool_use",
        "usage": { "input_tokens": 1200, "output_tokens": 80 },
        "content": [
            { "type": "thinking", "thinking": "" },
            { "type": "tool_use", "id": "t1", "name": "aggression", "input": { "nation": n1, "factor": 3.0, "years": 100 } },
            { "type": "tool_use", "id": "t2", "name": "aggression", "input": { "nation": n1, "factor": 99.0, "years": 100 } },
            { "type": "tool_use", "id": "t3", "name": "set_pace", "input": { "years": 25 } },
            { "type": "text", "text": "The northern kingdom stirs." }
        ]
    })]);
    let cfg = |provider: &str, base: &str, model: &str, key: &str| {
        let r = fwm_guide::handle(&session, &progress, "guide_config_set", &json!({ "provider": provider, "base_url": base, "model": model, "api_key": key, "effort": "medium" })).unwrap().unwrap();
        assert_eq!(r["has_key"], !key.is_empty());
        assert!(!r.to_string().contains(key) || key.is_empty(), "key never echoed");
    };
    cfg("anthropic", &addr, "claude-opus-5-5", "sk-ant-test-123456");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(dir.join("guide.json")).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "guide.json must be owner-read/write only");
    }
    let t = fwm_guide::handle(&session, &progress, "guide_turn", &json!({ "goal": "One great empire in the north", "years": 10 })).unwrap().unwrap();
    let seen = h.join().unwrap();
    let (url, headers, body) = &seen[0];
    assert_eq!(url, "/v1/messages");
    let hv = |k: &str| headers.iter().find(|h| h.0 == k).map(|h| h.1.clone());
    assert_eq!(hv("x-api-key").as_deref(), Some("sk-ant-test-123456"));
    assert_eq!(hv("anthropic-version").as_deref(), Some("2023-06-01"));
    assert!(hv("anthropic-beta").is_none() && body.get("fallbacks").is_none(), "fallbacks only on the Claude API itself");
    assert_eq!(body["model"], "claude-opus-5-5");
    assert_eq!(body["system"][0]["cache_control"]["type"], "ephemeral");
    assert_eq!(body["output_config"]["effort"], "medium");
    assert!(body.get("thinking").is_none());
    assert!(body["tools"].as_array().unwrap().iter().any(|t| t["name"] == "war" && t["input_schema"]["type"] == "object"));
    assert!(body["messages"][0]["content"].as_str().unwrap().contains("One great empire in the north"));
    assert_eq!(t["narration"], "The northern kingdom stirs.");
    assert_eq!(t["next_years"], 25.0);
    let acts = t["actions"].as_array().unwrap();
    assert_eq!(acts.len(), 2);
    assert_eq!(acts[0]["ok"], true);
    assert_eq!(acts[1]["ok"], false, "out-of-range factor refused: {}", acts[1]);
    {
        let s = session.lock().unwrap();
        let d = &s.world.edits.overrides.directives;
        assert_eq!(d.len(), 2, "aggression and the chronicle note");
        assert!(d.iter().all(|x| x.by == "guide" && x.stage == "nations" && x.at == 1610));
    }

    // OpenAI-compatible format (Vercel AI Gateway style), arguments as a JSON string,
    // plus set_pace clamping and verifying set_pace does not consume max_actions budget.
    let (addr, h) = serve(vec![json!({
        "model": "anthropic/claude-opus-5-5",
        "choices": [{ "finish_reason": "tool_calls", "message": { "role": "assistant", "content": "A war begins.", "tool_calls": [
            { "id": "c0", "type": "function", "function": { "name": "set_pace", "arguments": "{\"years\": 99999}" } },
            { "id": "c1", "type": "function", "function": { "name": "stability", "arguments": format!("{{\"nation\": {n1}, \"factor\": 0.3, \"years\": 50}}") } },
            { "id": "c2", "type": "function", "function": { "name": "aggression", "arguments": format!("{{\"nation\": {n1}, \"factor\": 2.0, \"years\": 50}}") } }
        ] } }],
        "usage": { "prompt_tokens": 900, "completion_tokens": 40 }
    })]);
    cfg("openai", &format!("{addr}/v1"), "anthropic/claude-opus-5-5", "vck-test-abcdef");
    let t = fwm_guide::handle(&session, &progress, "guide_turn", &json!({ "goal": "Then it breaks apart", "years": 30, "max_actions": 1 })).unwrap().unwrap();
    let seen = h.join().unwrap();
    let (url, headers, body) = &seen[0];
    assert_eq!(url, "/v1/chat/completions");
    assert_eq!(headers.iter().find(|h| h.0 == "authorization").map(|h| h.1.as_str()), Some("Bearer vck-test-abcdef"));
    assert_eq!(body["max_tokens"], 16000);
    assert_eq!(body["messages"][0]["role"], "system");
    assert_eq!(body["tools"][0]["type"], "function");
    assert_eq!(t["next_years"], 1000.0, "set_pace clamped to 5..=1000");
    assert_eq!(t["actions"].as_array().unwrap().len(), 1, "set_pace does not consume max_actions budget, second directive capped");
    assert_eq!(t["actions"][0]["action"], "stability");
    assert_eq!(t["actions"][0]["ok"], true);
    assert_eq!(t["input_tokens"], 900);

    // Advance to the end so sim["done"] == true, then verify guide_turn returns "status".
    let done_turn = fwm_guide::handle(&session, &progress, "guide_turn", &json!({ "goal": "Finish history", "years": 2000 })).unwrap().unwrap();
    assert_eq!(done_turn["done"], true);
    assert!(done_turn.get("status").is_some_and(|s| s["sim"]["done"] == true), "done turn must include status: {done_turn}");

    // The steered run commits and replays.
    let st = api("sim_commit", json!({}));
    assert_eq!(st["replayed"], false);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn openai_200_error_and_malformed_tool_args_fail() {
    let (addr, h) = serve(vec![
        json!({ "error": { "message": "upstream overloaded" } }),
        json!({
            "model": "gpt-test",
            "choices": [{ "finish_reason": "tool_calls", "message": { "role": "assistant", "content": "", "tool_calls": [
                { "id": "c1", "type": "function", "function": { "name": "war", "arguments": "{not valid json" } }
            ] } }]
        }),
    ]);
    let cfg = fwm_guide::GuideConfig {
        provider: "openai".into(),
        base_url: format!("{addr}/v1"),
        model: "gpt-test".into(),
        effort: String::new(),
        api_key: "sk-test".into(),
    };
    let err1 = fwm_guide::complete(&cfg, "sys", "usr", &[], 256).unwrap_err();
    assert!(err1.contains("upstream overloaded"), "got: {err1}");

    let err2 = fwm_guide::complete(&cfg, "sys", "usr", &[], 256).unwrap_err();
    assert!(err2.contains("malformed tool arguments"), "got: {err2}");

    let seen = h.join().unwrap();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0].2["max_tokens"], 256);
}

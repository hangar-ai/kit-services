//! `test/controller.test.js`: the tab on a pretend Hangar: what it reads,
//! when, and what it does when Docker is off, a command is denied or the
//! person asks an agent to stop something.

mod common;

use common::*;
use hangar_kit_sdk::api;
use hangar_kit_sdk::testing;
use kit_services_tab::{AskStatus, Health, LogsStatus, Problem, Show};
use serde_json::json;

fn ports_out() -> String {
    json!({
        "platform": "linux",
        "ports": [
            { "port": 5432, "pid": 1, "mine": true, "name": "postgres", "startedAt": "2026-10-30T08:00:00.000Z", "loopback": true, "portBase": null, "pane": null },
            { "port": 21100, "pid": 4, "mine": true, "name": "node", "startedAt": "2026-10-31T15:48:09.000Z", "loopback": true, "portBase": 21100, "pane": "Gus" },
            { "port": 21104, "pid": 5, "mine": true, "name": "python3", "startedAt": "2026-10-31T16:31:40.000Z", "loopback": false, "portBase": 21100, "pane": "Lou" }
        ],
        "notes": []
    })
    .to_string()
}

fn world() -> World {
    World::new(&ports_out())
}

#[test]
fn it_reads_the_ports_the_containers_and_the_panels_once_in_parallel() {
    let mut world = world();
    let mut tab = started(testing::start());
    assert_eq!(
        tab.methods(),
        ["command.run", "command.run", "panes.list"],
        "three calls go out together, before any is answered"
    );
    serve(&mut tab, &mut world);

    let state = tab.kit();
    assert!(state.loaded);
    let mut commands: Vec<_> = world
        .to("command.run")
        .iter()
        .map(|params| params["name"].as_str().unwrap().to_owned())
        .collect();
    commands.sort();
    assert_eq!(commands, ["docker-list", "ports"]);
    assert_eq!(world.to("panes.list").len(), 1);
    assert_eq!(
        state.ports.items.iter().map(|p| p.port).collect::<Vec<_>>(),
        [5432, 21100, 21104]
    );
    assert_eq!(state.docker.items.len(), 3);
    assert_eq!(state.updated_at, Some(now()));
}

#[test]
fn the_page_changes_once_when_all_three_reads_are_in() {
    let world = world();
    let mut tab = started(testing::start());
    // Two of three answers: still reading, nothing shown yet.
    let calls: Vec<_> = tab.calls().to_vec();
    tab.reply(calls[0].id, world.ports.clone());
    tab.reply(calls[1].id, world.docker.clone());
    assert!(!tab.kit().loaded);
    assert!(tab.kit().refreshing);
    assert!(tab.shows("Reading ports and containers…"));
    tab.reply(calls[2].id, world.panes.clone());
    assert!(tab.kit().loaded);
    assert!(!tab.kit().refreshing);
}

#[test]
fn every_10_s_while_shown_never_while_hidden_and_at_once_when_shown_again() {
    let mut world = world();
    let mut tab = open(&mut world);
    assert_eq!(world.runs("ports"), 1);

    // One read waits 10 s, from the moment the last one ended.
    let waiting: Vec<_> = tab
        .subscriptions()
        .timers
        .iter()
        .filter(|timer| timer.id.starts_with("~after:refresh:"))
        .map(|timer| timer.every_ms)
        .collect();
    assert_eq!(waiting, [10_000]);
    tab.elapse("refresh");
    serve(&mut tab, &mut world);
    assert_eq!(world.runs("ports"), 2);
    assert!(
        tab.waits("refresh"),
        "the next read is armed when this one ends"
    );

    tab.event("tab.visibility", json!({ "visible": false }));
    assert!(!tab.waits("refresh"), "no timer waits behind another tab");
    assert!(tab.calls().is_empty());
    assert_eq!(world.runs("ports"), 2);

    tab.event("tab.visibility", json!({ "visible": true }));
    serve(&mut tab, &mut world);
    assert_eq!(world.runs("ports"), 3, "it reads as soon as it is seen");
    assert!(tab.waits("refresh"));
}

#[test]
fn a_tab_that_starts_behind_another_reads_once_and_then_waits_to_be_seen() {
    let mut world = world();
    let mut start = testing::start();
    start.visible = false;
    let mut tab = open_as(start, &mut world);
    assert!(tab.kit().loaded);
    assert_eq!(world.runs("ports"), 1);
    assert!(!tab.waits("refresh"));
    // Shown again, it reads once more: only if it had read before.
    tab.event("tab.visibility", json!({ "visible": true }));
    serve(&mut tab, &mut world);
    assert_eq!(world.runs("ports"), 2);
}

#[test]
fn a_refresh_going_on_is_not_doubled_by_a_click_or_by_the_poll() {
    let mut world = world();
    let mut tab = open(&mut world);
    tab.press("Refresh");
    assert_eq!(tab.methods(), ["command.run", "command.run", "panes.list"]);
    assert!(
        disabled(&tab, "Reading…"),
        "the button waits while it reads"
    );
    // The poll coming round while it reads starts nothing new.
    tab.elapse("refresh");
    assert_eq!(tab.calls().len(), 3);
    serve(&mut tab, &mut world);
    assert_eq!(world.runs("ports"), 2);
    assert!(has_button(&tab, "Refresh"));
}

#[test]
fn docker_not_installed_containers_say_so_ports_are_fine() {
    let mut world = world();
    world.docker = refused("not_found");
    let tab = open(&mut world);
    assert_eq!(tab.kit().docker.health, Health::Absent);
    assert_eq!(tab.kit().ports.health, Health::Ok);
}

#[test]
fn docker_stopped_as_docker_desktop_says_it_on_windows_and_the_engine_on_linux() {
    for text in [fixture("docker-down.txt"), fixture("docker-down-unix.txt")] {
        let mut world = world();
        world.docker = exited(1, "", &text);
        let tab = open(&mut world);
        assert_eq!(tab.kit().docker.health, Health::Stopped);
        assert_eq!(tab.kit().ports.items.len(), 3);
    }
}

#[test]
fn another_docker_failure_is_an_error_with_dockers_words_not_stopped() {
    let mut world = world();
    world.docker = exited(
        1,
        "",
        "permission denied while trying to connect to the docker API",
    );
    let tab = open(&mut world);
    let Health::Error { problem, detail } = &tab.kit().docker.health else {
        panic!("{:?}", tab.kit().docker.health);
    };
    assert_eq!(*problem, Problem::Failed);
    assert!(detail.contains("permission denied"), "{detail}");
}

#[test]
fn denied_each_command_on_its_own_the_panels_too() {
    let mut world = world();
    world.ports = refused("denied");
    world.docker = refused("denied");
    world.panes = refused("denied");
    let tab = open(&mut world);
    let state = tab.kit();
    assert_eq!(state.ports.health, Health::Denied);
    assert_eq!(state.docker.health, Health::Denied);
    assert_eq!(state.panes, None);
    assert!(state.panes_denied);
}

#[test]
fn panes_that_cannot_be_read_for_another_reason_are_not_denied() {
    let mut world = world();
    world.panes = refused("unavailable");
    let tab = open(&mut world);
    assert_eq!(tab.kit().panes, None);
    assert!(!tab.kit().panes_denied);
}

fn ports_kind(world: &mut World) -> (Health, usize) {
    let tab = open(world);
    (tab.kit().ports.health.clone(), tab.kit().ports.items.len())
}

fn problem(health: &Health) -> Option<Problem> {
    match health {
        Health::Error { problem, .. } => Some(*problem),
        _ => None,
    }
}

#[test]
fn ports_no_node_no_listing_tool_an_answer_nobody_can_read() {
    let mut no_node = world();
    no_node.ports = refused("not_found");
    let (health, _) = ports_kind(&mut no_node);
    assert_eq!(problem(&health), Some(Problem::NoNode));

    let mut no_tool = world();
    no_tool.ports = exited(2, r#"{"error":"no-tool"}"#, "");
    assert_eq!(problem(&ports_kind(&mut no_tool).0), Some(Problem::NoTool));

    let mut no_tool_ok = world();
    no_tool_ok.ports = ran(r#"{"error":"no-tool"}"#);
    assert_eq!(
        problem(&ports_kind(&mut no_tool_ok).0),
        Some(Problem::NoTool)
    );

    let mut garbage = world();
    garbage.ports = ran("hello");
    assert_eq!(problem(&ports_kind(&mut garbage).0), Some(Problem::Failed));

    let mut slow = world();
    slow.ports = timed_out();
    assert_eq!(problem(&ports_kind(&mut slow).0), Some(Problem::Timeout));

    // The script's own limit, as it exits.
    let mut slow_tool = world();
    slow_tool.ports = exited(2, "{\"error\":\"timeout\"}\n", "");
    let (health, items) = ports_kind(&mut slow_tool);
    assert_eq!((problem(&health), items), (Some(Problem::Timeout), 0));

    let mut slow_tool_ok = world();
    slow_tool_ok.ports = ran(r#"{"error":"timeout"}"#);
    assert_eq!(
        problem(&ports_kind(&mut slow_tool_ok).0),
        Some(Problem::Timeout)
    );
}

#[test]
fn nothing_listening_and_no_container_are_answers_not_errors() {
    let mut world = world();
    world.ports = ran(r#"{"platform":"linux","ports":[],"notes":[]}"#);
    world.docker = ran("");
    let tab = open(&mut world);
    let state = tab.kit();
    assert_eq!(
        (
            &state.ports.health,
            state.ports.items.len(),
            &state.docker.health,
            state.docker.items.len()
        ),
        (&Health::Ok, 0, &Health::Ok, 0)
    );
}

#[test]
fn logs_asked_by_the_containers_id_only_stdout_and_stderr_both_and_a_second_click_closes_them() {
    let mut world = world();
    world.logs = exited(0, "out line\n", "Error: boom\n");
    let mut tab = open(&mut world);
    tab.press("Logs");
    serve(&mut tab, &mut world);
    assert_eq!(
        world.to("command.run").last().unwrap(),
        &&json!({ "name": "docker-logs", "args": { "id": "3f1c9a7b2d44" } })
    );
    let logs = tab.kit().logs.clone().unwrap();
    assert_eq!(logs.id, "3f1c9a7b2d44");
    assert_eq!(logs.status, LogsStatus::Ok);
    assert_eq!(logs.lines, ["out line", "Error: boom"]);
    assert_eq!(logs.detail, "");
    tab.press("Hide logs");
    assert!(tab.kit().logs.is_none());
}

#[test]
fn logs_an_id_that_is_not_an_id_is_never_sent() {
    // `docker ps` is what names the ids, and an id that is not 12 to 64
    // lowercase hex digits is no container: it has no row, so no button.
    let mut world = world();
    let evil = ["--rm", "../x", "3f1c9a7b2d44 --all", "", "3F1C9A7B2D44"];
    let mut lines: Vec<String> = evil
        .iter()
        .map(|id| json!({ "ID": id, "Names": "evil", "Image": "x", "State": "running", "Status": "Up", "Ports": "" }).to_string())
        .collect();
    lines.push(json!({ "ID": "3f1c9a7b2d44", "Names": "fine", "Image": "x", "State": "running", "Status": "Up", "Ports": "" }).to_string());
    world.docker = ran(&lines.join("\n"));
    let mut tab = open(&mut world);
    assert_eq!(tab.kit().docker.items.len(), 1);
    assert_eq!(container_items(&tab).len(), 1);
    tab.press("Logs");
    serve(&mut tab, &mut world);
    let asked: Vec<_> = world
        .to("command.run")
        .into_iter()
        .filter(|params| params["name"] == "docker-logs")
        .collect();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0]["args"]["id"], "3f1c9a7b2d44");
}

#[test]
fn logs_docker_failing_says_why() {
    let mut world = world();
    world.logs = exited(1, "", "Error response from daemon: No such container");
    let mut tab = open(&mut world);
    tab.press("Logs");
    serve(&mut tab, &mut world);
    let logs = tab.kit().logs.clone().unwrap();
    assert_eq!(logs.status, LogsStatus::Failed);
    assert!(logs.detail.contains("No such container"), "{}", logs.detail);
}

#[test]
fn logs_of_a_container_no_longer_listed_close_on_the_next_refresh() {
    let mut world = world();
    let mut tab = open(&mut world);
    tab.press("Logs");
    serve(&mut tab, &mut world);
    assert!(tab.kit().logs.is_some());
    let list = fixture("docker-ps.jsonl");
    let without: Vec<_> = list
        .lines()
        .filter(|line| !line.contains("3f1c9a7b2d44"))
        .collect();
    world.docker = ran(&without.join("\n"));
    tab.press("Refresh");
    serve(&mut tab, &mut world);
    assert!(tab.kit().logs.is_none());
}

#[test]
fn asking_an_agent_the_worker_who_owns_the_port_is_preselected_and_the_brief_is_what_is_sent() {
    let mut world = world();
    let mut tab = open(&mut world);
    let row = port_items(&tab)
        .into_iter()
        .find(|row| row.0.title.starts_with("21100"))
        .unwrap();
    press_in(&mut tab, &row, "Ask an agent to stop this");
    assert_eq!(tab.kit().ask.as_ref().unwrap().pane, "Gus");
    tab.press("Send");
    serve(&mut tab, &mut world);
    let sent = world.to("panes.send");
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["pane"], "Gus");
    let text = sent[0]["text"].as_str().unwrap();
    assert!(
        text.contains("port 21100: node, pid 4, started today at 3:48 PM"),
        "{text}"
    );
    assert!(text.contains("started in the pane of Gus"), "{text}");
    assert_eq!(tab.kit().ask.as_ref().unwrap().status, AskStatus::Sent);
}

#[test]
fn asking_about_a_leftover_whose_worker_is_gone_nobody_is_preselected_the_person_picks() {
    let mut world = world();
    let mut tab = open(&mut world);
    let row = port_items(&tab)
        .into_iter()
        .find(|row| row.0.title.starts_with("21104"))
        .unwrap();
    assert!(row.has_text("whose pane is closed"), "the worker is gone");
    press_in(&mut tab, &row, "Ask an agent to stop this");
    assert_eq!(tab.kit().ask.as_ref().unwrap().pane, "");
    // Nothing is sent without a pick: Send waits.
    assert!(disabled(&tab, "Send"));
    tab.choose("Send to", "Jack");
    tab.press("Send");
    serve(&mut tab, &mut world);
    assert_eq!(world.to("panes.send")[0]["pane"], "Jack");
}

#[test]
fn asking_again_closes_the_question_a_port_that_vanished_takes_an_unsent_question_with_it() {
    let mut world = world();
    let mut tab = open(&mut world);
    let first = |tab: &hangar_kit_sdk::testing::Harness<kit_services_tab::Services>| {
        port_items(tab).into_iter().next().unwrap()
    };
    let row = first(&tab);
    press_in(&mut tab, &row, "Ask an agent to stop this");
    assert!(tab.kit().ask.is_some());
    let row = first(&tab);
    press_in(&mut tab, &row, "Ask an agent to stop this");
    assert!(tab.kit().ask.is_none());
    let row = first(&tab);
    press_in(&mut tab, &row, "Ask an agent to stop this");
    world.ports = ran(&json!({ "platform": "linux", "ports": [], "notes": [] }).to_string());
    tab.press("Refresh");
    serve(&mut tab, &mut world);
    assert!(tab.kit().ask.is_none());
}

#[test]
fn a_sent_question_stays_when_its_port_vanishes() {
    let mut world = world();
    let mut tab = open(&mut world);
    let row = port_items(&tab)
        .into_iter()
        .find(|row| row.0.title.starts_with("21100"))
        .unwrap();
    press_in(&mut tab, &row, "Ask an agent to stop this");
    tab.press("Send");
    serve(&mut tab, &mut world);
    world.ports = ran(&json!({ "platform": "linux", "ports": [], "notes": [] }).to_string());
    tab.press("Refresh");
    serve(&mut tab, &mut world);
    assert_eq!(
        tab.kit().ask.as_ref().map(|ask| ask.status),
        Some(AskStatus::Sent)
    );
}

#[test]
fn a_refused_send_says_so_denied_included_the_kit_itself_calls_nothing_that_stops_anything() {
    let mut world = world();
    world.send_error = Some("denied");
    let mut tab = open(&mut world);
    let row = container_items(&tab).into_iter().next().unwrap();
    press_in(&mut tab, &row, "Ask an agent to stop this");
    tab.choose("Send to", "Jack");
    tab.press("Send");
    serve(&mut tab, &mut world);
    let ask = tab.kit().ask.as_ref().unwrap();
    assert_eq!(
        (ask.status, ask.detail.as_str()),
        (AskStatus::Failed, "denied")
    );

    let mut used: Vec<String> = world
        .calls
        .iter()
        .map(|(method, params)| match params["name"].as_str() {
            Some(name) if method == "command.run" => name.to_owned(),
            _ => method.clone(),
        })
        .collect();
    used.sort();
    used.dedup();
    assert_eq!(used, ["docker-list", "panes.list", "panes.send", "ports"]);
}

#[test]
fn panes_changed_reads_the_panels_again_nothing_else() {
    let mut world = world();
    let mut tab = open(&mut world);
    world.panes = Ok(
        json!([{ "name": "Ava", "agent": "claude", "role": "worker", "state": null, "canSend": true }]),
    );
    tab.event("panes.changed", json!({}));
    assert_eq!(tab.methods(), ["panes.list"]);
    serve(&mut tab, &mut world);
    let names: Vec<_> = tab
        .kit()
        .panes
        .as_ref()
        .unwrap()
        .iter()
        .map(|p| p.name.clone())
        .collect();
    assert_eq!(names, ["Ava"]);
    assert_eq!(world.to("command.run").len(), 2, "no command ran again");
}

#[test]
fn the_filter_is_remembered_and_only_applies_while_there_is_something_to_filter_by() {
    let mut world = world();
    let mut tab = open(&mut world);
    tab.press("Worker ports");
    assert_eq!(tab.kit().show, Show::Blocks);
    // Nothing in a worker's block any more: everything is shown, not nothing.
    world.ports = ran(
        r#"{"platform":"linux","ports":[{"port":5432,"pid":1,"mine":true,"name":"postgres"}],"notes":[]}"#,
    );
    tab.press("Refresh");
    serve(&mut tab, &mut world);
    assert_eq!(port_numbers(&tab), ["5432"]);
}

#[test]
fn the_tabs_language_follows_hangars() {
    let mut world = world();
    let mut tab = open(&mut world);
    assert!(tab.shows("Refresh"));
    tab.event("theme", json!({ "scheme": "dark", "locale": "fr" }));
    assert!(tab.shows("Actualiser"));
    tab.event("theme", json!({ "scheme": "dark", "locale": "en" }));
    assert!(tab.shows("Refresh"));
}

#[test]
fn the_tabs_calls_are_only_those_the_manifest_grants() {
    // The page's code called `command.run`, `panes.list` and `panes.send`, and
    // ran `ports`, `docker-list` and `docker-logs`: every call of a session is
    // held to the tab's permissions as Hangar holds it.
    let granted = granted();
    let mut world = world();
    let mut tab = open(&mut world);
    tab.press("Logs");
    serve(&mut tab, &mut world);
    press_nth(&mut tab, "Copy", 0);
    serve(&mut tab, &mut world);
    let row = port_items(&tab)
        .into_iter()
        .find(|row| row.0.title.starts_with("21100"))
        .unwrap();
    press_in(&mut tab, &row, "Ask an agent to stop this");
    tab.press("Send");
    serve(&mut tab, &mut world);

    for (method, params) in &world.calls {
        assert!(
            api::may_call(method, params, &granted),
            "{method} {params} is not granted"
        );
    }
    let mut methods: Vec<_> = world
        .calls
        .iter()
        .map(|(method, _)| method.as_str())
        .collect();
    methods.sort();
    methods.dedup();
    assert_eq!(
        methods,
        [
            "command.run",
            "panes.list",
            "panes.send",
            "ui.copy",
            "ui.toast"
        ]
    );
    let mut commands: Vec<_> = world
        .to("command.run")
        .iter()
        .map(|params| params["name"].as_str().unwrap().to_owned())
        .collect();
    commands.sort();
    commands.dedup();
    assert_eq!(commands, ["docker-list", "docker-logs", "ports"]);
    assert!(
        granted.contains(&"panes.read".to_owned()) && granted.contains(&"panes.send".to_owned())
    );
    for name in &commands {
        assert!(granted.contains(&format!("command:{name}")), "{name}");
    }
}

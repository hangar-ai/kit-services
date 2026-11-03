//! `test/logic.test.js`: what the tab knows apart from Hangar and the page,
//! on the tools' recorded output.

mod common;

use common::fixture;
use hangar_kit_sdk::api::Pane;
use hangar_kit_sdk::prelude::Status;
use kit_services_tab::logic::{
    self, Container, DockerFault, Fault, Owner, Port, PortRow, PortsError, Published, Target,
};
use regex::Regex;
use serde_json::json;

fn t(locale: &str) -> hangar_kit_sdk::i18n::Strings {
    logic::strings(locale)
}

fn pane(name: &str) -> Pane {
    serde_json::from_value(
        json!({ "name": name, "agent": "claude", "role": null, "state": null, "canSend": true }),
    )
    .unwrap()
}

fn port(port: u32) -> Port {
    Port {
        port,
        pid: None,
        name: None,
        started_at: None,
        loopback: false,
        mine: false,
        port_base: None,
        pane: None,
    }
}

#[test]
fn both_languages_have_the_same_keys_and_french_speaks_as_hangar_does() {
    // The glossary of Hangar's own strings: tu, panneau, workspace, Réglages.
    let keys = |strings: &[(&str, &str)]| {
        let mut keys: Vec<String> = strings.iter().map(|(key, _)| (*key).to_owned()).collect();
        keys.sort();
        keys
    };
    assert_eq!(keys(logic::FR), keys(logic::EN));

    let vous = Regex::new(r"(?i)\b(?:vous|votre|vos)\b").unwrap();
    let imperative = Regex::new(r"(?i)[a-zéè]ez\b").unwrap();
    let workspace = Regex::new(r"(?i)espace de travail").unwrap();
    let pane_word = Regex::new(r"(?i)\bpane\b|\bpanel\b").unwrap();
    let variable = Regex::new(r"\{\w+\}").unwrap();
    for (key, text) in logic::FR {
        assert!(!vous.is_match(text), "{key}: « tu »");
        // « Envoyez : » would be a vous too, unless it only introduces a colon.
        for found in imperative.find_iter(text) {
            assert!(
                text[found.end()..].trim_start().starts_with(':'),
                "{key}: « tu » ({})",
                found.as_str()
            );
        }
        assert!(!workspace.is_match(text), "{key}: « workspace »");
        assert!(!text.contains("Paramètres"), "{key}: « Réglages »");
        assert!(
            !pane_word.is_match(&variable.replace_all(text, "")),
            "{key}: « panneau »"
        );
        assert!(
            !text.contains('’'),
            "{key}: the apostrophe Hangar's strings use"
        );
    }
    let fr = |key: &str| logic::FR.iter().find(|(name, _)| *name == key).unwrap().1;
    assert!(
        Regex::new("Réglages › Kits")
            .unwrap()
            .is_match(fr("deniedHint"))
    );
    assert_eq!(fr("ask"), "Demander à un agent d'arrêter ça");
    let accents = Regex::new(r"(?i)[àâçéèêëîïôûù]").unwrap();
    for (key, text) in logic::EN {
        assert!(!accents.is_match(text), "{key}: English");
    }
}

#[test]
fn a_port_belongs_to_the_worker_block_it_falls_in_21000_21020() {
    assert_eq!(logic::block_of(21000), Some(21000));
    assert_eq!(logic::block_of(21019), Some(21000));
    assert_eq!(logic::block_of(21020), Some(21020));
    assert_eq!(logic::block_of(21100), Some(21100));
    assert_eq!(logic::block_of(26999), Some(26980));
    for outside in [20999, 27000, 3000, 80, 65535, 0] {
        assert_eq!(logic::block_of(outside), None, "{outside}");
    }
}

#[test]
fn ports_js_output_becomes_rows_sorted_with_nothing_but_what_the_tab_uses() {
    let stdout = json!({
        "platform": "linux",
        "ports": [
            { "port": 8080, "pid": 5, "name": "b", "startedAt": "2026-10-31T10:00:00.000Z", "loopback": false, "portBase": null, "pane": null, "extra": "dropped" },
            { "port": 21100, "pid": 4, "name": "node", "startedAt": "not a date", "loopback": true, "portBase": 21100, "pane": "Gus" },
            { "port": "x" }
        ],
        "notes": ["some-hidden", 3]
    })
    .to_string();
    let parsed = logic::parse_ports(&stdout).unwrap();
    assert_eq!(
        parsed.ports.iter().map(|row| row.port).collect::<Vec<_>>(),
        [8080, 21100]
    );
    assert_eq!(parsed.ports[1].started_at, None);
    assert!(parsed.ports[0].started_at.is_some());
    assert_eq!(parsed.notes, ["some-hidden"]);
    assert_eq!(
        logic::parse_ports(r#"{"error":"no-tool"}"#),
        Err(PortsError::Tool("no-tool".into()))
    );
    assert_eq!(logic::parse_ports("not json"), Err(PortsError::Unreadable));
    assert_eq!(
        logic::parse_ports(r#"{"ports":3}"#),
        Err(PortsError::Unreadable)
    );
}

#[test]
fn docker_ps_one_object_a_line_state_exit_code_published_ports_broken_ids_are_dropped() {
    let containers = logic::parse_docker(&fixture("docker-ps.jsonl")).unwrap();
    assert_eq!(
        containers
            .iter()
            .map(|c| (c.name.as_str(), c.state.as_str(), c.exit_code))
            .collect::<Vec<_>>(),
        [
            ("shop-db-1", "running", None),
            ("shop-api", "exited", Some(1)),
            ("cache", "created", None)
        ]
    );
    let db = &containers[0];
    assert_eq!(db.image, "postgres:16");
    assert_eq!(
        db.ports,
        [Published {
            host: 5432,
            container: 5432,
            proto: "tcp".into()
        }],
        "the IPv6 twin counts once"
    );
    assert_eq!(
        containers[1].ports,
        [Published {
            host: 21104,
            container: 3000,
            proto: "tcp".into()
        }]
    );
    assert!(containers[2].ports.is_empty());
    assert_eq!(
        logic::parse_docker(""),
        Some(Vec::new()),
        "docker answers, with no container"
    );
    assert_eq!(logic::parse_docker("{not json"), None);
    let array = json!([{ "ID": "0123456789ab", "Names": "a,b", "State": "running" }, { "ID": "--rm", "Names": "evil" }])
        .to_string();
    assert_eq!(
        logic::parse_docker(&array)
            .unwrap()
            .iter()
            .map(|c| (c.id.as_str(), c.name.as_str()))
            .collect::<Vec<_>>(),
        [("0123456789ab", "a")],
        "an id that is not hex is no container"
    );
}

#[test]
fn published_ports_ranges_protocols_no_host_port() {
    let hosts = |text: &str| -> Vec<(u32, u32)> {
        logic::published_ports(text)
            .iter()
            .map(|published| (published.host, published.container))
            .collect()
    };
    assert_eq!(
        hosts("0.0.0.0:21100-21102->3000-3002/tcp"),
        [(21100, 3000), (21101, 3001), (21102, 3002)]
    );
    assert_eq!(
        logic::published_ports("3000/tcp, 127.0.0.1:53->53/udp"),
        [Published {
            host: 53,
            container: 53,
            proto: "udp".into()
        }]
    );
    assert!(logic::published_ports("").is_empty());
    assert_eq!(
        logic::published_ports("0.0.0.0:1-60000->1/tcp").len(),
        100,
        "a wild range is cut"
    );
}

#[test]
fn a_containers_state_is_a_glyph_and_a_word() {
    let of = |state: &str, exit_code: Option<i32>| {
        logic::container_state(&Container {
            id: "0123456789ab".into(),
            name: "x".into(),
            image: String::new(),
            state: state.into(),
            status: String::new(),
            exit_code,
            ports: Vec::new(),
            ports_text: String::new(),
        })
    };
    let running = of("running", None);
    assert_eq!(
        (running.status, running.key),
        (Status::Success, "state_running")
    );
    assert_eq!(of("exited", Some(1)).status, Status::Failure);
    assert_eq!(of("exited", Some(1)).code, Some(1));
    assert_eq!(of("exited", Some(0)).status, Status::Neutral);
    assert_eq!(of("created", None).status, Status::Waiting);
    assert_eq!(of("restarting", None).status, Status::Running);
    assert_eq!(of("weird", None).key, "state_other");
}

#[test]
fn whose_a_port_is_the_pane_that_started_it_else_the_block_it_sits_in_else_nobody() {
    let panes = [pane("Gus"), pane("Jack")];
    let in_pane = |number: u32, pane_name: &str| Port {
        pane: Some(pane_name.into()),
        port_base: Some(21100),
        ..port(number)
    };
    assert_eq!(
        logic::owner_of(&in_pane(21100, "gus"), Some(&panes)),
        Some(Owner::Worker {
            name: "Gus".into(),
            gone: false,
            base: Some(21100)
        })
    );
    assert_eq!(
        logic::owner_of(&in_pane(21100, "Lou"), Some(&panes)),
        Some(Owner::Worker {
            name: "Lou".into(),
            gone: true,
            base: Some(21100)
        }),
        "its panel is closed: a leftover"
    );
    assert!(
        matches!(
            logic::owner_of(&in_pane(21100, "Lou"), None),
            Some(Owner::Worker { gone: false, .. })
        ),
        "without the panes nothing is claimed gone"
    );
    assert_eq!(
        logic::owner_of(&port(21107), Some(&panes)),
        Some(Owner::Block { base: 21100 })
    );
    assert_eq!(logic::owner_of(&port(5432), Some(&panes)), None);
}

#[test]
fn a_container_publishing_a_port_is_shown_on_that_ports_row() {
    let containers = logic::parse_docker(&fixture("docker-ps.jsonl")).unwrap();
    let ports = [port(5432), port(21104), port(9)];
    let rows = logic::port_rows(&ports, Some(&[]), &containers);
    assert_eq!(
        rows.iter()
            .map(|row| row.container.as_deref())
            .collect::<Vec<_>>(),
        [Some("shop-db-1"), Some("shop-api"), None]
    );
    assert!(matches!(rows[1].owner, Some(Owner::Block { .. })));
}

#[test]
fn the_brief_to_an_agent_says_what_to_check_what_to_do_and_what_to_report() {
    let since = |_: i64| "2 h ago".to_owned();
    let row = PortRow {
        port: Port {
            pid: Some(9052),
            name: Some("python.exe".into()),
            started_at: Some(1),
            ..port(21104)
        },
        owner: Some(Owner::Worker {
            name: "Lou".into(),
            gone: false,
            base: Some(21100),
        }),
        container: None,
    };
    let brief = logic::brief_for(&Target::Port(row.clone()), &t("en"), &since);
    assert!(
        brief.contains("port 21104: python.exe, pid 9052, started 2 h ago"),
        "{brief}"
    );
    assert!(brief.contains("started in the pane of Lou"));
    assert!(brief.contains("Check that it is yours"));
    assert!(brief.contains("If it is not yours, leave it"));

    let free = PortRow {
        port: port(21104),
        owner: Some(Owner::Block { base: 21100 }),
        container: None,
    };
    let free = logic::brief_for(&Target::Port(free), &t("en"), &since);
    assert!(free.contains("Unknown program, pid ?."), "{free}");
    assert!(free.contains("worker port block 21100–21119"));

    let container = Container {
        id: "a81e0c55d9f3aaaa".into(),
        name: "shop-api".into(),
        image: "shop-api:dev".into(),
        state: "exited".into(),
        status: "Exited (1) 12 minutes ago".into(),
        exit_code: Some(1),
        ports: Vec::new(),
        ports_text: String::new(),
    };
    let container = logic::brief_for(&Target::Container(container), &t("en"), &since);
    assert!(
        container.contains("Docker container shop-api (id a81e0c55d9f3, image shop-api:dev, Exited (1) 12 minutes ago)"),
        "{container}"
    );
    let french = logic::brief_for(&Target::Port(row), &t("fr"), &since);
    assert!(
        french.contains("Arrête ce qui écoute sur le port 21104"),
        "{french}"
    );
}

#[test]
fn the_brief_is_plain_text_for_an_agent_not_typography() {
    let row = PortRow {
        port: Port {
            pid: Some(1),
            name: Some("node".into()),
            ..port(21104)
        },
        owner: None,
        container: None,
    };
    let french = logic::brief_for(&Target::Port(row), &t("fr"), &|_| {
        "il y a 5\u{a0}min".to_owned()
    });
    assert!(
        !french.contains('\u{a0}') && !french.contains('\u{202f}'),
        "{french:?}"
    );
    assert!(french.contains("port 21104 : node"), "{french}");
}

#[test]
fn what_a_refusal_means_and_whether_docker_is_off_or_just_failing() {
    assert_eq!(logic::classify("denied"), Fault::Denied);
    assert_eq!(logic::classify("not_found"), Fault::Missing);
    assert_eq!(logic::classify("busy"), Fault::Busy);
    // The 30 s limit of kit.js is gone: only a command's own `timedOut` is a timeout.
    for code in ["", "failed", "unavailable", "rate_limited", "bad_args"] {
        assert_eq!(logic::classify(code), Fault::Failed, "{code}");
    }
    assert_eq!(
        logic::docker_fault(&fixture("docker-down.txt")),
        DockerFault::Stopped,
        "recorded on Windows with Docker Desktop off"
    );
    assert_eq!(
        logic::docker_fault(&fixture("docker-down-unix.txt")),
        DockerFault::Stopped
    );
    assert_eq!(
        logic::docker_fault("permission denied while trying to connect to the docker API"),
        DockerFault::Failed
    );
}

#[test]
fn a_log_ansi_gone_the_last_200_lines_no_empty_tail() {
    assert_eq!(
        logic::log_lines("a\n\u{1b}[31mred\u{1b}[0m\n\n"),
        ["a", "red"]
    );
    let many = (0..500)
        .map(|i| format!("line {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    let lines = logic::log_lines(&many);
    assert_eq!(lines.len(), 200);
    assert_eq!(lines[0], "line 300");
    assert!(logic::log_lines("").is_empty());
    assert_eq!(
        logic::log_lines("a\r\nb\r\n"),
        ["a", "b"],
        "Windows line ends"
    );
}

#[test]
fn colours_and_window_titles_are_stripped_and_nothing_else() {
    assert_eq!(logic::strip_ansi("\u{1b}[1;31mfail\u{1b}[0m ok"), "fail ok");
    assert_eq!(logic::strip_ansi("\u{1b}]0;title\u{7}after"), "after");
    assert_eq!(logic::strip_ansi("\u{1b}[?25l\u{1b}[2K\u{1b}[1Gx"), "x");
    assert_eq!(logic::strip_ansi("plain — é"), "plain — é");
    assert_eq!(
        logic::strip_ansi("a \u{1b} b"),
        "a \u{1b} b",
        "a lone escape is not a sequence"
    );
}

#[test]
fn the_strict_id_pattern() {
    for good in ["3f1c9a7b2d44", &"a".repeat(64)] {
        assert!(logic::is_container_id(good), "{good}");
    }
    for bad in [
        "",
        "abc",
        "-3f1c9a7b2d44",
        "3F1C9A7B2D44",
        "3f1c9a7b2d4g",
        "3f1c9a7b2d44 --rm",
        &"a".repeat(65),
        "3f1c9a7b2d44\n",
    ] {
        assert!(!logic::is_container_id(bad), "{bad:?}");
    }
}

#[test]
fn a_program_is_the_persons_on_a_clear_yes_only() {
    let entries: Vec<_> = [
        json!(true),
        json!(false),
        json!("true"),
        json!(1),
        json!(null),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, mine)| json!({ "port": 3000 + index, "pid": index + 1, "mine": mine }))
    .chain([json!({ "port": 3005, "pid": 6 })])
    .collect();
    let parsed = logic::parse_ports(&json!({ "ports": entries }).to_string()).unwrap();
    assert_eq!(
        parsed
            .ports
            .iter()
            .map(|port| port.mine)
            .collect::<Vec<_>>(),
        [true, false, false, false, false, false]
    );
}

#[test]
fn a_log_line_reads_as_an_error_as_hangar_uis_is_error_line_says() {
    // kit-ui.js's isErrorLine: the words of an error, in English and French,
    // not those of "0 errors" or `handles_failure_mode`.
    for error in [
        "Error: connect ECONNREFUSED 172.17.0.2:5432",
        "fatal: database unreachable, exiting",
        "thread 'main' panicked at src/main.rs:3",
        "2 tests failed",
        "Traceback ... Exception in thread",
        "erreur : connexion refusée",
        "Échec de la requête",
        "build failure.",
        "errors: 3",
        "[ERROR] boom",
    ] {
        assert!(logic::is_error_line(error), "{error}");
    }
    for fine in [
        "listening on :3000",
        "warn: slow query (412 ms)",
        "0 errors, 2 warnings",
        "no errors found",
        "errors: 0",
        "failed=0",
        "handles_failure_mode",
        "errors-kit started",
        "see src/errors/mod.rs",
        "sans erreur",
        "aucune erreur détectée",
        "terrorist",
        "",
    ] {
        assert!(!logic::is_error_line(fine), "{fine}");
    }
}

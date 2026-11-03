//! `test/view.test.js`: what the tab draws for each state, both languages,
//! every state designed.

mod common;

use common::*;
use hangar_kit_sdk::testing::{self, StartExt};
use hangar_kit_sdk::ui::{EmptyKind, Icon, Status};
use serde_json::json;

fn ports_out() -> String {
    json!({
        "platform": "linux",
        "ports": [
            { "port": 5432, "pid": 1, "mine": true, "name": "postgres", "startedAt": "2026-10-30T08:00:00.000Z", "loopback": true, "portBase": null, "pane": null },
            { "port": 21100, "pid": 4, "mine": true, "name": "node", "startedAt": "2026-10-31T15:48:09.000Z", "loopback": true, "portBase": 21100, "pane": "Gus" },
            { "port": 21104, "pid": 5, "mine": true, "name": "<img src=x onerror=alert(1)>", "startedAt": null, "loopback": false, "portBase": 21100, "pane": "Lou" },
            { "port": 21140, "pid": null, "mine": false, "name": null, "startedAt": null, "loopback": false, "portBase": null, "pane": null }
        ],
        "notes": ["some-hidden"]
    })
    .to_string()
}

fn world() -> World {
    let mut world = World::new(&ports_out());
    // Jack and Gus can be reached; the shell cannot.
    world.panes = Ok(panes());
    world
}

const ASK: &str = "Ask an agent to stop this";

type Tab = hangar_kit_sdk::testing::Harness<kit_services_tab::Services>;

/// Opens the question under the row of this port.
fn ask_about(tab: &mut Tab, port: &str) {
    let row = row_of(tab, port);
    press_in(tab, &row, ASK);
}

fn row_of(tab: &Tab, port: &str) -> Item {
    port_items(tab)
        .into_iter()
        .find(|row| row.0.title.starts_with(port))
        .unwrap_or_else(|| panic!("no row for port {port}"))
}

#[test]
fn while_it_reads_for_the_first_time_one_line_saying_so_no_list_of_nothing() {
    let tab = started(testing::start());
    assert!(tab.shows("Reading ports and containers…"));
    assert!(items(&tab).is_empty());
    assert_eq!(
        states(&tab)
            .iter()
            .map(|(kind, _)| *kind)
            .collect::<Vec<_>>(),
        [EmptyKind::Loading]
    );
}

#[test]
fn ports_are_rows_the_number_first_the_program_pid_age_and_reach_and_whose_block_it_is() {
    let mut world = world();
    let tab = open(&mut world);
    assert_eq!(port_numbers(&tab), ["5432", "21100", "21104", "21140"]);

    let node = row_of(&tab, "21100");
    assert_eq!(node.0.title, "21100  node");
    // 74 minutes ago on the same day: the day's clock.
    let sub = node
        .0
        .sub
        .as_deref()
        .unwrap()
        .replace(['\u{a0}', '\u{202f}'], " ");
    assert!(
        sub.contains("pid 4 · started today at 3:48 PM · local only"),
        "{sub}"
    );
    assert!(node.has_text("Port block of Gus"), "the worker is named");

    let lou = row_of(&tab, "21104");
    assert!(
        lou.has_text("Port block of Lou, whose pane is closed"),
        "a leftover says its worker is gone"
    );
    assert!(lou.has_text("reachable from the network"));

    let free = row_of(&tab, "21140");
    assert!(free.has_text("Worker port block 21140–21159"));
    assert!(free.has_text("Unknown program"));

    let postgres = row_of(&tab, "5432");
    assert!(
        !postgres.text().to_lowercase().contains("block"),
        "a port outside the pool is nobody's"
    );
    assert!(tab.shows("Some programs belong to another account"));
    assert_eq!(section(&tab, "Ports").unwrap().1.as_deref(), Some("4"));
}

#[test]
fn what_a_program_is_called_is_text_never_markup() {
    // A native page has no markup to inject into: the name is a title, as
    // typed, and the whole of it is what the tooltip says.
    let mut world = world();
    let tab = open(&mut world);
    let row = row_of(&tab, "21104");
    assert_eq!(row.0.title, "21104  <img src=x onerror=alert(1)>");
    assert_eq!(
        row.0.tooltip.as_deref(),
        Some("21104 <img src=x onerror=alert(1)>")
    );
}

#[test]
fn a_filter_shows_the_worker_ports_alone_and_says_how_many() {
    let mut world = world();
    let mut tab = open(&mut world);
    assert_eq!(
        filters(&tab),
        [
            ("All".to_owned(), Some("4".to_owned()), true),
            ("Worker ports".to_owned(), Some("3".to_owned()), false)
        ]
    );
    tab.press("Worker ports");
    assert_eq!(port_numbers(&tab), ["21100", "21104", "21140"]);
    assert!(filters(&tab)[1].2, "Worker ports is pressed");
    assert!(!filters(&tab)[0].2);
    tab.press("All");
    assert_eq!(port_numbers(&tab).len(), 4);
}

#[test]
fn containers_state_as_a_glyph_and_a_word_image_ports_and_the_port_block_they_publish_into() {
    let mut world = world();
    let tab = open(&mut world);
    let rows = container_items(&tab);
    assert_eq!(
        rows.iter()
            .map(|row| row.0.title.as_str())
            .collect::<Vec<_>>(),
        ["shop-db-1", "shop-api", "cache"]
    );
    assert!(
        rows[0]
            .0
            .sub
            .as_deref()
            .unwrap()
            .contains("Running · postgres:16")
    );
    let glyph = rows[0].glyph().unwrap();
    assert_eq!(
        (glyph.status, glyph.label.as_str()),
        (Status::Success, "Running")
    );
    assert!(rows[0].has_text("0.0.0.0:5432->5432/tcp"));
    assert!(rows[1].0.sub.as_deref().unwrap().contains("Stopped (1)"));
    assert_eq!(rows[1].glyph().unwrap().status, Status::Failure);
    assert!(rows[1].has_text("Worker port block 21100–21119"));
    assert!(rows[2].has_text("No published port"));
    assert!(
        row_of(&tab, "5432").has_text("Container shop-db-1"),
        "the port row of a published port names the container"
    );
    assert_eq!(section(&tab, "Containers").unwrap().1.as_deref(), Some("3"));
}

#[test]
fn logs_open_in_a_log_block_on_their_first_error_from_the_containers_id() {
    let mut world = world();
    let mut tab = open(&mut world);
    tab.press("Logs");
    // While they are read, the row says so.
    assert!(tab.shows("Reading the logs…"));
    serve(&mut tab, &mut world);

    assert_eq!(
        world.to("command.run").last().unwrap(),
        &&json!({ "name": "docker-logs", "args": { "id": "3f1c9a7b2d44" } })
    );
    let open = logs(&tab);
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].id, "logs-3f1c9a7b2d44");
    assert_eq!(open[0].lines.len(), 6);
    assert!(open[0].lines[3].text.starts_with("2026"));
    assert!(
        open[0].lines[3]
            .text
            .contains("Error: connect ECONNREFUSED")
    );
    assert_eq!(open[0].lines[3].number, Some(4));
    assert!(tab.shows("The last 200 lines."));

    // It opens on the first error, and says how many there are.
    use hangar_kit_sdk::ui::LineKind;
    use hangar_kit_sdk::ui::abi::Operation;
    let errors: Vec<_> = open[0]
        .lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.kind == LineKind::Error)
        .map(|(i, _)| i)
        .collect();
    assert_eq!(errors, [3, 5]);
    assert!(open[0].lines[3].current && !open[0].lines[5].current);
    assert_eq!(
        tab.take_operations().last(),
        Some(&Operation::ScrollTo {
            id: "logs-3f1c9a7b2d44".into(),
            line: Some(3),
            y: None,
            end: false
        })
    );
    assert!(tab.shows("Error 1 of 2"));

    // Next and Previous go round.
    tab.press("Next");
    assert!(tab.shows("Error 2 of 2"));
    assert!(logs_now(&tab).lines[5].current);
    assert_eq!(tab.take_operations().len(), 1);
    tab.press("Next");
    assert!(tab.shows("Error 1 of 2"), "after the last, the first");
    tab.press("Previous");
    assert!(tab.shows("Error 2 of 2"), "before the first, the last");

    tab.press("Hide logs");
    assert!(logs(&tab).is_empty(), "hidden again");
}

fn logs_now(tab: &Tab) -> hangar_kit_sdk::ui::Log {
    logs(tab).into_iter().next().expect("a log is open")
}

#[test]
fn a_log_without_an_error_opens_on_its_end_and_has_no_way_between_errors() {
    let mut world = world();
    world.logs = ran("listening on :3000\nGET /health 200 3ms\n");
    let mut tab = open(&mut world);
    tab.press("Logs");
    serve(&mut tab, &mut world);
    assert!(logs_now(&tab).at_end);
    assert!(tab.take_operations().is_empty());
    assert!(!has_button(&tab, "Next"));
    assert!(has_button(&tab, "Copy"), "the log can still be copied");
}

#[test]
fn the_log_can_be_copied_since_a_native_page_cannot_select_text() {
    let mut world = world();
    let mut tab = open(&mut world);
    tab.press("Logs");
    serve(&mut tab, &mut world);
    tab.press("Copy");
    let copy = tab.reply_to("ui.copy", Ok(json!({})));
    assert!(
        copy.params["text"]
            .as_str()
            .unwrap()
            .contains("fatal: database unreachable, exiting")
    );
    let toast = tab.reply_to("ui.toast", Ok(json!({ "action": false })));
    assert_eq!(toast.params["message"], "Copied");
}

#[test]
fn an_empty_log_and_a_failing_log_are_said() {
    let mut world = world();
    world.logs = ran("");
    let mut tab = open(&mut world);
    tab.press("Logs");
    serve(&mut tab, &mut world);
    assert!(tab.shows("This container has written nothing."));

    let mut world = self::world();
    world.logs = exited(1, "", "Error response from daemon: No such container");
    let mut tab = open(&mut world);
    tab.press("Logs");
    serve(&mut tab, &mut world);
    assert!(tab.shows("The logs could not be read: Error response from daemon"));
}

#[test]
fn asking_an_agent_the_question_opens_under_the_row_shows_the_brief_and_sends_it_to_the_pane_picked()
 {
    let mut world = world();
    let mut tab = open(&mut world);
    let row = row_of(&tab, "21100");
    assert!(row.has_button(ASK));
    assert!(
        row.buttons("Ask an agent to stop port 21100").len() == 1,
        "the button says what it is about on hover"
    );
    press_in(&mut tab, &row, ASK);

    let panel = row_of(&tab, "21100");
    assert!(panel.has_text("Ask an agent to stop port 21100"));
    let (options, chosen) = question_select(&tab).unwrap();
    assert_eq!(
        options
            .iter()
            .map(|(_, label)| label.as_str())
            .collect::<Vec<_>>(),
        ["—", "Jack", "Gus"],
        "only panes that can be sent to"
    );
    assert_eq!(
        chosen.as_deref(),
        Some("Gus"),
        "the worker who owns it is preselected"
    );
    let brief = brief(&tab).unwrap();
    assert!(
        brief
            .contains("Please stop what listens on port 21100: node, pid 4, started today at 3:48"),
        "{brief}"
    );

    tab.press("Send");
    assert!(disabled(&tab, "Send"), "while it goes, not twice");
    serve(&mut tab, &mut world);
    assert_eq!(world.to("panes.send")[0]["pane"], "Gus");
    assert_eq!(world.to("panes.send")[0]["text"].as_str().unwrap(), brief);
    assert!(row_of(&tab, "21100").has_text("Sent to Gus. It acts when it is free."));
    assert!(disabled(&tab, "Send"), "sent once, not twice");
    tab.press("Cancel");
    assert!(brief_none(&tab));
}

fn brief_none(tab: &Tab) -> bool {
    brief(tab).is_none()
}

#[test]
fn the_pane_is_picked_in_a_select_and_changing_it_changes_where_the_brief_goes() {
    let mut world = world();
    let mut tab = open(&mut world);
    ask_about(&mut tab, "21100");
    tab.choose("Send to", "Jack");
    assert_eq!(question_select(&tab).unwrap().1.as_deref(), Some("Jack"));
    tab.press("Send");
    serve(&mut tab, &mut world);
    assert_eq!(world.to("panes.send")[0]["pane"], "Jack");
}

#[test]
fn a_refused_send_is_a_banner_and_can_be_tried_again() {
    let mut world = world();
    world.send_error = Some("unavailable");
    let mut tab = open(&mut world);
    ask_about(&mut tab, "21100");
    tab.press("Send");
    serve(&mut tab, &mut world);
    assert!(tab.shows("Not sent: x"));
    assert!(!disabled(&tab, "Send"), "a failure leaves it to try again");
    world.send_error = Some("denied");
    tab.press("Send");
    serve(&mut tab, &mut world);
    assert!(tab.shows("This tab may not send to a pane. Accept panes.send in Settings › Kits."));
}

#[test]
fn asking_when_no_agent_can_be_reached_or_when_the_tab_may_not_send() {
    let mut world = world();
    world.panes = Ok(json!([panes()[2].clone()]));
    let mut tab = open(&mut world);
    ask_about(&mut tab, "21100");
    assert!(tab.shows("No agent can be reached"));
    assert!(question_select(&tab).is_none() && !has_button(&tab, "Send"));
    assert!(has_button(&tab, "Cancel"));

    let mut world = self::world();
    world.panes = refused("denied");
    let mut tab = open(&mut world);
    ask_about(&mut tab, "21100");
    assert!(tab.shows("This tab may not send to a pane"));

    let mut world = self::world();
    world.panes = refused("unavailable");
    let mut tab = open(&mut world);
    ask_about(&mut tab, "21100");
    assert!(tab.shows("The panes could not be listed."));
}

#[test]
fn docker_absent_docker_stopped_a_docker_error_each_its_own_state_the_ports_stay() {
    let mut absent = world();
    absent.docker = refused("not_found");
    let tab = open(&mut absent);
    assert!(tab.shows("Docker is not installed"));
    assert_eq!(port_numbers(&tab).len(), 4);
    assert!(
        !has_button(&tab, "Try again"),
        "nothing to try: Docker has to be installed"
    );

    let mut stopped = world();
    stopped.docker = exited(1, "", &fixture("docker-down.txt"));
    let tab = open(&mut stopped);
    assert!(tab.shows("Docker is not running"));
    assert!(tab.shows("Start Docker Desktop or the Docker service, then refresh"));
    assert!(has_button(&tab, "Try again"));

    let mut broken = world();
    broken.docker = exited(1, "", "permission denied");
    let tab = open(&mut broken);
    assert!(tab.shows("Docker answered with an error"));
    assert!(tab.shows("permission denied"));
}

#[test]
fn nothing_listening_and_no_container_said_with_what_makes_it_change() {
    let mut world = world();
    world.ports = ran(r#"{"platform":"linux","ports":[],"notes":[]}"#);
    world.docker = ran("");
    let tab = open(&mut world);
    assert!(tab.shows("Nothing is listening"));
    assert!(tab.shows("A server started later shows here."));
    assert!(tab.shows("No container"));
    assert!(filters(&tab).is_empty());
}

#[test]
fn denied_one_state_for_the_whole_tab_with_what_to_accept_and_where_and_a_lock() {
    let mut world = world();
    world.ports = refused("denied");
    world.docker = refused("denied");
    world.panes = refused("denied");
    let tab = open(&mut world);
    let denied: Vec<_> = states(&tab)
        .into_iter()
        .filter(|(kind, _)| *kind == EmptyKind::Denied)
        .collect();
    assert_eq!(
        denied,
        [(EmptyKind::Denied, "This tab may not read this".to_owned())]
    );
    assert!(tab.shows("Accept the commands of Services in Settings › Kits"));
    assert!(
        sections(&tab).is_empty(),
        "no second section saying the same"
    );
}

#[test]
fn denied_for_one_of_the_two_that_part_says_so_the_other_still_shows() {
    let mut world = world();
    world.docker = refused("denied");
    let tab = open(&mut world);
    assert_eq!(port_numbers(&tab).len(), 4);
    let denied: Vec<_> = states(&tab)
        .into_iter()
        .filter(|(kind, _)| *kind == EmptyKind::Denied)
        .collect();
    assert_eq!(denied.len(), 1);
    assert_eq!(sections(&tab), ["Ports", "Containers"]);
}

#[test]
fn the_ports_could_not_be_listed_node_missing_no_tool_with_a_way_to_try_again() {
    let mut no_node = world();
    no_node.ports = refused("not_found");
    assert!(open(&mut no_node).shows("Node is not installed"));

    let mut no_tool = world();
    no_tool.ports = ran(r#"{"error":"no-tool"}"#);
    let tab = open(&mut no_tool);
    assert!(tab.shows("No tool lists the ports here"));
    assert!(tab.shows("PowerShell on Windows, lsof on macOS, ss or lsof on Linux"));
    assert!(has_button(&tab, "Try again"));
}

#[test]
fn a_listing_that_took_too_long_says_so_and_never_that_no_tool_is_here() {
    let late = || exited(2, "{\"error\":\"timeout\"}\n", "");
    let mut world_en = world();
    world_en.ports = late();
    let tab = open(&mut world_en);
    assert!(tab.shows("It took too long"));
    assert!(!tab.shows("No tool lists the ports here"));
    assert!(has_button(&tab, "Try again"));

    let mut world_fr = world();
    world_fr.ports = late();
    let tab = open_as(testing::start().locale("fr"), &mut world_fr);
    assert!(tab.shows("C'était trop long"));
    assert!(!tab.shows("Aucun outil ne liste les ports"));
}

#[test]
fn the_10_s_read_changes_nothing_the_person_is_using() {
    // A page drawn whole keeps its fields' state; what the person opened is
    // the tab's state, and a new read leaves it as it is.
    let mut world = world();
    let mut tab = open(&mut world);
    tab.press("Logs");
    serve(&mut tab, &mut world);
    ask_about(&mut tab, "21100");
    tab.choose("Send to", "Jack");
    let before = (tab.kit().logs.clone(), tab.kit().ask.clone());
    tab.elapse("refresh");
    serve(&mut tab, &mut world);
    assert_eq!((tab.kit().logs.clone(), tab.kit().ask.clone()), before);
    assert_eq!(logs(&tab).len(), 1, "the log is still open");
    assert_eq!(
        question_select(&tab).unwrap().1.as_deref(),
        Some("Jack"),
        "so is the question, and its pane"
    );
    assert!(!tab.kit().refreshing);
}

#[test]
fn the_refresh_button_reads_reading_and_waits_while_it_reads() {
    let mut world = world();
    let mut tab = open(&mut world);
    assert!(!disabled(&tab, "Refresh"));
    tab.press("Refresh");
    assert!(disabled(&tab, "Reading…"));
    assert!(!has_button(&tab, "Refresh"));
    serve(&mut tab, &mut world);
    assert!(has_button(&tab, "Refresh"));
}

#[test]
fn the_header_says_when_it_was_updated_at_the_clock_of_the_person() {
    let mut world = world();
    let tab = open_as(testing::start().utc_offset(60), &mut world);
    assert!(tab.shows("Updated 18:02:00"), "{:?}", tab.texts());
}

#[test]
fn french_the_same_page_in_the_interfaces_words_demander_a_un_agent_darreter_ca_included() {
    let mut world = world();
    let tab = open_as(testing::start().locale("fr"), &mut world);
    // Hangar sets a no-break space between a number and its unit: said plainly here.
    let text = page_text(&tab).replace(['\u{a0}', '\u{202f}'], " ");
    assert!(text.contains("Ports") && text.contains("Conteneurs"));
    assert!(text.contains("Bloc de ports de Gus"));
    assert!(text.contains("dont le panneau est fermé"));
    assert!(has_button(&tab, "Actualiser"));
    assert!(has_button(&tab, "Journal"));
    assert!(buttons(&tab, "Demander à un agent d'arrêter ça").len() >= 3);
    assert!(text.contains("S'actualise toutes les 10 s"));
    assert!(text.contains("lancé aujourd'hui à 15 h 48"), "{text}");
}

#[test]
fn what_is_not_the_persons_offers_no_question_to_stop_it_and_says_whose_it_is() {
    let row = |port: u32, pid: u32, name: &str, mine: Option<bool>| {
        let mut row = json!({ "port": port, "pid": pid, "name": name, "startedAt": null, "loopback": true, "portBase": null, "pane": null });
        if let Some(mine) = mine {
            row["mine"] = json!(mine);
        }
        row
    };
    let ports = json!({
        "platform": "win32",
        "ports": [row(135, 1012, "svchost.exe", Some(false)), row(445, 4, "System", None), row(3000, 7, "node.exe", Some(true))],
        "notes": []
    })
    .to_string();
    for (locale, words) in [
        ("en", "the system's, or another account's"),
        ("fr", "du système ou d'un autre compte"),
    ] {
        let mut world = World::new(&ports);
        let tab = open_as(testing::start().locale(locale), &mut world);
        let find = |name: &str| {
            port_items(&tab)
                .into_iter()
                .find(|row| row.0.title.contains(name))
                .unwrap_or_else(|| panic!("{name}"))
        };
        let ask = if locale == "en" {
            ASK
        } else {
            "Demander à un agent d'arrêter ça"
        };
        assert!(!find("svchost.exe").has_button(ask), "svchost");
        assert!(
            !find("System").has_button(ask),
            "System, which says nothing of whose it is"
        );
        assert!(find("node.exe").has_button(ask), "the person's own program");
        assert!(find("svchost.exe").has_text(words));
        assert!(!find("node.exe").has_text(words));
    }
}

#[test]
fn the_page_names_no_colour_and_no_size_of_its_own() {
    // The kit says what something is; Hangar draws it in the person's theme.
    let mut world = world();
    let mut tab = open(&mut world);
    tab.press("Logs");
    serve(&mut tab, &mut world);
    ask_about(&mut tab, "21100");
    let page = serde_json::to_string(&tab.view().root).unwrap();
    let colour = regex::Regex::new(r#"#[0-9a-fA-F]{3,8}\b|rgba?\(|hsla?\(|oklch\("#).unwrap();
    assert!(!colour.is_match(&page));
    assert!(!page.contains("\"ink\""));
}

#[test]
fn every_state_has_an_icon_that_is_not_the_alert_of_a_failure_when_it_is_not_one() {
    let mut absent = world();
    absent.docker = refused("not_found");
    let tab = open(&mut absent);
    let empties = gather(&tab.view().root, |element| match element {
        hangar_kit_sdk::ui::Element::Empty(empty) => {
            Some((empty.kind, empty.icon, empty.title.clone()))
        }
        _ => None,
    });
    assert_eq!(
        empties,
        [(
            EmptyKind::Empty,
            Some(Icon::Info),
            "Docker is not installed".to_owned()
        )]
    );
}

#[test]
fn only_one_button_is_primary_at_a_time() {
    let mut world = world();
    let mut tab = open(&mut world);
    let primaries = |tab: &Tab| {
        gather(&tab.view().root, |element| match element {
            hangar_kit_sdk::ui::Element::Button(button)
                if button.kind == hangar_kit_sdk::ui::ButtonKind::Primary =>
            {
                button.label.clone()
            }
            _ => None,
        })
    };
    assert_eq!(primaries(&tab), ["Refresh"]);
    ask_about(&mut tab, "21100");
    assert_eq!(
        primaries(&tab),
        ["Send"],
        "with a question open, Send is the one"
    );
}

//! Shared by the tests: the recorded output of the tools, a pretend Hangar
//! that answers the tab's calls as docs/kits.md says (`test/helpers.js`'s
//! `fakeHangar`), and ways to read the page the tab draws.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use hangar_kit_sdk::api::KitError;
use hangar_kit_sdk::format;
use hangar_kit_sdk::testing::{self, Harness, StartExt};
use hangar_kit_sdk::ui::abi::Call;
use hangar_kit_sdk::ui::{Element, HandlerId, ListItem};
use kit_services_tab::Services;
use serde_json::{Value, json};

/// The folder of the kit: the one `hangar-kit.json` is in.
pub fn kit() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("native/ sits in the kit")
        .to_path_buf()
}

pub fn fixture(name: &str) -> String {
    std::fs::read_to_string(kit().join("test/fixtures").join(name))
        .unwrap_or_else(|error| panic!("fixture {name}: {error}"))
}

pub fn manifest() -> Value {
    let text = std::fs::read_to_string(kit().join("hangar-kit.json")).expect("hangar-kit.json");
    serde_json::from_str(&text).expect("hangar-kit.json is JSON")
}

/// The permissions the manifest gives the tab: what the tab holds in these
/// tests.
pub fn granted() -> Vec<String> {
    manifest()["tabs"][0]["permissions"]
        .as_array()
        .expect("the tab declares permissions")
        .iter()
        .map(|permission| {
            permission
                .as_str()
                .expect("a permission is text")
                .to_owned()
        })
        .collect()
}

/// 2026-10-31 17:02 UTC: the moment every test is at.
pub fn now() -> i64 {
    format::parse_iso("2026-10-31T17:02:00Z").expect("a date")
}

// ---------------------------------------------------------------- Hangar.

pub type Answer = Result<Value, KitError>;

/// A command that ran and exited 0.
pub fn ran(stdout: &str) -> Answer {
    exited(0, stdout, "")
}

pub fn exited(code: i32, stdout: &str, stderr: &str) -> Answer {
    Ok(
        json!({ "exitCode": code, "stdout": stdout, "stderr": stderr, "timedOut": false, "truncated": false }),
    )
}

/// A command stopped at its limit: no exit code, and `timedOut`.
pub fn timed_out() -> Answer {
    Ok(
        json!({ "exitCode": null, "stdout": "", "stderr": "", "timedOut": true, "truncated": false }),
    )
}

pub fn refused(code: &str) -> Answer {
    Err(KitError::new(
        code,
        format!("{code}: refused by the pretend Hangar"),
    ))
}

pub fn panes() -> Value {
    json!([
        { "name": "Jack", "agent": "claude", "role": "orchestrator", "state": "idle", "canSend": true },
        { "name": "Gus", "agent": "claude", "role": "worker", "state": "working", "canSend": true },
        { "name": "Shell", "agent": "shell", "role": null, "state": null, "canSend": false }
    ])
}

/// What the pretend Hangar answers, and every call it was asked.
pub struct World {
    pub ports: Answer,
    pub docker: Answer,
    pub logs: Answer,
    pub panes: Answer,
    /// `panes.send` refused with this code.
    pub send_error: Option<&'static str>,
    pub calls: Vec<(String, Value)>,
}

impl World {
    pub fn new(ports_stdout: &str) -> World {
        World {
            ports: ran(ports_stdout),
            docker: ran(&fixture("docker-ps.jsonl")),
            logs: ran(&fixture("docker-logs.txt")),
            panes: Ok(panes()),
            send_error: None,
            calls: Vec::new(),
        }
    }

    fn answer(&mut self, call: &Call) -> Answer {
        self.calls.push((call.method.clone(), call.params.clone()));
        match call.method.as_str() {
            "command.run" => match call.params["name"].as_str() {
                Some("ports") => self.ports.clone(),
                Some("docker-list") => self.docker.clone(),
                Some("docker-logs") => self.logs.clone(),
                other => Err(KitError::new(
                    "denied",
                    format!("no such command {other:?}"),
                )),
            },
            "panes.list" => self.panes.clone(),
            "panes.send" => {
                if let Some(code) = self.send_error {
                    return Err(KitError::new(code, "x"));
                }
                let wanted = call.params["pane"]
                    .as_str()
                    .unwrap_or_default()
                    .to_lowercase();
                let known = self
                    .panes
                    .as_ref()
                    .ok()
                    .and_then(Value::as_array)
                    .is_some_and(|panes| {
                        panes.iter().any(|pane| {
                            pane["name"].as_str().unwrap_or_default().to_lowercase() == wanted
                        })
                    });
                if known {
                    Ok(json!({ "queued": true, "pane": call.params["pane"] }))
                } else {
                    Err(KitError::new("not_found", "no such pane"))
                }
            }
            "ui.toast" => Ok(json!({ "action": false })),
            "ui.copy" => Ok(json!({})),
            other => Err(KitError::new("unknown", format!("unknown {other}"))),
        }
    }

    /// How many times the command `name` was run.
    pub fn runs(&self, name: &str) -> usize {
        self.calls
            .iter()
            .filter(|(method, params)| method == "command.run" && params["name"] == name)
            .count()
    }

    /// The calls to `method`, with their params.
    pub fn to(&self, method: &str) -> Vec<&Value> {
        self.calls
            .iter()
            .filter(|(called, _)| called == method)
            .map(|(_, params)| params)
            .collect()
    }
}

/// Answers every call the tab is waiting on, and those its answers make.
pub fn serve(tab: &mut Harness<Services>, world: &mut World) {
    while let Some(call) = tab.calls().first().cloned() {
        let answer = world.answer(&call);
        tab.reply(call.id, answer);
    }
}

/// A tab that holds the manifest's permissions, started, and read.
pub fn open(world: &mut World) -> Harness<Services> {
    open_as(testing::start(), world)
}

pub fn open_as(start: hangar_kit_sdk::Start, world: &mut World) -> Harness<Services> {
    let mut tab = started(start);
    serve(&mut tab, world);
    tab
}

/// A tab started, its first calls waiting for an answer.
pub fn started(start: hangar_kit_sdk::Start) -> Harness<Services> {
    testing::set_now(Some(now()));
    Harness::new(start.grant(granted()))
}

// ---------------------------------------------------------------- The page.

/// Every text, button label, chip and banner of an element and under it.
pub fn texts_of(root: &Element) -> Vec<String> {
    let mut texts = Vec::new();
    root.for_each(&mut |element| match element {
        Element::Text(text) => texts.push(text.text.clone()),
        Element::Header(header) => texts.push(header.title.clone()),
        Element::Section(section) => {
            texts.push(section.title.clone());
            texts.extend(section.count.clone());
        }
        Element::Button(button) => texts.extend(button.label.clone()),
        Element::List(list) => {
            for item in &list.items {
                texts.push(item.title.clone());
                texts.extend(item.sub.clone());
            }
        }
        Element::Empty(empty) => {
            texts.push(empty.title.clone());
            texts.extend(empty.text.clone());
        }
        Element::Banner(banner) => {
            texts.extend(banner.title.clone());
            texts.push(banner.text.clone());
        }
        Element::Log(log) => texts.extend(log.lines.iter().map(|line| line.text.clone())),
        Element::Select(select) => {
            texts.extend(select.label.clone());
            texts.extend(
                select
                    .options
                    .iter()
                    .filter(|option| select.selected.as_deref() == Some(&option.value))
                    .map(|option| option.label.clone()),
            );
        }
        Element::Filter(filter) => {
            texts.push(filter.label.clone());
            texts.extend(filter.count.clone());
        }
        Element::State(state) => texts.push(state.label.clone()),
        Element::Glyph(glyph) => texts.push(glyph.label.clone()),
        Element::Chip(chip) => texts.push(chip.label.clone()),
        Element::Pre(pre) => texts.push(pre.text.clone()),
        _ => {}
    });
    texts
}

/// Everything the page says, in one text.
pub fn page_text(tab: &Harness<Services>) -> String {
    texts_of(&tab.view().root).join("\n")
}

/// What `pick` finds in the element and under it.
pub fn gather<T>(root: &Element, mut pick: impl FnMut(&Element) -> Option<T>) -> Vec<T> {
    let mut found = Vec::new();
    root.for_each(&mut |element| {
        if let Some(item) = pick(element) {
            found.push(item);
        }
    });
    found
}

/// A row of a list, with what hangs under it.
pub struct Item(pub ListItem);

impl Item {
    pub fn parts(&self) -> Vec<Element> {
        let mut parts: Vec<Element> = Vec::new();
        parts.extend(self.0.lead.as_deref().cloned());
        parts.extend(self.0.end.as_deref().cloned());
        parts.extend(self.0.actions.iter().cloned());
        parts.extend(self.0.body.iter().cloned());
        parts
    }

    /// The title, the second line and every text under the row.
    pub fn text(&self) -> String {
        let mut texts = vec![self.0.title.clone()];
        texts.extend(self.0.sub.clone());
        for part in self.parts() {
            texts.extend(texts_of(&part));
        }
        texts.join("\n")
    }

    /// The handlers of the buttons labelled `label` in this row (none for a
    /// button drawn disabled).
    pub fn buttons(&self, label: &str) -> Vec<Option<HandlerId>> {
        self.parts()
            .iter()
            .flat_map(|part| {
                gather(part, |element| match element {
                    Element::Button(button)
                        if button.label.as_deref() == Some(label)
                            || button.tooltip.as_deref() == Some(label) =>
                    {
                        Some(button.on_press)
                    }
                    _ => None,
                })
            })
            .collect()
    }

    pub fn has_button(&self, label: &str) -> bool {
        !self.buttons(label).is_empty()
    }

    pub fn has_text(&self, needle: &str) -> bool {
        self.text().contains(needle)
    }

    /// The glyph at the head of a container's row.
    pub fn glyph(&self) -> Option<hangar_kit_sdk::ui::Glyph> {
        match self.0.lead.as_deref() {
            Some(Element::Glyph(glyph)) => Some(glyph.clone()),
            _ => None,
        }
    }
}

/// Every row of every list of the page: ports first, then containers.
pub fn items(tab: &Harness<Services>) -> Vec<Item> {
    gather(&tab.view().root, |element| match element {
        Element::List(list) => Some(list.items.clone()),
        _ => None,
    })
    .into_iter()
    .flatten()
    .map(Item)
    .collect()
}

/// The rows of ports: the title starts with the number.
pub fn port_items(tab: &Harness<Services>) -> Vec<Item> {
    items(tab)
        .into_iter()
        .filter(|item| item.0.lead.is_none())
        .collect()
}

/// The rows of containers: a glyph leads them.
pub fn container_items(tab: &Harness<Services>) -> Vec<Item> {
    items(tab)
        .into_iter()
        .filter(|item| item.glyph().is_some())
        .collect()
}

/// The ports listed, as numbers.
pub fn port_numbers(tab: &Harness<Services>) -> Vec<String> {
    port_items(tab)
        .iter()
        .map(|item| {
            item.0
                .title
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .to_owned()
        })
        .collect()
}

/// The handlers of the buttons of the page labelled `label`.
pub fn buttons(tab: &Harness<Services>, label: &str) -> Vec<Option<HandlerId>> {
    gather(&tab.view().root, |element| match element {
        Element::Button(button)
            if button.label.as_deref() == Some(label)
                || button.tooltip.as_deref() == Some(label) =>
        {
            Some(button.on_press)
        }
        _ => None,
    })
}

pub fn has_button(tab: &Harness<Services>, label: &str) -> bool {
    !buttons(tab, label).is_empty()
}

/// Whether the button labelled `label` is drawn disabled.
pub fn disabled(tab: &Harness<Services>, label: &str) -> bool {
    let all = buttons(tab, label);
    assert!(!all.is_empty(), "no button is labelled {label:?}");
    all.iter().all(Option::is_none)
}

/// Presses the `n`th button of the page labelled `label`, from 0.
pub fn press_nth(tab: &mut Harness<Services>, label: &str, n: usize) {
    let handler = buttons(tab, label)
        .get(n)
        .copied()
        .flatten()
        .unwrap_or_else(|| panic!("no enabled button number {n} is labelled {label:?}"));
    tab.ui(handler, None);
}

/// Presses the button `label` of a row of the page, then the page is served.
pub fn press_in(tab: &mut Harness<Services>, row: &Item, label: &str) {
    let handler = row
        .buttons(label)
        .first()
        .copied()
        .flatten()
        .unwrap_or_else(|| panic!("the row has no enabled button {label:?}"));
    tab.ui(handler, None);
}

/// The section titled `title`, with its count.
pub fn section(tab: &Harness<Services>, title: &str) -> Option<(String, Option<String>)> {
    gather(&tab.view().root, |element| match element {
        Element::Section(section) if section.title == title => {
            Some((section.title.clone(), section.count.clone()))
        }
        _ => None,
    })
    .into_iter()
    .next()
}

/// The titles of the page's sections.
pub fn sections(tab: &Harness<Services>) -> Vec<String> {
    gather(&tab.view().root, |element| match element {
        Element::Section(section) => Some(section.title.clone()),
        _ => None,
    })
}

/// The filters of the page, as `(label, count, pressed)`.
pub fn filters(tab: &Harness<Services>) -> Vec<(String, Option<String>, bool)> {
    gather(&tab.view().root, |element| match element {
        Element::Filter(filter) => {
            Some((filter.label.clone(), filter.count.clone(), filter.pressed))
        }
        _ => None,
    })
}

/// The titles of the empty, loading, failed and denied states of the page.
pub fn states(tab: &Harness<Services>) -> Vec<(hangar_kit_sdk::ui::EmptyKind, String)> {
    gather(&tab.view().root, |element| match element {
        Element::Empty(empty) => Some((empty.kind, empty.title.clone())),
        _ => None,
    })
}

/// The long logs of the page: their id and their lines.
pub fn logs(tab: &Harness<Services>) -> Vec<hangar_kit_sdk::ui::Log> {
    gather(&tab.view().root, |element| match element {
        Element::Log(log) => Some(log.clone()),
        _ => None,
    })
}

/// The select of the question: its options as `(value, label)` and its choice.
pub type Choice = (Vec<(String, String)>, Option<String>);

pub fn question_select(tab: &Harness<Services>) -> Option<Choice> {
    gather(&tab.view().root, |element| match element {
        Element::Select(select) => Some((
            select
                .options
                .iter()
                .map(|option| (option.value.clone(), option.label.clone()))
                .collect(),
            select.selected.clone(),
        )),
        _ => None,
    })
    .into_iter()
    .next()
}

/// The text of the `pre` of the open question.
pub fn brief(tab: &Harness<Services>) -> Option<String> {
    gather(&tab.view().root, |element| match element {
        Element::Pre(pre) => Some(pre.text.clone()),
        _ => None,
    })
    .into_iter()
    .next()
}

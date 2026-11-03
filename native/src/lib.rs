//! The Services tab, native: what listens on this computer, on which port,
//! and whose it is.
//!
//! Three read-only commands (`ports`, `docker-list`, `docker-logs`) and the
//! workspace's panes, read every 10 s while the tab is in front. A port is
//! tied to the worker whose port block it sits in; "Ask an agent to stop this"
//! sends a brief to the pane the person picks, and the kit stops nothing
//! itself.
//!
//! This is `tabs/services/{controller,view,app}.js` in Rust, with the same
//! host calls (`command.run`, `panes.list`, `panes.send`) under the same
//! permissions. The pure parts, `tabs/services/logic.js`, are in [`logic`].
//!
//! - **State**: [`Services`] is what the page's controller held.
//! - **Calls are tasks**: the three reads go out together, and the page is
//!   updated once all three have come back, as `Promise.all` did.
//! - **The poll** is a one-shot `cx.after("refresh", …)` armed when a read
//!   ends, so a read never overlaps the next, and none is armed behind another
//!   tab.

pub mod logic;

use std::collections::BTreeMap;
use std::time::Duration;

use hangar_kit_sdk::api::{
    CommandAnswer, CommandRun, CopyText, Done, Pane, PanesList, PanesSend, Queued, Theme,
};
use hangar_kit_sdk::prelude::*;
use hangar_kit_sdk::ui::{Button, Empty, ListItem, Section};

use logic::{
    BLOCK_SIZE, Container, Owner, ParsedPorts, Port, PortRow, PortsError, Target, brief_for,
    classify, container_state, docker_fault, is_container_id, is_error_line, parse_docker,
    parse_ports, port_rows, strip_ansi,
};

/// How long after a read the next one starts, while the tab is in front.
pub const REFRESH_EVERY: Duration = Duration::from_secs(10);

/// The most of a failure's own words that is shown.
const DETAIL_CHARS: usize = 500;

/// How tall a container's log is drawn, in pixels (the page's `40vh`).
const LOG_HEIGHT: f32 = 320.0;

// ---------------------------------------------------------------- State.

/// How a part of the page was read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Health {
    Ok,
    /// The tab may not run the command.
    Denied,
    /// Docker is not installed.
    Absent,
    /// Docker is installed and not running.
    Stopped,
    Error {
        problem: Problem,
        /// What the tool said, for the person to read.
        detail: String,
    },
}

/// What went wrong with a command that was allowed to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Problem {
    /// Node is not on the PATH: `ports.js` cannot run.
    NoNode,
    /// `ports.js` ran and no tool of the system lists ports.
    NoTool,
    Timeout,
    Failed,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PortsState {
    pub items: Vec<Port>,
    pub notes: Vec<String>,
    pub health: Health,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DockerState {
    pub items: Vec<Container>,
    pub health: Health,
}

/// Which ports are listed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Show {
    All,
    /// Only those in a worker's port block.
    Blocks,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogsStatus {
    Loading,
    Ok,
    Failed,
}

/// A container's open log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Logs {
    pub id: String,
    pub status: LogsStatus,
    pub lines: Vec<String>,
    /// The indexes of the lines that read as errors.
    pub errors: Vec<usize>,
    /// Which of `errors` is looked at.
    pub current: Option<usize>,
    pub detail: String,
}

/// Which port or container a question is about. A port is its number and its
/// program's pid: the same number can be another program a minute later.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AskKey {
    Port { port: u32, pid: Option<u32> },
    Container(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AskStatus {
    Idle,
    Sending,
    Sent,
    Failed,
}

/// "Ask an agent to stop this", open under a row.
#[derive(Debug, Clone, PartialEq)]
pub struct Ask {
    /// Told apart from an earlier question about the same thing, so that its
    /// answer is not given to this one.
    pub serial: u32,
    pub key: AskKey,
    pub target: Target,
    /// The pane picked; empty until one is.
    pub pane: String,
    pub status: AskStatus,
    /// Why it failed: `denied`, or Hangar's words.
    pub detail: String,
}

/// The three reads of one refresh, as they come back.
#[derive(Default)]
struct InFlight {
    ports: Option<Result<CommandAnswer, KitError>>,
    docker: Option<Result<CommandAnswer, KitError>>,
    panes: Option<Result<Vec<Pane>, KitError>>,
}

/// The tab. Its fields are public so that the tests read them; nothing outside
/// the module sets them.
pub struct Services {
    pub t: Strings,
    pub utc_offset: i32,
    /// When the last message was handled, in ms: what ages are counted from.
    pub now: i64,
    pub visible: bool,
    pub loaded: bool,
    pub refreshing: bool,
    pub updated_at: Option<i64>,
    pub ports: PortsState,
    pub docker: DockerState,
    /// `panes.list`, or none when it could not be read.
    pub panes: Option<Vec<Pane>>,
    pub panes_denied: bool,
    pub show: Show,
    pub logs: Option<Logs>,
    pub ask: Option<Ask>,
    generation: u32,
    in_flight: InFlight,
    asks: u32,
}

#[derive(Clone, Debug)]
pub enum Message {
    /// The button, or the poll coming round.
    Refresh,
    Tick,
    PortsRead(u32, Result<CommandAnswer, KitError>),
    DockerRead(u32, Result<CommandAnswer, KitError>),
    PanesRead(u32, Result<Vec<Pane>, KitError>),
    /// `panes.changed`: the panes alone are read again.
    PanesChanged,
    PanesListed(Result<Vec<Pane>, KitError>),
    Visible(bool),
    Theme(Theme),
    Show(Show),
    ToggleLogs(String),
    LogsRead(String, Result<CommandAnswer, KitError>),
    /// Next (1) or previous (-1) error of the open log.
    Error(i32),
    CopyLogs,
    Copied(Result<Done, KitError>),
    Ask(AskKey),
    Pick(String),
    Send,
    Sent(u32, Result<Queued, KitError>),
    CloseAsk,
}

// ---------------------------------------------------------------- Reading a command.

/// What a command did, as the tab tells it.
enum Ran {
    Done { stdout: String, stderr: String },
    Failed(Failure),
}

struct Failure {
    fault: logic::Fault,
    /// What the command or Hangar said, stripped of colours and cut.
    detail: String,
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
}

fn ran(answer: Result<CommandAnswer, KitError>) -> Ran {
    let refused = |fault, detail: String| {
        Ran::Failed(Failure {
            fault,
            detail,
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
        })
    };
    match answer {
        Err(error) => refused(classify(&error.code), error.message),
        Ok(answer) if answer.timed_out => refused(logic::Fault::Timeout, String::new()),
        Ok(answer) if answer.exit_code == Some(0) => Ran::Done {
            stdout: answer.stdout,
            stderr: answer.stderr,
        },
        Ok(answer) => {
            let said = if answer.stderr.is_empty() {
                &answer.stdout
            } else {
                &answer.stderr
            };
            Ran::Failed(Failure {
                fault: logic::Fault::Failed,
                detail: strip_ansi(said).trim().chars().take(DETAIL_CHARS).collect(),
                exit_code: answer.exit_code,
                stdout: answer.stdout,
                stderr: answer.stderr,
            })
        }
    }
}

fn read_ports(ran: Ran) -> PortsState {
    let broken = |health| PortsState {
        items: Vec::new(),
        notes: Vec::new(),
        health,
    };
    let error = |problem| {
        broken(Health::Error {
            problem,
            detail: String::new(),
        })
    };
    match ran {
        Ran::Done { stdout, .. } => match parse_ports(&stdout) {
            Ok(ParsedPorts { ports, notes }) => PortsState {
                items: ports,
                notes,
                health: Health::Ok,
            },
            Err(PortsError::Tool(said)) if said == "no-tool" => error(Problem::NoTool),
            Err(PortsError::Tool(said)) if said == "timeout" => error(Problem::Timeout),
            Err(_) => error(Problem::Failed),
        },
        Ran::Failed(failure) => match failure.fault {
            logic::Fault::Denied => broken(Health::Denied),
            logic::Fault::Missing => error(Problem::NoNode),
            // The script says which on stdout, and exits 2 for both.
            _ if failure.exit_code == Some(2) && failure.stdout.contains("no-tool") => {
                error(Problem::NoTool)
            }
            _ if failure.exit_code == Some(2) && failure.stdout.contains("\"timeout\"") => {
                error(Problem::Timeout)
            }
            logic::Fault::Timeout => error(Problem::Timeout),
            _ => broken(Health::Error {
                problem: Problem::Failed,
                detail: failure.detail,
            }),
        },
    }
}

fn read_docker(ran: Ran) -> DockerState {
    let broken = |health| DockerState {
        items: Vec::new(),
        health,
    };
    match ran {
        Ran::Done { stdout, .. } => match parse_docker(&stdout) {
            Some(items) => DockerState {
                items,
                health: Health::Ok,
            },
            None => broken(Health::Error {
                problem: Problem::Failed,
                detail: String::new(),
            }),
        },
        Ran::Failed(failure) => match failure.fault {
            logic::Fault::Denied => broken(Health::Denied),
            logic::Fault::Missing => broken(Health::Absent),
            logic::Fault::Failed => {
                match docker_fault(&format!("{} {}", failure.stderr, failure.stdout)) {
                    logic::DockerFault::Stopped => broken(Health::Stopped),
                    logic::DockerFault::Failed => broken(Health::Error {
                        problem: Problem::Failed,
                        detail: failure.detail,
                    }),
                }
            }
            fault => broken(Health::Error {
                problem: if fault == logic::Fault::Timeout {
                    Problem::Timeout
                } else {
                    Problem::Failed
                },
                detail: failure.detail,
            }),
        },
    }
}

fn read_panes(answer: Result<Vec<Pane>, KitError>) -> (Option<Vec<Pane>>, bool) {
    match answer {
        Ok(panes) => (Some(panes), false),
        Err(error) => (None, error.is(KitError::DENIED)),
    }
}

fn command(name: &str, args: &[(&str, &str)]) -> CommandRun {
    CommandRun {
        name: name.to_owned(),
        args: args
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect::<BTreeMap<_, _>>(),
    }
}

// ---------------------------------------------------------------- The tab.

impl Services {
    /// Reads everything again. A read already going is not doubled.
    fn refresh(&mut self, cx: &mut Context<Message>) {
        if self.refreshing {
            return;
        }
        self.refreshing = true;
        self.generation += 1;
        self.in_flight = InFlight::default();
        let generation = self.generation;
        cx.call(command("ports", &[]), move |answer| {
            Message::PortsRead(generation, answer)
        });
        cx.call(command("docker-list", &[]), move |answer| {
            Message::DockerRead(generation, answer)
        });
        cx.call(PanesList {}, move |answer| {
            Message::PanesRead(generation, answer)
        });
    }

    /// The three reads are in: the page changes once.
    fn finish_refresh(&mut self, cx: &mut Context<Message>) {
        let reads = &self.in_flight;
        if reads.ports.is_none() || reads.docker.is_none() || reads.panes.is_none() {
            return;
        }
        let InFlight {
            ports: Some(ports),
            docker: Some(docker),
            panes: Some(panes),
        } = std::mem::take(&mut self.in_flight)
        else {
            return;
        };
        self.ports = read_ports(ran(ports));
        self.docker = read_docker(ran(docker));
        (self.panes, self.panes_denied) = read_panes(panes);
        self.loaded = true;
        self.refreshing = false;
        self.updated_at = Some(self.now);
        // A container that is gone takes its open logs and its question with it.
        if let Some(logs) = &self.logs
            && self.docker.health == Health::Ok
            && !self
                .docker
                .items
                .iter()
                .any(|container| container.id == logs.id)
        {
            self.logs = None;
        }
        if self
            .ask
            .as_ref()
            .is_some_and(|ask| ask.status == AskStatus::Idle && !self.still_there(&ask.target))
        {
            self.ask = None;
        }
        self.schedule(cx);
    }

    fn still_there(&self, target: &Target) -> bool {
        match target {
            Target::Container(container) => self
                .docker
                .items
                .iter()
                .any(|other| other.id == container.id),
            Target::Port(row) => self
                .ports
                .items
                .iter()
                .any(|port| port.port == row.port.port && port.pid == row.port.pid),
        }
    }

    /// Arms the next read, or none while the tab is behind another.
    fn schedule(&self, cx: &mut Context<Message>) {
        if self.visible {
            cx.after("refresh", REFRESH_EVERY, Message::Tick);
        } else {
            cx.cancel("refresh");
        }
    }

    pub fn rows(&self) -> Vec<PortRow> {
        port_rows(&self.ports.items, self.panes.as_deref(), &self.docker.items)
    }

    /// The row or container a question is about, as it is now.
    fn target(&self, key: &AskKey) -> Option<Target> {
        match key {
            AskKey::Port { port, pid } => self
                .rows()
                .into_iter()
                .find(|row| row.port.port == *port && row.port.pid == *pid)
                .map(Target::Port),
            AskKey::Container(id) => self
                .docker
                .items
                .iter()
                .find(|container| container.id == *id)
                .cloned()
                .map(Target::Container),
        }
    }

    /// When a time was, in words.
    fn since(&self, at: i64) -> String {
        format::since(at, self.now, self.utc_offset, self.t.locale())
    }

    /// The panes `panes.send` reaches now.
    fn sendable(&self) -> Vec<&Pane> {
        self.panes
            .iter()
            .flatten()
            .filter(|pane| pane.can_send)
            .collect()
    }

    fn open_ask(&mut self, key: AskKey) {
        // Asking again about the same thing closes the question.
        if self.ask.as_ref().is_some_and(|ask| ask.key == key) {
            self.ask = None;
            return;
        }
        let Some(target) = self.target(&key) else {
            return;
        };
        // The worker who owns the port is the one to ask first.
        let owner = match &target {
            Target::Port(PortRow {
                owner: Some(Owner::Worker { name, .. }),
                ..
            }) => Some(name.to_lowercase()),
            _ => None,
        };
        let pane = self
            .sendable()
            .into_iter()
            .find(|pane| Some(pane.name.to_lowercase()) == owner)
            .map(|pane| pane.name.clone())
            .unwrap_or_default();
        self.asks += 1;
        self.ask = Some(Ask {
            serial: self.asks,
            key,
            target,
            pane,
            status: AskStatus::Idle,
            detail: String::new(),
        });
    }

    fn brief(&self, target: &Target) -> String {
        brief_for(target, &self.t, &|at| self.since(at))
    }

    fn logs_id(id: &str) -> String {
        format!("logs-{id}")
    }

    /// The log gets its lines: the last 200, stdout then stderr, which is where
    /// a container writes its errors. It opens on its first error.
    fn read_logs(
        &mut self,
        id: &str,
        answer: Result<CommandAnswer, KitError>,
        cx: &mut Context<Message>,
    ) {
        let Some(logs) = self.logs.as_mut().filter(|logs| logs.id == id) else {
            return;
        };
        match ran(answer) {
            Ran::Done { stdout, stderr } => {
                let text = [stdout.trim_end(), stderr.trim_end()]
                    .into_iter()
                    .filter(|part| !part.is_empty())
                    .collect::<Vec<_>>()
                    .join("\n");
                logs.lines = logic::log_lines(&text);
                logs.errors = (0..logs.lines.len())
                    .filter(|index| is_error_line(&logs.lines[*index]))
                    .collect();
                logs.current = (!logs.errors.is_empty()).then_some(0);
                logs.status = LogsStatus::Ok;
                logs.detail = String::new();
                if let Some(first) = logs.errors.first() {
                    cx.scroll_to_line(Self::logs_id(id), *first as u32);
                }
            }
            Ran::Failed(failure) => {
                logs.status = LogsStatus::Failed;
                logs.detail = if failure.detail.is_empty() {
                    failure.fault.as_str().to_owned()
                } else {
                    failure.detail
                };
            }
        }
    }
}

impl Kit for Services {
    type Message = Message;

    fn init(start: &Start, cx: &mut Context<Message>) -> Self {
        let mut tab = Services {
            t: logic::strings(&start.theme.locale),
            utc_offset: start.utc_offset,
            now: cx.now(),
            visible: start.visible,
            loaded: false,
            refreshing: false,
            updated_at: None,
            ports: PortsState {
                items: Vec::new(),
                notes: Vec::new(),
                health: Health::Ok,
            },
            docker: DockerState {
                items: Vec::new(),
                health: Health::Ok,
            },
            panes: None,
            panes_denied: false,
            show: Show::All,
            logs: None,
            ask: None,
            generation: 0,
            in_flight: InFlight::default(),
            asks: 0,
        };
        tab.refresh(cx);
        tab
    }

    fn update(&mut self, message: Message, cx: &mut Context<Message>) {
        self.now = cx.now();
        match message {
            Message::Refresh | Message::Tick => self.refresh(cx),
            Message::PortsRead(generation, answer) if generation == self.generation => {
                self.in_flight.ports = Some(answer);
                self.finish_refresh(cx);
            }
            Message::DockerRead(generation, answer) if generation == self.generation => {
                self.in_flight.docker = Some(answer);
                self.finish_refresh(cx);
            }
            Message::PanesRead(generation, answer) if generation == self.generation => {
                self.in_flight.panes = Some(answer);
                self.finish_refresh(cx);
            }
            Message::PortsRead(..) | Message::DockerRead(..) | Message::PanesRead(..) => {}
            Message::PanesChanged => cx.call(PanesList {}, Message::PanesListed),
            Message::PanesListed(answer) => (self.panes, self.panes_denied) = read_panes(answer),
            // A tab behind another does not read; shown again, it reads at once.
            Message::Visible(visible) => {
                if self.visible == visible {
                    return;
                }
                self.visible = visible;
                if !visible {
                    cx.cancel("refresh");
                } else if self.loaded && !self.refreshing {
                    self.refresh(cx);
                }
            }
            Message::Theme(theme) => self.t.set_locale(theme.locale),
            Message::Show(show) => self.show = show,
            Message::ToggleLogs(id) => {
                if !is_container_id(&id) {
                    return;
                }
                if self.logs.as_ref().is_some_and(|logs| logs.id == id) {
                    self.logs = None;
                    return;
                }
                self.logs = Some(Logs {
                    id: id.clone(),
                    status: LogsStatus::Loading,
                    lines: Vec::new(),
                    errors: Vec::new(),
                    current: None,
                    detail: String::new(),
                });
                let asked = id.clone();
                cx.call(command("docker-logs", &[("id", &id)]), move |answer| {
                    Message::LogsRead(asked, answer)
                });
            }
            Message::LogsRead(id, answer) => self.read_logs(&id, answer, cx),
            Message::Error(step) => {
                if let Some(logs) = self.logs.as_mut()
                    && let Some(current) = logs.current
                {
                    let count = logs.errors.len() as i32;
                    let next = (current as i32 + step).rem_euclid(count) as usize;
                    logs.current = Some(next);
                    cx.scroll_to_line(Self::logs_id(&logs.id), logs.errors[next] as u32);
                }
            }
            Message::CopyLogs => {
                if let Some(logs) = &self.logs {
                    cx.call(
                        CopyText {
                            text: logs.lines.join("\n"),
                        },
                        Message::Copied,
                    );
                }
            }
            Message::Copied(Ok(_)) => cx.toast(Notice::Success, self.t.t("copied")),
            Message::Copied(Err(error)) => cx.toast(Notice::Error, error.message),
            Message::Ask(key) => self.open_ask(key),
            // The person picked a pane: remembered, unless it is being sent.
            Message::Pick(name) => {
                if let Some(ask) = self
                    .ask
                    .as_mut()
                    .filter(|ask| ask.status != AskStatus::Sending)
                {
                    ask.pane = name;
                }
            }
            Message::CloseAsk => self.ask = None,
            Message::Send => {
                let Some(ask) = self.ask.as_ref() else {
                    return;
                };
                if ask.status == AskStatus::Sending || ask.pane.is_empty() {
                    return;
                }
                let (serial, pane) = (ask.serial, ask.pane.clone());
                let text = self.brief(&ask.target);
                if let Some(ask) = self.ask.as_mut() {
                    ask.status = AskStatus::Sending;
                }
                cx.call(PanesSend { pane, text }, move |answer| {
                    Message::Sent(serial, answer)
                });
            }
            Message::Sent(serial, answer) => {
                if let Some(ask) = self.ask.as_mut().filter(|ask| ask.serial == serial) {
                    match answer {
                        Ok(_) => ask.status = AskStatus::Sent,
                        Err(error) => {
                            ask.status = AskStatus::Failed;
                            ask.detail = if error.is(KitError::DENIED) {
                                "denied".to_owned()
                            } else {
                                error.message
                            };
                        }
                    }
                }
            }
        }
    }

    fn view(&self) -> Element<Message> {
        let t = &self.t;
        let mut header = header(t.t("tab")).action(
            // The one primary action of the page, unless a question is open:
            // then it is Send.
            {
                let reading = button(t.t(if self.refreshing {
                    "refreshing"
                } else {
                    "refresh"
                }))
                .on_press_maybe((!self.refreshing).then_some(Message::Refresh));
                if self.ask.is_some() {
                    reading
                } else {
                    reading.primary()
                }
            },
        );
        if let Some(at) = self.updated_at {
            header = header.state(
                text(t.tv(
                    "updated",
                    &[("time", &format::clock(at, self.utc_offset, true))],
                ))
                .small()
                .muted(),
            );
        }

        let mut children: Vec<Element<Message>> = vec![header.into()];
        let both_denied =
            self.ports.health == Health::Denied && self.docker.health == Health::Denied;
        if !self.loaded {
            children.push(section(t.t("ports"), vec![loading(t.t("loading")).into()]).into());
            children.push(section(t.t("containers"), Vec::new()).into());
        } else if both_denied {
            // Nothing can be read at all: one state says it, not two.
            children.push(self.no_permission().into());
        } else {
            children.push(self.ports_section().into());
            children.push(self.docker_section().into());
        }
        children.push(text(t.t("auto")).small().muted().into());
        page(children).into()
    }

    fn subscriptions(&self) -> Subscriptions<Message> {
        Subscriptions::none()
            .panes_changed(Message::PanesChanged)
            .visibility(Message::Visible)
            .theme(Message::Theme)
    }
}

// ---------------------------------------------------------------- The page.

type H = hangar_kit_sdk::Handler<Message>;
type EmptyState = Empty<H>;

impl Services {
    fn no_permission(&self) -> EmptyState {
        denied(self.t.t("deniedTitle")).text(self.t.t("deniedHint"))
    }

    fn try_again(&self) -> Button<H> {
        button(self.t.t("tryAgain")).on_press(Message::Refresh)
    }

    fn ports_section(&self) -> Section<H> {
        let t = &self.t;
        let rows = self.rows();
        let in_blocks = rows.iter().filter(|row| row.owner.is_some()).count();
        // A filter with nothing to filter by shows everything.
        let only_blocks = self.show == Show::Blocks && in_blocks > 0;

        let body: Vec<Element<Message>> = match &self.ports.health {
            Health::Denied => vec![self.no_permission().into()],
            Health::Error { problem, .. } => {
                let (title, hint) = match problem {
                    Problem::NoNode => ("errNoNodeTitle", "errNoNodeHint"),
                    Problem::NoTool => ("errNoToolTitle", "errNoToolHint"),
                    Problem::Timeout => ("errTimeoutTitle", "errTimeoutHint"),
                    Problem::Failed => ("errPortsTitle", "errPortsHint"),
                };
                vec![
                    failed(t.t(title))
                        .text(t.t(hint))
                        .action(self.try_again())
                        .into(),
                ]
            }
            _ if rows.is_empty() => vec![empty(t.t("noPorts")).text(t.t("noPortsHint")).into()],
            _ => {
                let shown = rows
                    .iter()
                    .filter(|row| !only_blocks || row.owner.is_some());
                let mut body: Vec<Element<Message>> =
                    vec![list(shown.map(|row| self.port_item(row))).into()];
                if self.ports.notes.iter().any(|note| note == "some-hidden") {
                    body.push(text(t.t("hiddenNote")).small().muted().into());
                }
                body
            }
        };

        let mut section = section(t.t("ports"), body);
        if self.ports.health == Health::Ok {
            section = section.count(rows.len().to_string());
            if in_blocks > 0 {
                section = section
                    .action(
                        filter(t.t("allPorts"), !only_blocks)
                            .count(rows.len().to_string())
                            .on_press(Message::Show(Show::All)),
                    )
                    .action(
                        filter(t.t("onlyBlocks"), only_blocks)
                            .count(in_blocks.to_string())
                            .on_press(Message::Show(Show::Blocks)),
                    );
            }
        }
        section
    }

    fn port_item(&self, row: &PortRow) -> ListItem<H> {
        let t = &self.t;
        let port = &row.port;
        let program = port.name.clone().unwrap_or_else(|| t.t("unknownProgram"));

        let mut facts = Vec::new();
        if let Some(pid) = port.pid {
            facts.push(t.tv("pid", &[("pid", &pid.to_string())]));
        }
        if let Some(at) = port.started_at {
            facts.push(t.tv("started", &[("since", &self.since(at))]));
        }
        facts.push(t.t(if port.loopback { "loopback" } else { "exposed" }));
        if !port.mine {
            facts.push(t.t("notYours"));
        }

        // The port first, the program after it; the whole of it on hover.
        let mut item = item(format!("{}  {}", port.port, program))
            .tooltip(
                format!("{} {}", port.port, port.name.as_deref().unwrap_or(""))
                    .trim()
                    .to_owned(),
            )
            .sub(facts.join(" · "))
            .wrap();
        if let Some(owner) = self.owner_line(row) {
            // A worker's name is said once, in the weight of a label.
            let worker = matches!(row.owner, Some(Owner::Worker { .. }));
            item =
                item.body(
                    text(owner)
                        .small()
                        .tone(if worker { Tone::Default } else { Tone::Dim }),
                );
        }
        let key = AskKey::Port {
            port: port.port,
            pid: port.pid,
        };
        // What is not the person's to stop is not offered for stopping.
        if port.mine {
            item = item.body(
                row![self.ask_button(&key, t.tv("portWhat", &[("port", &port.port.to_string())]))]
                    .spacing(4.0),
            );
        }
        if let Some(ask) = self.ask.as_ref().filter(|ask| ask.key == key) {
            item = item.body(self.ask_panel(ask));
        }
        item
    }

    fn owner_line(&self, row: &PortRow) -> Option<String> {
        let t = &self.t;
        match &row.owner {
            None => row
                .container
                .as_ref()
                .map(|name| t.tv("dockerPort", &[("name", name)])),
            Some(Owner::Worker { name, gone, .. }) => Some(t.tv(
                if *gone { "blockGone" } else { "blockOf" },
                &[("name", name)],
            )),
            Some(Owner::Block { base }) => Some(self.block_line(*base)),
        }
    }

    fn block_line(&self, base: u32) -> String {
        self.t.tv(
            "blockFree",
            &[
                ("from", &base.to_string()),
                ("to", &(base + BLOCK_SIZE - 1).to_string()),
            ],
        )
    }

    fn ask_button(&self, key: &AskKey, what: String) -> Button<H> {
        button(self.t.t("ask"))
            .small()
            .ghost()
            .tooltip(self.t.tv("askTitle", &[("what", &what)]))
            .on_press(Message::Ask(key.clone()))
    }

    fn docker_section(&self) -> Section<H> {
        let t = &self.t;
        let body: Vec<Element<Message>> = match &self.docker.health {
            Health::Denied => vec![self.no_permission().into()],
            Health::Absent => vec![
                empty(t.t("dockerAbsentTitle"))
                    .text(t.t("dockerAbsentHint"))
                    .icon(Icon::Info)
                    .into(),
            ],
            Health::Stopped => vec![
                empty(t.t("dockerStoppedTitle"))
                    .text(t.t("dockerStoppedHint"))
                    .icon(Icon::Info)
                    .action(self.try_again())
                    .into(),
            ],
            Health::Error { problem, detail } => {
                let timeout = *problem == Problem::Timeout;
                vec![
                    failed(t.t(if timeout {
                        "errTimeoutTitle"
                    } else {
                        "dockerFailedTitle"
                    }))
                    .text(if timeout {
                        t.t("errTimeoutHint")
                    } else if detail.is_empty() {
                        t.t("errPortsHint")
                    } else {
                        detail.clone()
                    })
                    .action(self.try_again())
                    .into(),
                ]
            }
            Health::Ok if self.docker.items.is_empty() => {
                vec![
                    empty(t.t("noContainers"))
                        .text(t.t("noContainersHint"))
                        .into(),
                ]
            }
            Health::Ok => vec![
                list(
                    self.docker
                        .items
                        .iter()
                        .map(|container| self.container_item(container)),
                )
                .into(),
            ],
        };
        let mut section = section(t.t("containers"), body);
        if self.docker.health == Health::Ok {
            section = section.count(self.docker.items.len().to_string());
        }
        section
    }

    fn container_item(&self, container: &Container) -> ListItem<H> {
        let t = &self.t;
        let state = container_state(container);
        let code = state.code.map(|code| code.to_string()).unwrap_or_default();
        let named = if container.state.is_empty() {
            "?"
        } else {
            &container.state
        };
        let word = t.tv(state.key, &[("code", &code), ("state", named)]);

        let published = if container.ports_text.is_empty() {
            t.t("noPublished")
        } else {
            container.ports_text.clone()
        };
        let mut blocks: Vec<u32> = Vec::new();
        for published in &container.ports {
            if let Some(base) = logic::block_of(published.host)
                && !blocks.contains(&base)
            {
                blocks.push(base);
            }
        }

        let mut item = item(container.name.clone())
            .lead(glyph(state.status, word.clone()))
            .tooltip(container.name.clone())
            .sub(
                [word.as_str(), container.image.as_str()]
                    .into_iter()
                    .filter(|part| !part.is_empty())
                    .collect::<Vec<_>>()
                    .join(" · "),
            )
            .wrap()
            .body(text(published).small().dim().mono());
        if !blocks.is_empty() {
            item = item.body(
                text(
                    blocks
                        .iter()
                        .map(|base| self.block_line(*base))
                        .collect::<Vec<_>>()
                        .join(" · "),
                )
                .small()
                .dim(),
            );
        }
        let logs_open = self
            .logs
            .as_ref()
            .is_some_and(|logs| logs.id == container.id);
        let key = AskKey::Container(container.id.clone());
        item = item.body(
            row![
                button(t.t(if logs_open { "hideLogs" } else { "logs" }))
                    .small()
                    .ghost()
                    .on_press(Message::ToggleLogs(container.id.clone())),
                self.ask_button(&key, t.tv("containerWhat", &[("name", &container.name)])),
            ]
            .spacing(4.0)
            .wrap(),
        );
        if let Some(logs) = self.logs.as_ref().filter(|logs| logs.id == container.id) {
            item = item.body(self.logs_block(logs));
        }
        if let Some(ask) = self.ask.as_ref().filter(|ask| ask.key == key) {
            item = item.body(self.ask_panel(ask));
        }
        item
    }

    /// A container's log, with the way between its errors.
    fn logs_block(&self, logs: &Logs) -> Element<Message> {
        let t = &self.t;
        match logs.status {
            LogsStatus::Loading => state_label(Status::Running, t.t("logsLoading")).into(),
            LogsStatus::Failed => banner(
                Notice::Error,
                t.tv(
                    "logsFailed",
                    &[(
                        "detail",
                        if logs.detail.is_empty() {
                            "?"
                        } else {
                            &logs.detail
                        },
                    )],
                ),
            )
            .into(),
            LogsStatus::Ok if logs.lines.is_empty() => {
                text(t.t("logsEmpty")).small().muted().into()
            }
            LogsStatus::Ok => {
                let id = Self::logs_id(&logs.id);
                let current = logs.current.map(|current| logs.errors[current]);
                let lines = logs.lines.iter().enumerate().map(|(index, line)| {
                    log_line(line.clone())
                        .number(index as u32 + 1)
                        .kind(if logs.errors.contains(&index) {
                            LineKind::Error
                        } else {
                            LineKind::Normal
                        })
                        .current(current == Some(index))
                });
                let mut log = log_lines::<Message>(lines).id(id).height(LOG_HEIGHT);
                if logs.errors.is_empty() {
                    // Without an error to look at, it opens on its end.
                    log = log.at_end();
                }

                let mut bar: Vec<Element<Message>> = Vec::new();
                if let Some(at) = logs.current {
                    let n = logs.errors.len();
                    bar.push(
                        text(if n > 1 {
                            t.tv(
                                "errorAt",
                                &[("at", &(at + 1).to_string()), ("n", &n.to_string())],
                            )
                        } else {
                            t.tn("errors", n as i64, &[])
                        })
                        .small()
                        .dim()
                        .into(),
                    );
                    bar.push(fill().into());
                    if n > 1 {
                        bar.push(
                            button(t.t("previous"))
                                .small()
                                .ghost()
                                .on_press(Message::Error(-1))
                                .into(),
                        );
                        bar.push(
                            button(t.t("next"))
                                .small()
                                .ghost()
                                .on_press(Message::Error(1))
                                .into(),
                        );
                    }
                } else {
                    bar.push(fill().into());
                }
                bar.push(
                    button(t.t("copyLogs"))
                        .small()
                        .ghost()
                        .tooltip(t.t("copyLogsTitle"))
                        .on_press(Message::CopyLogs)
                        .into(),
                );

                col![
                    row(bar).spacing(4.0).align(Align::Center),
                    log,
                    text(t.tv("logsTail", &[("n", &logic::LOG_LINES.to_string())]))
                        .small()
                        .muted(),
                ]
                .spacing(4.0)
                .into()
            }
        }
    }

    /// The question under a row: whom to send to, and the brief as it will go.
    fn ask_panel(&self, ask: &Ask) -> Element<Message> {
        let t = &self.t;
        let what = match &ask.target {
            Target::Container(container) => t.tv("containerWhat", &[("name", &container.name)]),
            Target::Port(row) => t.tv("portWhat", &[("port", &row.port.port.to_string())]),
        };
        let mut parts: Vec<Element<Message>> = vec![
            text(t.tv("askTitle", &[("what", &what)]))
                .weight(Weight::Semibold)
                .into(),
        ];
        let cancel = || {
            button(t.t("askCancel"))
                .small()
                .ghost()
                .on_press(Message::CloseAsk)
        };

        let sendable = self.sendable();
        if self.panes_denied {
            parts.push(banner(Notice::Error, t.t("askDenied")).into());
            parts.push(row![cancel()].into());
        } else if self.panes.is_none() {
            parts.push(banner(Notice::Error, t.t("askNoPanes")).into());
            parts.push(row![cancel()].into());
        } else if sendable.is_empty() {
            parts.push(banner(Notice::Warning, t.t("askNone")).into());
            parts.push(row![cancel()].into());
        } else {
            let options = std::iter::once(select_option("", "—")).chain(
                sendable
                    .iter()
                    .map(|pane| select_option(pane.name.clone(), pane.name.clone())),
            );
            parts.push(
                select::<Message>(options, Some(ask.pane.clone()))
                    .label(t.t("askTo"))
                    .on_select(Message::Pick)
                    .into(),
            );
            parts.push(pre(self.brief(&ask.target)).height(160.0).into());
            match ask.status {
                AskStatus::Sent => {
                    parts
                        .push(banner(Notice::Info, t.tv("askSent", &[("pane", &ask.pane)])).into());
                }
                AskStatus::Failed => {
                    parts.push(
                        banner(
                            Notice::Error,
                            if ask.detail == "denied" {
                                t.t("askDenied")
                            } else {
                                t.tv("askFailed", &[("detail", &ask.detail)])
                            },
                        )
                        .into(),
                    );
                }
                AskStatus::Idle | AskStatus::Sending => {}
            }
            let can_send = ask.status == AskStatus::Idle || ask.status == AskStatus::Failed;
            parts.push(
                row![
                    button(t.t("askSend")).small().primary().on_press_maybe(
                        (can_send && !ask.pane.is_empty()).then_some(Message::Send)
                    ),
                    cancel(),
                ]
                .spacing(4.0)
                .into(),
            );
        }
        container(column(parts).spacing(8.0))
            .width(Length::Fill)
            .surface(Surface::Well)
            .radius(6.0)
            .padding(10.0)
            .into()
    }
}

export_kit!(Services);

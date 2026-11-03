//! What the Services tab knows, apart from Hangar and from the page: the
//! tools' answers read into rows, which worker a port belongs to, what a
//! failure means, the brief an agent is sent, and the tab's words in English
//! and French (the glossary of Hangar's own strings: tu, panneau, workspace,
//! Settings).
//!
//! This is `tabs/services/logic.js` in Rust. Nothing here calls Hangar or
//! draws; it takes text and answers data, so the tests read it directly.

use std::cmp::Ordering;

use hangar_kit_sdk::api::Pane;
use hangar_kit_sdk::format;
use hangar_kit_sdk::i18n::Strings;
use hangar_kit_sdk::prelude::Status;
use serde_json::Value;

/// The pool every worker's block comes from (Hangar's workerControl): 21000,
/// 21020…
pub const POOL_START: u32 = 21_000;
pub const POOL_END: u32 = 26_999;
pub const BLOCK_SIZE: u32 = 20;
/// How many lines of a container's log are kept (`docker logs --tail 200`).
pub const LOG_LINES: usize = 200;

/// What `docker logs` is asked for: the id `docker ps` prints, in full or
/// short, `[0-9a-f]{12,64}`. Never an option.
pub fn is_container_id(id: &str) -> bool {
    (12..=64).contains(&id.len()) && id.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

// ---------------------------------------------------------------- Words.

/// English, the language every other one falls back to.
pub const EN: &[(&str, &str)] = &[
    ("tab", "Services"),
    ("refresh", "Refresh"),
    ("refreshing", "Reading…"),
    ("updated", "Updated {time}"),
    ("auto", "Refreshes every 10 s while this tab is shown."),
    ("ports", "Ports"),
    ("containers", "Containers"),
    ("onlyBlocks", "Worker ports"),
    ("allPorts", "All"),
    ("loading", "Reading ports and containers…"),
    ("noPorts", "Nothing is listening"),
    (
        "noPortsHint",
        "No program is waiting for a connection on this computer. A server started later shows here.",
    ),
    ("noContainers", "No container"),
    (
        "noContainersHint",
        "Docker answers, and has no container, running or stopped.",
    ),
    ("pid", "pid {pid}"),
    ("started", "started {since}"),
    ("loopback", "local only"),
    ("exposed", "reachable from the network"),
    ("unknownProgram", "Unknown program"),
    ("notYours", "the system's, or another account's"),
    (
        "hiddenNote",
        "Some programs belong to another account: their name is not shown.",
    ),
    ("blockOf", "Port block of {name}"),
    ("blockGone", "Port block of {name}, whose pane is closed"),
    ("blockFree", "Worker port block {from}–{to}"),
    ("dockerPort", "Container {name}"),
    ("ask", "Ask an agent to stop this"),
    ("askTitle", "Ask an agent to stop {what}"),
    ("askTo", "Send to"),
    (
        "askNone",
        "No agent can be reached. Open a Claude Code pane in this workspace, then try again.",
    ),
    ("askSend", "Send"),
    ("askCancel", "Cancel"),
    ("askSent", "Sent to {pane}. It acts when it is free."),
    ("askFailed", "Not sent: {detail}"),
    (
        "askDenied",
        "This tab may not send to a pane. Accept panes.send in Settings › Kits.",
    ),
    ("askNoPanes", "The panes could not be listed."),
    ("portWhat", "port {port}"),
    ("containerWhat", "container {name}"),
    (
        "briefPort",
        "Please stop what listens on port {port}: {program}, pid {pid}{since}. {owner}Check that it is yours and that nothing needs it, then stop it cleanly (not by killing other programs) and tell me what you stopped. If it is not yours, leave it and tell me.",
    ),
    (
        "briefContainer",
        "Please stop the Docker container {name} (id {id}, image {image}, {status}). Check that it is yours and that nothing needs it, then stop it with docker, and tell me what you stopped. If it is not yours, leave it and tell me.",
    ),
    ("briefOwnerWorker", "It was started in the pane of {name}. "),
    (
        "briefOwnerBlock",
        "It sits in the worker port block {from}–{to}. ",
    ),
    ("logs", "Logs"),
    ("hideLogs", "Hide logs"),
    ("logsLoading", "Reading the logs…"),
    ("logsEmpty", "This container has written nothing."),
    ("logsFailed", "The logs could not be read: {detail}"),
    ("logsTail", "The last {n} lines."),
    ("noPublished", "No published port"),
    ("state_running", "Running"),
    ("state_paused", "Paused"),
    ("state_restarting", "Restarting"),
    ("state_created", "Created"),
    ("state_exited", "Stopped"),
    ("state_exited_code", "Stopped ({code})"),
    ("state_dead", "Dead"),
    ("state_other", "{state}"),
    ("errPortsTitle", "The ports could not be listed"),
    (
        "errPortsHint",
        "The listing tool did not answer. Try again.",
    ),
    ("errNoNodeTitle", "Node is not installed"),
    (
        "errNoNodeHint",
        "This tab lists the ports with a small Node script. Install Node, then try again.",
    ),
    ("errNoToolTitle", "No tool lists the ports here"),
    (
        "errNoToolHint",
        "It needs PowerShell on Windows, lsof on macOS, ss or lsof on Linux. Install one, then try again.",
    ),
    ("errTimeoutTitle", "It took too long"),
    (
        "errTimeoutHint",
        "The system did not answer in time. Try again.",
    ),
    ("dockerAbsentTitle", "Docker is not installed"),
    (
        "dockerAbsentHint",
        "Containers show here once the docker command is on your PATH. The ports above do not need it.",
    ),
    ("dockerStoppedTitle", "Docker is not running"),
    (
        "dockerStoppedHint",
        "Start Docker Desktop or the Docker service, then refresh. The ports above do not need it.",
    ),
    ("dockerFailedTitle", "Docker answered with an error"),
    ("deniedTitle", "This tab may not read this"),
    (
        "deniedHint",
        "Accept the commands of Services in Settings › Kits, then reload the tab.",
    ),
    ("tryAgain", "Try again"),
    // What the page's log widget said by itself, and its copy button.
    ("errors_one", "{n} error"),
    ("errors_other", "{n} errors"),
    ("errorAt", "Error {at} of {n}"),
    ("previous", "Previous"),
    ("next", "Next"),
    ("copyLogs", "Copy"),
    ("copyLogsTitle", "Copy the logs"),
    ("copied", "Copied"),
];

/// French, in Hangar's own register: tu, panneau, workspace, Réglages.
pub const FR: &[(&str, &str)] = &[
    ("tab", "Services"),
    ("refresh", "Actualiser"),
    ("refreshing", "Lecture…"),
    ("updated", "Actualisé {time}"),
    (
        "auto",
        "S'actualise toutes les 10 s tant que cet onglet est affiché.",
    ),
    ("ports", "Ports"),
    ("containers", "Conteneurs"),
    ("onlyBlocks", "Ports des workers"),
    ("allPorts", "Tous"),
    ("loading", "Lecture des ports et des conteneurs…"),
    ("noPorts", "Rien n'écoute"),
    (
        "noPortsHint",
        "Aucun programme n'attend de connexion sur cet ordinateur. Un serveur lancé plus tard apparaît ici.",
    ),
    ("noContainers", "Aucun conteneur"),
    (
        "noContainersHint",
        "Docker répond, et n'a aucun conteneur, démarré ou arrêté.",
    ),
    ("pid", "pid {pid}"),
    ("started", "lancé {since}"),
    ("loopback", "local uniquement"),
    ("exposed", "joignable depuis le réseau"),
    ("unknownProgram", "Programme inconnu"),
    ("notYours", "du système ou d'un autre compte"),
    (
        "hiddenNote",
        "Certains programmes appartiennent à un autre compte : leur nom n'est pas affiché.",
    ),
    ("blockOf", "Bloc de ports de {name}"),
    (
        "blockGone",
        "Bloc de ports de {name}, dont le panneau est fermé",
    ),
    ("blockFree", "Bloc de ports d'un worker {from}–{to}"),
    ("dockerPort", "Conteneur {name}"),
    ("ask", "Demander à un agent d'arrêter ça"),
    ("askTitle", "Demander à un agent d'arrêter {what}"),
    ("askTo", "Envoyer à"),
    (
        "askNone",
        "Aucun agent n'est joignable. Ouvre un panneau Claude Code dans ce workspace, puis réessaie.",
    ),
    ("askSend", "Envoyer"),
    ("askCancel", "Annuler"),
    ("askSent", "Envoyé à {pane}. Il agit quand il est libre."),
    ("askFailed", "Non envoyé : {detail}"),
    (
        "askDenied",
        "Cet onglet ne peut pas écrire à un panneau. Accepte panes.send dans Réglages › Kits.",
    ),
    ("askNoPanes", "Les panneaux n'ont pas pu être listés."),
    ("portWhat", "le port {port}"),
    ("containerWhat", "le conteneur {name}"),
    (
        "briefPort",
        "Arrête ce qui écoute sur le port {port} : {program}, pid {pid}{since}. {owner}Vérifie que c'est à toi et que rien n'en a besoin, puis arrête-le proprement (sans tuer d'autres programmes) et dis-moi ce que tu as arrêté. Si ce n'est pas à toi, n'y touche pas et dis-le-moi.",
    ),
    (
        "briefContainer",
        "Arrête le conteneur Docker {name} (id {id}, image {image}, {status}). Vérifie que c'est à toi et que rien n'en a besoin, puis arrête-le avec docker et dis-moi ce que tu as arrêté. Si ce n'est pas à toi, n'y touche pas et dis-le-moi.",
    ),
    (
        "briefOwnerWorker",
        "Il a été lancé dans le panneau de {name}. ",
    ),
    (
        "briefOwnerBlock",
        "Il est dans le bloc de ports de worker {from}–{to}. ",
    ),
    ("logs", "Journal"),
    ("hideLogs", "Masquer le journal"),
    ("logsLoading", "Lecture du journal…"),
    ("logsEmpty", "Ce conteneur n'a rien écrit."),
    ("logsFailed", "Le journal n'a pas pu être lu : {detail}"),
    ("logsTail", "Les {n} dernières lignes."),
    ("noPublished", "Aucun port publié"),
    ("state_running", "En marche"),
    ("state_paused", "En pause"),
    ("state_restarting", "Redémarre"),
    ("state_created", "Créé"),
    ("state_exited", "Arrêté"),
    ("state_exited_code", "Arrêté ({code})"),
    ("state_dead", "Mort"),
    ("state_other", "{state}"),
    ("errPortsTitle", "Les ports n'ont pas pu être listés"),
    (
        "errPortsHint",
        "L'outil de liste n'a pas répondu. Réessaie.",
    ),
    ("errNoNodeTitle", "Node n'est pas installé"),
    (
        "errNoNodeHint",
        "Cet onglet liste les ports avec un petit script Node. Installe Node, puis réessaie.",
    ),
    ("errNoToolTitle", "Aucun outil ne liste les ports ici"),
    (
        "errNoToolHint",
        "Il faut PowerShell sous Windows, lsof sous macOS, ss ou lsof sous Linux. Installe-en un, puis réessaie.",
    ),
    ("errTimeoutTitle", "C'était trop long"),
    (
        "errTimeoutHint",
        "Le système n'a pas répondu à temps. Réessaie.",
    ),
    ("dockerAbsentTitle", "Docker n'est pas installé"),
    (
        "dockerAbsentHint",
        "Les conteneurs apparaissent ici dès que la commande docker est dans ton PATH. Les ports ci-dessus n'en ont pas besoin.",
    ),
    ("dockerStoppedTitle", "Docker n'est pas démarré"),
    (
        "dockerStoppedHint",
        "Démarre Docker Desktop ou le service Docker, puis actualise. Les ports ci-dessus n'en ont pas besoin.",
    ),
    ("dockerFailedTitle", "Docker a répondu par une erreur"),
    ("deniedTitle", "Cet onglet n'a pas le droit de lire ça"),
    (
        "deniedHint",
        "Accepte les commandes de Services dans Réglages › Kits, puis recharge l'onglet.",
    ),
    ("tryAgain", "Réessayer"),
    ("errors_one", "{n} erreur"),
    ("errors_other", "{n} erreurs"),
    ("errorAt", "Erreur {at} sur {n}"),
    ("previous", "Précédente"),
    ("next", "Suivante"),
    ("copyLogs", "Copier"),
    ("copyLogsTitle", "Copier le journal"),
    ("copied", "Copié"),
];

/// The tab's words, spoken in `locale`.
pub fn strings(locale: &str) -> Strings {
    Strings::new(locale).add("en", EN).add("fr", FR)
}

// ---------------------------------------------------------------- Text.

/// Terminal colour codes and window titles out of a text: `ESC [ … m` and
/// `ESC ] … BEL`.
pub fn strip_ansi(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut without_colours = String::with_capacity(text.len());
    let mut at = 0;
    while at < chars.len() {
        if chars[at] == '\u{1b}' && chars.get(at + 1) == Some(&'[') {
            let mut end = at + 2;
            while end < chars.len()
                && (chars[end].is_ascii_digit() || matches!(chars[end], ';' | '?'))
            {
                end += 1;
            }
            while end < chars.len() && (' '..='/').contains(&chars[end]) {
                end += 1;
            }
            if end < chars.len() && ('@'..='~').contains(&chars[end]) {
                at = end + 1;
                continue;
            }
        }
        without_colours.push(chars[at]);
        at += 1;
    }

    let chars: Vec<char> = without_colours.chars().collect();
    let mut out = String::with_capacity(without_colours.len());
    let mut at = 0;
    while at < chars.len() {
        if chars[at] == '\u{1b}'
            && chars.get(at + 1) == Some(&']')
            && let Some(bell) = chars[at + 2..].iter().position(|c| *c == '\u{7}')
        {
            at += 2 + bell + 1;
            continue;
        }
        out.push(chars[at]);
        at += 1;
    }
    out
}

/// A container's log as lines: ANSI gone, no empty tail, the last
/// [`LOG_LINES`].
pub fn log_lines(text: &str) -> Vec<String> {
    let text = strip_ansi(text);
    let segments: Vec<&str> = text.split('\n').collect();
    let last = segments.len() - 1;
    let mut lines: Vec<String> = segments
        .iter()
        .enumerate()
        .map(|(index, line)| match line.strip_suffix('\r') {
            Some(line) if index < last => line.to_owned(),
            _ => (*line).to_owned(),
        })
        .collect();
    while lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    let skip = lines.len().saturating_sub(LOG_LINES);
    lines.split_off(skip)
}

/// Whether a log line reads as an error: the words of one, in English and in
/// French, and not those of "0 errors" or `handles_failure_mode`. A guess, as
/// Hangar UI's `isErrorLine` is, and the same one.
pub fn is_error_line(text: &str) -> bool {
    let lower: Vec<char> = text.to_lowercase().chars().collect();
    error_word(&lower) && !no_error(&lower)
}

/// `errors?|failed|failures?|fatal|panic|panicked|exception|erreurs?|échecs?|
/// échouée?s?`, longest first.
const ERROR_WORDS: &[&str] = &[
    "errors",
    "error",
    "failed",
    "failures",
    "failure",
    "fatal",
    "panicked",
    "panic",
    "exception",
    "erreurs",
    "erreur",
    "échecs",
    "échec",
    "échouées",
    "échouée",
    "échoués",
    "échoué",
];

fn starts_with(chars: &[char], word: &str) -> bool {
    let mut at = chars.iter();
    word.chars().all(|c| at.next() == Some(&c))
}

fn error_word(chars: &[char]) -> bool {
    (0..chars.len()).any(|start| {
        // A word alone: not part of a name like `handles_failure_mode`.
        let before_ok = start == 0 || {
            let before = chars[start - 1];
            !(before.is_alphanumeric() || matches!(before, '-' | '_' | '/' | '\\' | '.'))
        };
        before_ok
            && ERROR_WORDS.iter().any(|word| {
                if !starts_with(&chars[start..], word) {
                    return false;
                }
                let end = start + word.chars().count();
                let ends_ok = chars
                    .get(end)
                    .is_none_or(|next| !(next.is_alphanumeric() || matches!(next, '-' | '_')));
                ends_ok && !zero_follows(&chars[end..])
            })
    })
}

/// `\s*[:=]\s*0\b`: "errors: 0", "failed=0".
fn zero_follows(chars: &[char]) -> bool {
    let mut at = 0;
    while chars.get(at).is_some_and(|c| c.is_whitespace()) {
        at += 1;
    }
    if !matches!(chars.get(at), Some(':' | '=')) {
        return false;
    }
    at += 1;
    while chars.get(at).is_some_and(|c| c.is_whitespace()) {
        at += 1;
    }
    chars.get(at) == Some(&'0')
        && chars
            .get(at + 1)
            .is_none_or(|next| !(next.is_ascii_alphanumeric() || *next == '_'))
}

/// `(0|no|zero|without|sans|aucune?) (errors?|failures?|failed|erreurs?|échecs?)`.
fn no_error(chars: &[char]) -> bool {
    const NONE: &[&str] = &["aucune", "aucun", "without", "zero", "sans", "no", "0"];
    const NOUNS: &[&str] = &[
        "errors", "error", "failures", "failure", "failed", "erreurs", "erreur", "échecs", "échec",
    ];
    (0..chars.len()).any(|start| {
        if start > 0 && chars[start - 1].is_alphanumeric() {
            return false;
        }
        NONE.iter().any(|none| {
            if !starts_with(&chars[start..], none) {
                return false;
            }
            let mut at = start + none.chars().count();
            let spaces = chars[at..].iter().take_while(|c| c.is_whitespace()).count();
            if spaces == 0 {
                return false;
            }
            at += spaces;
            NOUNS.iter().any(|noun| {
                starts_with(&chars[at..], noun)
                    && chars
                        .get(at + noun.chars().count())
                        .is_none_or(|next| !next.is_alphanumeric())
            })
        })
    })
}

// ---------------------------------------------------------------- Ports.

/// The first port of the worker block `port` falls in, or none for a port
/// outside the pool.
pub fn block_of(port: u32) -> Option<u32> {
    (POOL_START..=POOL_END)
        .contains(&port)
        .then(|| POOL_START + (port - POOL_START) / BLOCK_SIZE * BLOCK_SIZE)
}

/// One listening port, as `tools/ports.js` printed it and the tab uses it.
#[derive(Debug, Clone, PartialEq)]
pub struct Port {
    pub port: u32,
    pub pid: Option<u32>,
    pub name: Option<String>,
    /// Milliseconds since the Unix epoch.
    pub started_at: Option<i64>,
    pub loopback: bool,
    /// The program is the person's to have stopped; only a clear yes counts.
    pub mine: bool,
    pub port_base: Option<u32>,
    pub pane: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedPorts {
    pub ports: Vec<Port>,
    pub notes: Vec<String>,
}

/// Why `ports.js` printed nothing to list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortsError {
    /// Not JSON, or not the shape of an answer.
    Unreadable,
    /// The script said so itself: `no-tool`, `timeout`.
    Tool(String),
}

fn integer(value: Option<&Value>) -> Option<i64> {
    let value = value?;
    value.as_i64().or_else(|| {
        value
            .as_f64()
            .filter(|number| number.fract() == 0.0 && number.abs() < 9e15)
            .map(|number| number as i64)
    })
}

fn number(value: Option<&Value>) -> Option<u32> {
    integer(value).and_then(|number| u32::try_from(number).ok())
}

fn nonempty(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

/// What `node tools/ports.js` printed → the ports and the notes, or why not.
pub fn parse_ports(stdout: &str) -> Result<ParsedPorts, PortsError> {
    let data: Value = serde_json::from_str(stdout.trim()).map_err(|_| PortsError::Unreadable)?;
    if !data.is_object() && !data.is_array() {
        return Err(PortsError::Unreadable);
    }
    if let Some(error) = data.get("error") {
        let said = match error {
            Value::Null | Value::Bool(false) => None,
            Value::String(text) if text.is_empty() => None,
            Value::Number(n) if n.as_f64() == Some(0.0) => None,
            Value::String(text) => Some(text.clone()),
            other => Some(other.to_string()),
        };
        if let Some(said) = said {
            return Err(PortsError::Tool(said));
        }
    }
    let entries = data
        .get("ports")
        .and_then(Value::as_array)
        .ok_or(PortsError::Unreadable)?;
    let mut ports: Vec<Port> = entries
        .iter()
        .filter_map(|entry| {
            Some(Port {
                port: number(entry.get("port"))?,
                pid: number(entry.get("pid")),
                name: nonempty(entry.get("name")),
                started_at: entry
                    .get("startedAt")
                    .and_then(Value::as_str)
                    .and_then(format::parse_iso),
                loopback: entry.get("loopback") == Some(&Value::Bool(true)),
                // Only a clear yes: what cannot be told is not offered for stopping.
                mine: entry.get("mine") == Some(&Value::Bool(true)),
                port_base: number(entry.get("portBase")),
                pane: nonempty(entry.get("pane")),
            })
        })
        .collect();
    ports.sort_by(|a, b| {
        a.port
            .cmp(&b.port)
            .then(a.pid.unwrap_or(0).cmp(&b.pid.unwrap_or(0)))
    });
    let notes = data
        .get("notes")
        .and_then(Value::as_array)
        .map(|notes| {
            notes
                .iter()
                .filter_map(|note| note.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    Ok(ParsedPorts { ports, notes })
}

// ---------------------------------------------------------------- Docker.

/// A host port a container publishes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Published {
    pub host: u32,
    pub container: u32,
    pub proto: String,
}

/// How far a published range is followed: a wild one is cut.
const RANGE_MAX: u32 = 100;

/// `0.0.0.0:5432->5432/tcp, :::5432->5432/tcp` → the host ports, without the
/// IPv6 twins; `21100-21102->3000-3002/tcp` is three of them.
pub fn published_ports(text: &str) -> Vec<Published> {
    let mut seen: Vec<(u32, String)> = Vec::new();
    let mut out = Vec::new();
    for part in text.split(',') {
        let Some((first, last, container, proto)) = published_part(part) else {
            continue;
        };
        for host in first..=last.min(first + RANGE_MAX - 1) {
            if seen
                .iter()
                .any(|(seen_host, seen_proto)| *seen_host == host && *seen_proto == proto)
            {
                continue;
            }
            seen.push((host, proto.clone()));
            out.push(Published {
                host,
                container: container + (host - first),
                proto: proto.clone(),
            });
        }
    }
    out
}

/// `(\d{1,5})(?:-(\d{1,5}))?->(\d{1,5})(?:-\d{1,5})?/(\w+)`, found anywhere in
/// the part: the host range, the container's first port, the protocol.
fn published_part(part: &str) -> Option<(u32, u32, u32, String)> {
    let arrow = part.find("->")?;
    let (left, right) = (&part[..arrow], &part[arrow + 2..]);

    let digits_at_end = |text: &str| -> usize {
        text.bytes()
            .rev()
            .take_while(u8::is_ascii_digit)
            .count()
            .min(5)
    };
    // The host side: `21100`, or `21100-21102` just before the arrow.
    let len = digits_at_end(left);
    if len == 0 {
        return None;
    }
    let last: u32 = left[left.len() - len..].parse().ok()?;
    let before = &left[..left.len() - len];
    let first = match before.strip_suffix('-') {
        Some(range) if digits_at_end(range) > 0 => {
            let len = digits_at_end(range);
            range[range.len() - len..].parse().ok()?
        }
        _ => last,
    };

    // The container side: `3000`, or `3000-3002` (only the first counts),
    // then `/tcp`.
    let digits = right.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 || digits > 5 {
        return None;
    }
    let container: u32 = right[..digits].parse().ok()?;
    let mut rest = &right[digits..];
    if let Some(range) = rest.strip_prefix('-') {
        let more = range.bytes().take_while(u8::is_ascii_digit).count();
        if (1..=5).contains(&more) {
            rest = &range[more..];
        }
    }
    let proto: String = rest
        .strip_prefix('/')?
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    if proto.is_empty() {
        return None;
    }
    Some((first, last, container, proto))
}

/// One container of `docker ps -a`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Container {
    pub id: String,
    pub name: String,
    pub image: String,
    /// Lower case: `running`, `exited`…
    pub state: String,
    /// Docker's own words: `Exited (1) 12 minutes ago`.
    pub status: String,
    pub exit_code: Option<i32>,
    pub ports: Vec<Published>,
    pub ports_text: String,
}

/// `docker ps -a --format json`: one object per line (a JSON array is read as
/// well). None when it cannot be read; an empty text is no container.
pub fn parse_docker(stdout: &str) -> Option<Vec<Container>> {
    let text = stdout.trim();
    if text.is_empty() {
        return Some(Vec::new());
    }
    let items: Vec<Value> = if text.starts_with('[') {
        serde_json::from_str(text).ok()?
    } else {
        let mut items = Vec::new();
        for line in text.lines() {
            if line.trim().is_empty() {
                continue;
            }
            items.push(serde_json::from_str(line).ok()?);
        }
        items
    };
    let field = |item: &Value, name: &str| -> String {
        match item.get(name) {
            Some(Value::String(text)) => text.clone(),
            Some(Value::Number(n)) => n.to_string(),
            _ => String::new(),
        }
    };
    let mut containers: Vec<Container> = items
        .iter()
        .filter(|item| item.is_object())
        .filter(|item| is_container_id(&field(item, "ID")))
        .map(|item| {
            let id = field(item, "ID");
            let status = field(item, "Status");
            let names = field(item, "Names");
            let name = if names.is_empty() { id.clone() } else { names };
            let ports_text = field(item, "Ports");
            Container {
                name: name.split(',').next().unwrap_or_default().to_owned(),
                image: field(item, "Image"),
                state: field(item, "State").to_lowercase(),
                exit_code: exit_code(&status),
                ports: published_ports(&ports_text),
                ports_text,
                status,
                id,
            }
        })
        .collect();
    containers.sort_by(|a, b| {
        rank(a)
            .cmp(&rank(b))
            .then_with(|| locale_cmp(&a.name, &b.name))
    });
    Some(containers)
}

/// `Exited (1) 12 minutes ago` → 1.
fn exit_code(status: &str) -> Option<i32> {
    let inside = status.strip_prefix("Exited (")?;
    let end = inside.find(')')?;
    inside[..end].parse().ok()
}

/// What to show first: the running, then the trouble, the stopped last.
fn rank(container: &Container) -> u8 {
    match container.state.as_str() {
        "running" => 0,
        "restarting" => 1,
        "dead" => 2,
        "paused" => 3,
        "created" => 4,
        "exited" if container.exit_code.is_some_and(|code| code != 0) => 2,
        "exited" => 5,
        _ => 6,
    }
}

/// Names in the order a person reads them: without regard to case, a lower
/// case letter before its capital.
fn locale_cmp(a: &str, b: &str) -> Ordering {
    a.to_lowercase()
        .cmp(&b.to_lowercase())
        .then_with(|| b.cmp(a))
}

/// The glyph and the word of a container's state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateWord {
    pub status: Status,
    /// The key in [`EN`] and [`FR`].
    pub key: &'static str,
    pub code: Option<i32>,
}

pub fn container_state(container: &Container) -> StateWord {
    let word = |status, key| StateWord {
        status,
        key,
        code: None,
    };
    match container.state.as_str() {
        "running" => word(Status::Success, "state_running"),
        "restarting" => word(Status::Running, "state_restarting"),
        "paused" => word(Status::Waiting, "state_paused"),
        "created" => word(Status::Waiting, "state_created"),
        "exited" => match container.exit_code {
            Some(code) if code != 0 => StateWord {
                status: Status::Failure,
                key: "state_exited_code",
                code: Some(code),
            },
            _ => word(Status::Neutral, "state_exited"),
        },
        "dead" => word(Status::Failure, "state_dead"),
        _ => word(Status::Neutral, "state_other"),
    }
}

/// `docker ps` failing: is the engine off, or is it another trouble?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockerFault {
    Stopped,
    Failed,
}

pub fn docker_fault(text: &str) -> DockerFault {
    const OFF: &[&str] = &[
        "cannot connect to the docker daemon",
        "failed to connect to the docker api",
        "error during connect",
        "is the docker daemon running",
        "docker daemon is not running",
        "pipe/docker",
        "docker.sock",
    ];
    let text = text.to_lowercase();
    if OFF.iter().any(|sign| text.contains(sign)) {
        DockerFault::Stopped
    } else {
        DockerFault::Failed
    }
}

// ---------------------------------------------------------------- Whose.

/// Whose a port is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Owner {
    /// The program carries its pane's variables. `gone`: that pane is not
    /// among the panes any more, so what listens is a leftover.
    Worker {
        name: String,
        gone: bool,
        base: Option<u32>,
    },
    /// Only the port says so: it sits in this worker block.
    Block { base: u32 },
}

/// A program that carries its pane's variables names it (Linux, macOS); any
/// other port in the pool belongs to a block. `panes` is `panes.list`, or none
/// when it could not be read: then nothing is claimed gone.
pub fn owner_of(port: &Port, panes: Option<&[Pane]>) -> Option<Owner> {
    if let Some(pane) = &port.pane {
        let known = panes.and_then(|panes| {
            panes
                .iter()
                .find(|known| known.name.to_lowercase() == pane.to_lowercase())
        });
        return Some(Owner::Worker {
            name: known.map_or_else(|| pane.clone(), |known| known.name.clone()),
            gone: panes.is_some() && known.is_none(),
            base: port.port_base,
        });
    }
    block_of(port.port).map(|base| Owner::Block { base })
}

/// A row of the port list: the port, whose it is, and the container that
/// publishes it, if any.
#[derive(Debug, Clone, PartialEq)]
pub struct PortRow {
    pub port: Port,
    pub owner: Option<Owner>,
    /// The name of the container publishing this port.
    pub container: Option<String>,
}

pub fn port_rows(ports: &[Port], panes: Option<&[Pane]>, containers: &[Container]) -> Vec<PortRow> {
    ports
        .iter()
        .map(|port| PortRow {
            owner: owner_of(port, panes),
            // The last to publish a port is the one named.
            container: containers
                .iter()
                .rev()
                .find(|container| {
                    container
                        .ports
                        .iter()
                        .any(|published| published.host == port.port)
                })
                .map(|container| container.name.clone()),
            port: port.clone(),
        })
        .collect()
}

// ---------------------------------------------------------------- The brief.

/// What the person asks an agent to stop.
#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    Port(PortRow),
    Container(Container),
}

/// The brief an agent is sent to stop a port or a container: it checks first,
/// and says what it did. `since` says when a time was, in words. The spaces
/// Hangar sets between a number and its unit are plain here: an agent reads
/// text, not typography.
pub fn brief_for(target: &Target, t: &Strings, since: &dyn Fn(i64) -> String) -> String {
    let brief = match target {
        Target::Container(container) => t.tv(
            "briefContainer",
            &[
                ("name", &container.name),
                ("id", container.id.get(..12).unwrap_or(&container.id)),
                (
                    "image",
                    if container.image.is_empty() {
                        "?"
                    } else {
                        &container.image
                    },
                ),
                (
                    "status",
                    if container.status.is_empty() {
                        &container.state
                    } else {
                        &container.status
                    },
                ),
            ],
        ),
        Target::Port(row) => {
            let owner = match &row.owner {
                Some(Owner::Worker { name, .. }) => t.tv("briefOwnerWorker", &[("name", name)]),
                Some(Owner::Block { base }) => t.tv(
                    "briefOwnerBlock",
                    &[
                        ("from", &base.to_string()),
                        ("to", &(base + BLOCK_SIZE - 1).to_string()),
                    ],
                ),
                None => String::new(),
            };
            let started = row
                .port
                .started_at
                .map(|at| format!(", {}", t.tv("started", &[("since", &since(at))])))
                .unwrap_or_default();
            let program = row
                .port
                .name
                .clone()
                .unwrap_or_else(|| t.t("unknownProgram"));
            let pid = row
                .port
                .pid
                .map_or_else(|| "?".to_owned(), |pid| pid.to_string());
            t.tv(
                "briefPort",
                &[
                    ("port", &row.port.port.to_string()),
                    ("program", &program),
                    ("pid", &pid),
                    ("since", &started),
                    ("owner", &owner),
                ],
            )
        }
    };
    brief.replace(['\u{a0}', '\u{202f}'], " ")
}

// ---------------------------------------------------------------- Failures.

/// What a command's refusal or exit means to the tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fault {
    Denied,
    /// `not_found`: the program is not there (Node, docker).
    Missing,
    Busy,
    /// The command's own limit passed (`timedOut`).
    Timeout,
    Failed,
}

impl Fault {
    pub fn as_str(self) -> &'static str {
        match self {
            Fault::Denied => "denied",
            Fault::Missing => "missing",
            Fault::Busy => "busy",
            Fault::Timeout => "timeout",
            Fault::Failed => "failed",
        }
    }
}

/// The fault of a refused call, by its code. The 30 s limit kit.js put on a
/// call is gone natively: a timeout is only a command's own `timedOut`.
pub fn classify(code: &str) -> Fault {
    match code {
        "denied" => Fault::Denied,
        "not_found" => Fault::Missing,
        "busy" => Fault::Busy,
        _ => Fault::Failed,
    }
}

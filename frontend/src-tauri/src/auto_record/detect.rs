//! Meeting-app classification from window titles and microphone-in-use signals.
//!
//! Window titles are heuristics. The strong signal is Core Audio
//! `kAudioProcessPropertyIsRunningInput` for a known bundle id, which means
//! that process has an active input stream. Titles are used for browser
//! meetings and as a fallback when the process list is unavailable.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeetingApp {
    Teams,
    Zoom,
    Meet,
}

impl MeetingApp {
    pub fn label(self) -> &'static str {
        match self {
            MeetingApp::Teams => "Microsoft Teams",
            MeetingApp::Zoom => "Zoom",
            MeetingApp::Meet => "Google Meet",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ObservedWindow {
    pub owner: String,
    pub title: String,
}

#[derive(Debug, Clone)]
pub struct ObservedInput {
    pub bundle_id: String,
    pub input_active: bool,
}

const TEAMS_BUNDLES: &[&str] = &["com.microsoft.teams2", "com.microsoft.teams"];
const ZOOM_BUNDLES: &[&str] = &["us.zoom.xos"];
const BROWSER_BUNDLES: &[&str] = &[
    "com.google.chrome",
    "com.google.chrome.canary",
    "com.apple.safari",
    "com.microsoft.edgemac",
    "org.mozilla.firefox",
    "com.brave.browser",
    "company.thebrowser.browser",
];

pub fn detect_meeting(windows: &[ObservedWindow], inputs: &[ObservedInput]) -> Option<MeetingApp> {
    inspect(windows, inputs).app
}

/// Why the detector did or did not see a meeting. The text never includes the
/// window title, which can contain the meeting name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detection {
    pub app: Option<MeetingApp>,
    pub hint: &'static str,
}

pub fn inspect(windows: &[ObservedWindow], inputs: &[ObservedInput]) -> Detection {
    let app = if teams_active(windows, inputs) {
        Some(MeetingApp::Teams)
    } else if zoom_active(windows, inputs) {
        Some(MeetingApp::Zoom)
    } else if meet_active(windows, inputs) {
        Some(MeetingApp::Meet)
    } else {
        None
    };
    Detection {
        app,
        hint: hint_for(app, windows, inputs),
    }
}

/// Nome da reunião tirado do título da janela do Teams ("Planejamento Q1 |
/// Microsoft Teams"), para a gravação não nascer como "Microsoft Teams".
/// Só devolve um nome quando há uma única janela do Teams que não é uma das
/// telas ociosas, e nunca um título genérico ("Reunião", "Chamada").
/// O título não vai para o log.
pub fn teams_meeting_title(windows: &[ObservedWindow]) -> Option<String> {
    let mut titles: Vec<String> = Vec::new();
    for window in windows {
        let owner = normalize(&window.owner);
        let folded = normalize(&window.title);
        if !(teams_owner(&owner) || folded.ends_with("| microsoft teams")) || folded.is_empty() || idle_teams_title(&folded) {
            continue;
        }
        let raw = window.title.trim();
        let name = match raw.rfind('|') {
            Some(index) if raw[index + 1..].trim().eq_ignore_ascii_case("Microsoft Teams") => raw[..index].trim(),
            _ => raw,
        };
        // Contador de não lidas no começo: "(3) Planejamento".
        let name = match name.strip_prefix('(').and_then(|rest| rest.split_once(')')) {
            Some((count, rest)) if count.chars().all(|c| c.is_ascii_digit()) => rest.trim(),
            _ => name,
        };
        let generic = matches!(
            name.to_lowercase().as_str(),
            "" | "microsoft teams" | "teams" | "meeting" | "reunião" | "reuniao" | "call" | "chamada" | "meet now"
                | "reunir agora" | "meeting in progress" | "reunião em andamento" | "calling" | "chamando"
        );
        let name: String = name.chars().take(90).collect();
        if !generic && name.chars().filter(|c| c.is_alphanumeric()).count() >= 3 && !titles.contains(&name) {
            titles.push(name);
        }
    }
    if titles.len() == 1 {
        titles.pop()
    } else {
        None
    }
}

fn teams_active(windows: &[ObservedWindow], inputs: &[ObservedInput]) -> bool {
    if bundle_input(inputs, TEAMS_BUNDLES) {
        return true;
    }
    // A muted or solo meeting often has no Core Audio input. The call window
    // still counts. The process being listed must not hide that window.
    let browser_mic = bundle_input(inputs, BROWSER_BUNDLES);
    windows.iter().any(|window| {
        if teams_call_window(window) {
            return true;
        }
        let owner = normalize(&window.owner);
        let title = normalize(&window.title);
        browser_owner(&owner) && browser_mic && title.contains("microsoft teams") && !idle_teams_title(&title)
    })
}

fn teams_call_window(window: &ObservedWindow) -> bool {
    let owner = normalize(&window.owner);
    let title = normalize(&window.title);
    let from_teams = teams_owner(&owner) || title.contains("microsoft teams");
    from_teams && teams_call_title(&title)
}

fn zoom_active(windows: &[ObservedWindow], inputs: &[ObservedInput]) -> bool {
    if bundle_input(inputs, ZOOM_BUNDLES) {
        return true;
    }
    windows.iter().any(|window| {
        let owner = normalize(&window.owner);
        let title = normalize(&window.title);
        (owner.contains("zoom.us") || owner == "zoom") && zoom_call_title(&title)
    })
}

fn meet_active(windows: &[ObservedWindow], inputs: &[ObservedInput]) -> bool {
    let coded = windows.iter().any(|window| {
        let owner = normalize(&window.owner);
        let title = normalize(&window.title);
        browser_owner(&owner) && meet_code(&title)
    });
    if coded {
        return true;
    }
    bundle_input(inputs, BROWSER_BUNDLES)
        && windows.iter().any(|window| {
            let owner = normalize(&window.owner);
            let title = normalize(&window.title);
            browser_owner(&owner) && title.contains("google meet")
        })
}

fn bundle_input(inputs: &[ObservedInput], bundles: &[&str]) -> bool {
    inputs.iter().any(|input| input.input_active && bundle_is(input, bundles))
}

fn bundle_is(input: &ObservedInput, bundles: &[&str]) -> bool {
    let id = input.bundle_id.trim().to_ascii_lowercase();
    bundles.iter().any(|bundle| id == *bundle || id.starts_with(&format!("{bundle}.")))
}

fn teams_owner(owner: &str) -> bool {
    owner.contains("microsoft teams") || owner == "teams"
}

fn browser_owner(owner: &str) -> bool {
    owner.contains("chrome")
        || owner.contains("safari")
        || owner.contains("edge")
        || owner.contains("firefox")
        || owner.contains("brave")
        || owner == "arc"
}

fn teams_call_title(title: &str) -> bool {
    if title.is_empty() || idle_teams_title(title) {
        return false;
    }
    title.contains("meeting")
        || title.contains("reunião")
        || title.contains("reuniao")
        || title.contains("reunir")
        || title.contains("meet now")
        || title.contains("chamada")
        || title.contains("calling")
        || title.contains("call ")
        || title.ends_with(" call")
        || title == "call"
        || title.contains("em andamento")
}

fn zoom_call_title(title: &str) -> bool {
    title.contains("zoom meeting")
        || title.contains("reunião do zoom")
        || title.contains("reuniao do zoom")
        || title.contains("zoom webinar")
}

fn meet_code(title: &str) -> bool {
    title.starts_with("meet -") || title.starts_with("meet –") || title.contains("meet - ")
}

fn idle_teams_title(title: &str) -> bool {
    matches!(
        title,
        "microsoft teams"
            | "teams"
            | "chat | microsoft teams"
            | "calendar | microsoft teams"
            | "calls | microsoft teams"
            | "activity | microsoft teams"
            | "files | microsoft teams"
            | "apps | microsoft teams"
            | "onedrive | microsoft teams"
            | "bate-papo | microsoft teams"
            | "calendário | microsoft teams"
            | "calendario | microsoft teams"
            | "chamadas | microsoft teams"
            | "atividade | microsoft teams"
            | "arquivos | microsoft teams"
            | "aplicativos | microsoft teams"
            | "comunidades | microsoft teams"
    )
}

fn hint_for(app: Option<MeetingApp>, windows: &[ObservedWindow], inputs: &[ObservedInput]) -> &'static str {
    if app.is_some() {
        return match app {
            Some(MeetingApp::Teams) if bundle_input(inputs, TEAMS_BUNDLES) => {
                "Microfone do Teams em uso. Se continuar, a gravação começa."
            }
            Some(MeetingApp::Zoom) if bundle_input(inputs, ZOOM_BUNDLES) => {
                "Microfone do Zoom em uso. Se continuar, a gravação começa."
            }
            Some(MeetingApp::Meet) if bundle_input(inputs, BROWSER_BUNDLES) => {
                "Microfone do navegador em uso numa aba do Meet. Se continuar, a gravação começa."
            }
            Some(_) => "Janela de reunião reconhecida. Se continuar, a gravação começa.",
            None => "Nenhuma reunião detectada.",
        };
    }
    if teams_title_hidden(windows) {
        return "O Teams está na tela, mas o macOS não entregou o título da janela. Em Ajustes do Sistema → Privacidade e Segurança → Gravação de Tela, permita o Meetily Live e abra o app de novo. Sem o título, a gravação só começa com o microfone do Teams sem mudo.";
    }
    if bundle_listed(inputs, TEAMS_BUNDLES) || windows.iter().any(|window| teams_owner(&normalize(&window.owner))) {
        return "O Teams está aberto, ainda fora de uma reunião. Clique em Reunir agora, entre na chamada (pode ser sozinho) e espere o contador. Deixe o microfone sem mudo se o título não mudar.";
    }
    if bundle_listed(inputs, ZOOM_BUNDLES) {
        return "O Zoom está aberto, ainda fora de uma reunião. Entre na sala e espere o contador.";
    }
    "Nenhuma reunião detectada. Abra o Teams, o Zoom ou o Meet, entre numa chamada e acompanhe o status aqui."
}

fn teams_title_hidden(windows: &[ObservedWindow]) -> bool {
    let mut saw_owner = false;
    let mut saw_title = false;
    for window in windows {
        let owner = normalize(&window.owner);
        if !teams_owner(&owner) {
            continue;
        }
        saw_owner = true;
        if !window.title.trim().is_empty() {
            saw_title = true;
        }
    }
    saw_owner && !saw_title
}

fn bundle_listed(inputs: &[ObservedInput], bundles: &[&str]) -> bool {
    inputs.iter().any(|input| bundle_is(input, bundles))
}

fn normalize(value: &str) -> String {
    let trimmed = value.trim().to_lowercase();
    let stripped = if let Some(rest) = trimmed.strip_prefix('(') {
        if let Some(end) = rest.find(')') {
            rest[end + 1..].trim()
        } else {
            trimmed.as_str()
        }
    } else {
        trimmed.as_str()
    };
    stripped.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(owner: &str, title: &str) -> ObservedWindow {
        ObservedWindow {
            owner: owner.to_string(),
            title: title.to_string(),
        }
    }

    fn input(bundle: &str, active: bool) -> ObservedInput {
        ObservedInput {
            bundle_id: bundle.to_string(),
            input_active: active,
        }
    }

    #[test]
    fn teams_window_gives_the_meeting_name_only_when_unambiguous() {
        let idle = window("Microsoft Teams", "Bate-papo | Microsoft Teams");
        let meeting = window("Microsoft Teams", "(2) Planejamento Q1 – ACME | Microsoft Teams");
        assert_eq!(
            teams_meeting_title(&[idle.clone(), meeting.clone()]).as_deref(),
            Some("Planejamento Q1 – ACME")
        );
        assert_eq!(teams_meeting_title(&[idle.clone()]), None);
        assert_eq!(teams_meeting_title(&[idle.clone(), window("Microsoft Teams", "Reunião | Microsoft Teams")]), None);
        // Duas janelas candidatas: não dá para saber qual é a reunião.
        assert_eq!(
            teams_meeting_title(&[meeting, window("Microsoft Teams", "Ana Souza | Microsoft Teams")]),
            None
        );
        assert_eq!(teams_meeting_title(&[window("Google Chrome", "Meet - abc-defg-hij")]), None);
    }

    #[test]
    fn teams_mic_in_use_starts_and_idle_chat_does_not() {
        assert_eq!(
            detect_meeting(&[], &[input("com.microsoft.teams2", true)]),
            Some(MeetingApp::Teams)
        );
        assert_eq!(
            detect_meeting(
                &[window("Microsoft Teams", "Chat | Microsoft Teams")],
                &[input("com.microsoft.teams2", false)]
            ),
            None
        );
        assert_eq!(
            detect_meeting(
                &[window("Microsoft Teams", "(3) Chat | Microsoft Teams")],
                &[input("com.microsoft.teams", false)]
            ),
            None
        );
    }

    #[test]
    fn a_call_window_counts_even_when_teams_is_muted() {
        assert_eq!(
            detect_meeting(&[window("Microsoft Teams", "Chamada | Microsoft Teams")], &[]),
            Some(MeetingApp::Teams)
        );
        assert_eq!(
            detect_meeting(
                &[window("Microsoft Teams", "Reunião de Renan | Microsoft Teams")],
                &[input("com.microsoft.teams2", false)]
            ),
            Some(MeetingApp::Teams)
        );
        assert_eq!(
            detect_meeting(
                &[window("Microsoft Teams", "Chamadas | Microsoft Teams")],
                &[input("com.microsoft.teams2", false)]
            ),
            None
        );
        assert_eq!(
            detect_meeting(
                &[window("Microsoft Teams", "")],
                &[input("com.microsoft.teams2", false)]
            ),
            None
        );
        assert!(inspect(&[window("Microsoft Teams", "")], &[input("com.microsoft.teams2", false)])
            .hint
            .contains("Gravação de Tela"));
    }

    #[test]
    fn browser_teams_requires_the_browser_microphone() {
        let title = window("Google Chrome", "Daily | Microsoft Teams");
        assert_eq!(
            detect_meeting(&[title.clone()], &[input("com.google.Chrome", true)]),
            Some(MeetingApp::Teams)
        );
        assert_eq!(
            detect_meeting(&[title], &[input("com.google.Chrome", false)]),
            None
        );
    }

    #[test]
    fn zoom_and_meet_share_the_same_signals_and_teams_wins() {
        assert_eq!(
            detect_meeting(&[], &[input("us.zoom.xos", true)]),
            Some(MeetingApp::Zoom)
        );
        assert_eq!(
            detect_meeting(
                &[window("zoom.us", "Zoom Meeting")],
                &[]
            ),
            Some(MeetingApp::Zoom)
        );
        assert_eq!(
            detect_meeting(
                &[window("Google Chrome", "Meet - abc-defg-hij")],
                &[input("com.google.Chrome", true)]
            ),
            Some(MeetingApp::Meet)
        );
        assert_eq!(
            detect_meeting(
                &[window("Safari", "Meet - abc")],
                &[input("com.apple.Safari", false)]
            ),
            Some(MeetingApp::Meet)
        );
        assert_eq!(
            detect_meeting(
                &[window("Google Chrome", "Google Meet")],
                &[input("com.google.Chrome", false)]
            ),
            None
        );
        assert_eq!(
            detect_meeting(
                &[window("zoom.us", "Zoom Meeting")],
                &[input("us.zoom.xos", false)]
            ),
            Some(MeetingApp::Zoom)
        );
        assert_eq!(
            detect_meeting(
                &[window("Google Chrome", "Meet - abc")],
                &[input("com.microsoft.teams2", true), input("com.google.Chrome", true)]
            ),
            Some(MeetingApp::Teams)
        );
    }
}

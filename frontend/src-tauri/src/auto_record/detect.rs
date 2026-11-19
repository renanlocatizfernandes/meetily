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
    if teams_active(windows, inputs) {
        return Some(MeetingApp::Teams);
    }
    if zoom_active(windows, inputs) {
        return Some(MeetingApp::Zoom);
    }
    if meet_active(windows, inputs) {
        return Some(MeetingApp::Meet);
    }
    None
}

fn teams_active(windows: &[ObservedWindow], inputs: &[ObservedInput]) -> bool {
    if bundle_input(inputs, TEAMS_BUNDLES) {
        return true;
    }
    let process_list_knows_teams = inputs.iter().any(|input| bundle_is(input, TEAMS_BUNDLES));
    if process_list_knows_teams {
        return false;
    }
    let browser_mic = bundle_input(inputs, BROWSER_BUNDLES);
    windows.iter().any(|window| {
        let owner = normalize(&window.owner);
        let title = normalize(&window.title);
        if teams_owner(&owner) && teams_call_title(&title) && inputs.is_empty() {
            return true;
        }
        browser_owner(&owner) && browser_mic && title.contains("microsoft teams") && !idle_teams_title(&title)
    })
}

fn zoom_active(windows: &[ObservedWindow], inputs: &[ObservedInput]) -> bool {
    if bundle_input(inputs, ZOOM_BUNDLES) {
        return true;
    }
    let zoom_known = inputs.iter().any(|input| bundle_is(input, ZOOM_BUNDLES));
    if zoom_known {
        return false;
    }
    windows.iter().any(|window| {
        let owner = normalize(&window.owner);
        let title = normalize(&window.title);
        (owner.contains("zoom.us") || owner == "zoom")
            && (title.contains("zoom meeting") || title.contains("reunião do zoom") || title.contains("reuniao do zoom"))
    })
}

fn meet_active(windows: &[ObservedWindow], inputs: &[ObservedInput]) -> bool {
    if !bundle_input(inputs, BROWSER_BUNDLES) && !inputs.is_empty() {
        return false;
    }
    if inputs.is_empty() {
        return false;
    }
    windows.iter().any(|window| {
        let owner = normalize(&window.owner);
        let title = normalize(&window.title);
        browser_owner(&owner) && meet_title(&title)
    })
}

fn bundle_input(inputs: &[ObservedInput], bundles: &[&str]) -> bool {
    inputs.iter().any(|input| input.input_active && bundle_is(input, bundles))
}

fn bundle_is(input: &ObservedInput, bundles: &[&str]) -> bool {
    let id = input.bundle_id.trim().to_ascii_lowercase();
    bundles.iter().any(|bundle| id == *bundle)
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
        || title.contains("chamada")
        || title.contains("calling")
        || title.contains("call ")
        || title.ends_with(" call")
        || title == "call"
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
    )
}

fn meet_title(title: &str) -> bool {
    title.contains("google meet") || title.starts_with("meet -") || title.starts_with("meet –")
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
    fn teams_call_title_is_used_only_when_process_list_is_missing() {
        assert_eq!(
            detect_meeting(&[window("Microsoft Teams", "Chamada | Microsoft Teams")], &[]),
            Some(MeetingApp::Teams)
        );
        assert_eq!(
            detect_meeting(
                &[window("Microsoft Teams", "Chamada | Microsoft Teams")],
                &[input("com.microsoft.teams2", false)]
            ),
            None
        );
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
            None
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalendarEvent {
    pub title: String,
    pub notes: String,
    pub attendees: String,
}

pub fn prefill_context(events: &[CalendarEvent]) -> String {
    let Some(event) = events.first() else {
        return String::new();
    };
    let mut lines = vec![format!("Reunião: {}", event.title.trim())];
    if !event.notes.trim().is_empty() {
        lines.push(format!("Pauta: {}", event.notes.trim()));
    }
    if !event.attendees.trim().is_empty() {
        lines.push(format!("Participantes: {}", event.attendees.trim()));
    }
    lines.join("\n")
}

pub fn parse_calendar_lines(output: &str) -> Vec<CalendarEvent> {
    output
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, '\t');
            let title = parts.next()?.trim();
            if title.is_empty() {
                return None;
            }
            Some(CalendarEvent {
                title: title.to_string(),
                notes: parts.next().unwrap_or("").trim().to_string(),
                attendees: parts.next().unwrap_or("").trim().to_string(),
            })
        })
        .take(5)
        .collect()
}

pub fn parse_ics(ics: &str) -> Vec<CalendarEvent> {
    let mut events = Vec::new();
    let mut current: Option<CalendarEvent> = None;
    for raw in unfold(ics).lines() {
        let line = raw.trim();
        if line.eq_ignore_ascii_case("BEGIN:VEVENT") {
            current = Some(CalendarEvent {
                title: String::new(),
                notes: String::new(),
                attendees: String::new(),
            });
            continue;
        }
        let Some(event) = current.as_mut() else { continue };
        if line.eq_ignore_ascii_case("END:VEVENT") {
            if !event.title.is_empty() {
                events.push(event.clone());
            }
            current = None;
            continue;
        }
        if let Some(value) = line.strip_prefix("SUMMARY:") {
            event.title = unescape(value);
        } else if let Some(value) = line.strip_prefix("DESCRIPTION:") {
            event.notes = unescape(value);
        } else if let Some(value) = line.strip_prefix("ATTENDEE") {
            let name = value
                .split(';')
                .find_map(|part| part.strip_prefix("CN="))
                .map(|cn| cn.split(':').next().unwrap_or(cn))
                .or_else(|| value.rsplit(':').next())
                .unwrap_or("")
                .trim_matches('"');
            if !name.is_empty() {
                if !event.attendees.is_empty() {
                    event.attendees.push_str(", ");
                }
                event.attendees.push_str(name);
            }
        }
    }
    events
}

fn unfold(ics: &str) -> String {
    ics.replace("\r\n ", "").replace("\n ", "").replace("\r\n\t", "").replace("\n\t", "")
}

fn unescape(value: &str) -> String {
    value.replace("\\n", " ").replace("\\,", ",").replace("\\;", ";").replace("\\\\", "\\")
}

pub fn upcoming_on_macos() -> Result<Vec<CalendarEvent>, String> {
    #[cfg(target_os = "macos")]
    {
        read_calendar_app()
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("A agenda local só é lida no macOS. Cole um .ics exportado do Calendário.".to_string())
    }
}

#[cfg(target_os = "macos")]
fn read_calendar_app() -> Result<Vec<CalendarEvent>, String> {
    // Calendar scripting dictionary (Calendar.app), which sees the same local
    // calendars EventKit uses, including accounts already synced on the Mac.
    let script = r#"
set output to ""
set startDate to current date
set endDate to startDate + (1 * days)
tell application "Calendar"
  repeat with cal in calendars
    try
      set evs to (every event of cal whose start date ≥ startDate and start date ≤ endDate)
      repeat with ev in evs
        set noteText to ""
        try
          set noteText to description of ev
        end try
        set output to output & (summary of ev) & tab & noteText & tab & "" & linefeed
      end repeat
    end try
  end repeat
end tell
return output
"#;
    let output = std::process::Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|_| "Não foi possível falar com o app Calendário.".to_string())?;
    if !output.status.success() {
        return Err("O Calendário não autorizou a leitura. Aceite o pedido de controle do app Calendário.".to_string());
    }
    let text = String::from_utf8_lossy(&output.stdout);
    Ok(parse_calendar_lines(&text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ics_and_lines_become_meeting_context() {
        let ics = "BEGIN:VCALENDAR\nBEGIN:VEVENT\nSUMMARY:Proposta Aurora\nDESCRIPTION:Fechar preço e prazo\nATTENDEE;CN=Ana:mailto:ana@example.com\nEND:VEVENT\nEND:VCALENDAR\n";
        let events = parse_ics(ics);
        let context = prefill_context(&events);
        assert!(context.contains("Reunião: Proposta Aurora"));
        assert!(context.contains("Pauta: Fechar preço e prazo"));
        assert!(context.contains("Participantes: Ana"));
        let lines = parse_calendar_lines("Proposta Aurora\tFechar preço\tAna\n");
        assert_eq!(lines[0].title, "Proposta Aurora");
    }
}

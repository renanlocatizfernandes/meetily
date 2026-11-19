use std::collections::HashMap;
use std::sync::Mutex;

static ALIASES: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

fn map() -> std::sync::MutexGuard<'static, Option<HashMap<String, String>>> {
    ALIASES.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn clear_aliases() {
    *map() = Some(HashMap::new());
}

pub fn rename(key: String, display_name: String) {
    let mut guard = map();
    let aliases = guard.get_or_insert_with(HashMap::new);
    let trimmed = display_name.trim().chars().take(40).collect::<String>();
    if trimmed.is_empty() {
        aliases.remove(&key);
    } else {
        aliases.insert(key, trimmed);
    }
}

pub fn display_name(key: &str, default_label: &str) -> String {
    map()
        .as_ref()
        .and_then(|aliases| aliases.get(key).cloned())
        .unwrap_or_else(|| default_label.to_string())
}

use super::LiveModelEntry;

pub(super) fn managed_model_dir_name(model: &LiveModelEntry) -> String {
    let preferred = model
        .short_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(&model.id);
    safe_path_component(preferred)
}

pub(super) fn safe_path_component(value: &str) -> String {
    let mut component = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    while component.starts_with('.') {
        component.remove(0);
    }
    while component.ends_with('.') {
        component.pop();
    }
    if component.is_empty() {
        "model".to_owned()
    } else {
        component
    }
}

pub(super) fn optional_str(value: Option<&str>) -> &str {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("-")
}

pub(super) fn format_size_bytes(size_bytes: Option<u64>) -> String {
    let Some(size_bytes) = size_bytes else {
        return "unknown".to_owned();
    };
    if size_bytes < 1024 {
        return format!("{size_bytes} B");
    }
    if size_bytes < 1024 * 1024 {
        return format_tenths(size_bytes, 1024, "KiB");
    }
    if size_bytes < 1024 * 1024 * 1024 {
        return format_tenths(size_bytes, 1024 * 1024, "MiB");
    }
    format_tenths(size_bytes, 1024 * 1024 * 1024, "GiB")
}

pub(super) fn format_tenths(size_bytes: u64, unit: u64, label: &str) -> String {
    let tenths = u128::from(size_bytes) * 10 / u128::from(unit);
    format!("{}.{:01} {label}", tenths / 10, tenths % 10)
}

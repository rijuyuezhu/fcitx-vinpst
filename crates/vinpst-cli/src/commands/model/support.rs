use super::LiveModelEntry;

pub(super) fn managed_model_dir_name(model: &LiveModelEntry) -> String {
    vinpst_registry::managed_model_dir_name(model)
}

pub(super) fn safe_path_component(value: &str) -> String {
    vinpst_registry::safe_path_component(value)
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

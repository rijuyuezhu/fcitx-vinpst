use crate::ConfigExample;

pub(crate) fn config_example_description(kind: ConfigExample) -> &'static str {
    match kind {
        ConfigExample::Default => "upstream-compatible default config skeleton",
        ConfigExample::CommandDemo => "deterministic command ASR/text adapter demo",
        ConfigExample::ConfiguredPipewireLive => {
            "configured command backends for live PipeWire smoke"
        }
    }
}

pub(crate) fn config_example_contents(kind: ConfigExample) -> &'static str {
    match kind {
        ConfigExample::Default => include_str!("../../../data/default-config.json"),
        ConfigExample::CommandDemo => include_str!("../../../data/e2e-command-demo-config.json"),
        ConfigExample::ConfiguredPipewireLive => {
            include_str!("../../../data/e2e-configured-pipewire-live.json")
        }
    }
}

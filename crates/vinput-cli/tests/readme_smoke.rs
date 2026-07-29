//! Regression tests for the primary project check commands documented in README.

mod common;

use common::workspace_file;

#[test]
fn readme_documents_primary_check_recipes() {
    let readme = std::fs::read_to_string(workspace_file("README.md")).expect("read README");
    let justfile = std::fs::read_to_string(workspace_file("justfile")).expect("read justfile");

    for command in ["just ci", "just smoke"] {
        assert!(
            readme.contains(command),
            "README should document primary command `{command}`"
        );
    }

    assert!(
        justfile.lines().any(|line| line == "ci: check"),
        "justfile should keep the CI alias"
    );
    assert!(
        justfile.lines().any(|line| line.starts_with("smoke:")),
        "justfile should define the smoke recipe"
    );
    assert!(
        readme
            .contains("Optional live PipeWire and real-desktop checks are intentionally excluded"),
        "README should distinguish deterministic CI from live validation"
    );
}

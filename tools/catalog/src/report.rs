//! Prints what the checks found, for people and for GitHub Actions.

use std::env;
use std::path::Path;

use zendiloos_catalog::Problem;

use crate::app::Checked;

/// Prints one line per app and its problems; on GitHub Actions also one
/// annotation per problem, so they show up in the pull request. Returns
/// whether all apps passed.
pub fn print(apps: &Path, checked: &[Checked]) -> bool {
    let annotate = env::var("GITHUB_ACTIONS").is_ok_and(|value| value == "true");
    let mut failed = 0;
    for app in checked {
        if app.problems.is_empty() {
            println!("✓ {}", app.id);
            continue;
        }
        failed += 1;
        println!("✗ {}", app.id);
        for problem in &app.problems {
            println!("    {problem}");
            if annotate {
                println!("{}", annotation(apps, &app.id, problem));
            }
        }
    }
    let total = checked.len();
    if failed == 0 {
        println!("\n{total} {} passed", if total == 1 { "app" } else { "apps" });
    } else {
        println!("\n{failed} of {total} {} failed", if total == 1 { "app" } else { "apps" });
    }
    failed == 0
}

fn annotation(apps: &Path, id: &str, problem: &Problem) -> String {
    let mut file = apps.join(id);
    if let Some(name) = problem.file.name() {
        file.push(name);
    }
    let mut properties = format!("file={}", escape_property(&file.to_string_lossy()));
    if let Some(line) = problem.line {
        properties.push_str(&format!(",line={line}"));
    }
    properties.push_str(&format!(",title={}", escape_property(id)));
    let message = if problem.at.is_empty() {
        problem.message.clone()
    } else {
        format!("{}: {}", problem.at, problem.message)
    };
    format!("::error {properties}::{}", escape_data(&message))
}

fn escape_data(text: &str) -> String {
    text.replace('%', "%25").replace('\r', "%0D").replace('\n', "%0A")
}

fn escape_property(text: &str) -> String {
    escape_data(text).replace(':', "%3A").replace(',', "%2C")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn annotations_point_at_the_file_and_escape_their_text() {
        let problem = Problem::compose("services.web.ports[0]", "100% wrong,\nreally");
        assert_eq!(
            annotation(Path::new("apps"), "demo", &problem),
            "::error file=apps/demo/compose.yaml,title=demo::services.web.ports[0]: 100%25 wrong,%0Areally"
        );
    }
}

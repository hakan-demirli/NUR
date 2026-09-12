use crate::error::CiError;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct Workflow {
    pub name: String,
    pub jobs: Vec<WorkflowJob>,
}

#[derive(Debug, Clone)]
pub struct WorkflowJob {
    pub name: String,
    pub needs: Vec<String>,
    pub steps: Vec<WorkflowStep>,
    pub job_id: String,
}

#[derive(Debug, Clone)]
pub struct WorkflowStep {
    pub name: String,
    pub run: String,
}

pub fn parse_workflows(repo_dir: &Path) -> Result<Vec<Workflow>, CiError> {
    let workflows_dir = repo_dir.join(".github").join("workflows");
    if !workflows_dir.is_dir() {
        return Err(CiError::ConfigValidation {
            detail: format!(
                "no .github/workflows/ directory found in {}",
                repo_dir.display()
            ),
        });
    }

    let mut workflows = Vec::new();
    let mut entries: Vec<_> = std::fs::read_dir(&workflows_dir)
        .map_err(|e| CiError::ConfigIo {
            path: workflows_dir.clone(),
            source: e,
        })?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            name.ends_with(".yml") || name.ends_with(".yaml")
        })
        .collect();

    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let path = entry.path();
        let content = std::fs::read_to_string(&path).map_err(|e| CiError::ConfigIo {
            path: path.clone(),
            source: e,
        })?;
        let workflow = parse_workflow_yaml(&content, &path)?;
        workflows.push(workflow);
    }

    if workflows.is_empty() {
        return Err(CiError::ConfigValidation {
            detail: format!(
                "no workflow YAML files found in {}",
                workflows_dir.display()
            ),
        });
    }

    Ok(workflows)
}

fn parse_workflow_yaml(yaml_str: &str, origin: &Path) -> Result<Workflow, CiError> {
    let doc: serde_yaml::Value =
        serde_yaml::from_str(yaml_str).map_err(|e| CiError::ConfigValidation {
            detail: format!("failed to parse YAML {}: {e}", origin.display()),
        })?;

    let workflow_name = doc
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| {
            origin
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("workflow")
        })
        .to_string();

    let jobs_map = doc
        .get("jobs")
        .and_then(|v| v.as_mapping())
        .ok_or_else(|| CiError::ConfigValidation {
            detail: format!("no 'jobs' key in workflow {}", origin.display()),
        })?;

    let mut jobs = Vec::new();

    for (job_key, job_value) in jobs_map {
        let job_id = job_key
            .as_str()
            .ok_or_else(|| CiError::ConfigValidation {
                detail: format!("non-string job key in {}", origin.display()),
            })?
            .to_string();

        let needs = parse_needs(job_value);

        let matrix_combos = parse_matrix(job_value);

        let raw_steps = job_value
            .get("steps")
            .and_then(|v| v.as_sequence())
            .ok_or_else(|| CiError::ConfigValidation {
                detail: format!("job '{}' has no 'steps' in {}", job_id, origin.display()),
            })?;

        if matrix_combos.is_empty() {
            let steps = extract_nix_steps(raw_steps, &job_id, origin, &HashMap::new())?;
            let display_name = job_value
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or(&job_id)
                .to_string();
            jobs.push(WorkflowJob {
                name: display_name,
                needs: needs.clone(),
                steps,
                job_id: job_id.clone(),
            });
        } else {
            let name_template = job_value
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or(&job_id);

            for combo in &matrix_combos {
                let expanded_name = expand_matrix_string(name_template, combo);
                let steps = extract_nix_steps(raw_steps, &job_id, origin, combo)?;
                let expanded_job_id = if combo.len() == 1 {
                    let (_, val) = combo.iter().next().unwrap();
                    format!("{job_id} ({val})")
                } else {
                    let parts: Vec<String> =
                        combo.iter().map(|(k, v)| format!("{k}={v}")).collect();
                    format!("{job_id} ({})", parts.join(", "))
                };

                jobs.push(WorkflowJob {
                    name: expanded_name,
                    needs: needs.clone(),
                    steps,
                    job_id: expanded_job_id,
                });
            }
        }
    }

    Ok(Workflow {
        name: workflow_name,
        jobs,
    })
}

fn parse_needs(job_value: &serde_yaml::Value) -> Vec<String> {
    match job_value.get("needs") {
        Some(serde_yaml::Value::String(s)) => vec![s.clone()],
        Some(serde_yaml::Value::Sequence(seq)) => seq
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect(),
        _ => vec![],
    }
}

fn parse_matrix(job_value: &serde_yaml::Value) -> Vec<HashMap<String, String>> {
    let matrix = match job_value
        .get("strategy")
        .and_then(|s| s.get("matrix"))
        .and_then(|m| m.as_mapping())
    {
        Some(m) => m,
        None => return vec![],
    };

    let mut axes: Vec<(String, Vec<String>)> = Vec::new();

    for (key, value) in matrix {
        let key_str = match key.as_str() {
            Some(k) if k != "include" && k != "exclude" => k.to_string(),
            _ => continue,
        };

        let values: Vec<String> = match value.as_sequence() {
            Some(seq) => seq
                .iter()
                .map(|v| match v {
                    serde_yaml::Value::String(s) => s.clone(),
                    serde_yaml::Value::Number(n) => n.to_string(),
                    serde_yaml::Value::Bool(b) => b.to_string(),
                    other => serde_yaml::to_string(other).unwrap_or_default(),
                })
                .collect(),
            None => continue,
        };

        if !values.is_empty() {
            axes.push((key_str, values));
        }
    }

    if axes.is_empty() {
        return vec![];
    }

    let mut combos: Vec<HashMap<String, String>> = vec![HashMap::new()];
    for (key, values) in &axes {
        let mut new_combos = Vec::new();
        for combo in &combos {
            for val in values {
                let mut new_combo = combo.clone();
                new_combo.insert(key.clone(), val.clone());
                new_combos.push(new_combo);
            }
        }
        combos = new_combos;
    }

    combos
}

fn expand_matrix_string(template: &str, combo: &HashMap<String, String>) -> String {
    let mut result = template.to_string();
    for (key, value) in combo {
        let pattern = format!("${{{{ matrix.{key} }}}}");
        result = result.replace(&pattern, value);
    }
    result
}

fn extract_nix_steps(
    steps: &[serde_yaml::Value],
    job_id: &str,
    origin: &Path,
    matrix: &HashMap<String, String>,
) -> Result<Vec<WorkflowStep>, CiError> {
    let mut nix_steps = Vec::new();

    for (idx, step) in steps.iter().enumerate() {
        if step.get("uses").is_some() {
            continue;
        }

        let run_cmd = match step.get("run").and_then(|v| v.as_str()) {
            Some(cmd) => cmd.to_string(),
            None => continue,
        };

        let run_cmd = expand_matrix_string(&run_cmd, matrix);

        validate_nix_only(&run_cmd, job_id, idx, origin)?;

        let step_name = step
            .get("name")
            .and_then(|v| v.as_str())
            .map(|s| expand_matrix_string(s, matrix))
            .unwrap_or_else(|| format!("step-{idx}"));

        nix_steps.push(WorkflowStep {
            name: step_name,
            run: run_cmd,
        });
    }

    Ok(nix_steps)
}

fn validate_nix_only(
    run_cmd: &str,
    job_id: &str,
    step_idx: usize,
    origin: &Path,
) -> Result<(), CiError> {
    let mut logical_lines: Vec<String> = Vec::new();
    let mut current = String::new();

    for line in run_cmd.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            if !current.is_empty() {
                logical_lines.push(std::mem::take(&mut current));
            }
            continue;
        }

        if current.is_empty() {
            current = trimmed.to_string();
        } else {
            current.push(' ');
            current.push_str(trimmed);
        }

        if !trimmed.ends_with('\\') {
            logical_lines.push(std::mem::take(&mut current));
        } else {
            current = current.trim_end_matches('\\').trim_end().to_string();
        }
    }

    if !current.is_empty() {
        logical_lines.push(current);
    }

    for logical_line in &logical_lines {
        let cmd = logical_line.trim();
        if cmd.is_empty() {
            continue;
        }

        let first_word = cmd.split_whitespace().next().unwrap_or("");
        if first_word != "nix" {
            return Err(CiError::ConfigValidation {
                detail: format!(
                    "job '{}', step {}: run command does not start with 'nix': '{}' (in {}). \
                     ci-local only supports nix commands.",
                    job_id,
                    step_idx,
                    cmd.chars().take(80).collect::<String>(),
                    origin.display()
                ),
            });
        }
    }

    Ok(())
}

pub fn flatten_workflows(workflows: &[Workflow]) -> Vec<WorkflowJob> {
    let mut all_jobs = Vec::new();
    for workflow in workflows {
        all_jobs.extend(workflow.jobs.clone());
    }
    all_jobs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_workflow() {
        let yaml = r#"
name: "CI"
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Build
        run: nix build -L .#default
"#;
        let workflow = parse_workflow_yaml(yaml, Path::new("test.yml")).unwrap();
        assert_eq!(workflow.name, "CI");
        assert_eq!(workflow.jobs.len(), 1);
        assert_eq!(workflow.jobs[0].name, "build");
        assert_eq!(workflow.jobs[0].steps.len(), 1);
        assert_eq!(workflow.jobs[0].steps[0].name, "Build");
        assert!(workflow.jobs[0].steps[0].run.contains("nix build"));
    }

    #[test]
    fn parse_workflow_with_needs() {
        let yaml = r#"
name: "CI"
on: push
jobs:
  lint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Lint
        run: nix build -L .#checks.x86_64-linux.lint
  test:
    needs: lint
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Test
        run: nix build -L .#checks.x86_64-linux.test
"#;
        let workflow = parse_workflow_yaml(yaml, Path::new("test.yml")).unwrap();
        assert_eq!(workflow.jobs.len(), 2);
        assert!(workflow.jobs[0].needs.is_empty());
        assert_eq!(workflow.jobs[1].needs, vec!["lint"]);
    }

    #[test]
    fn parse_workflow_with_needs_list() {
        let yaml = r#"
name: "CI"
on: push
jobs:
  a:
    runs-on: ubuntu-latest
    steps:
      - name: A
        run: nix build .#a
  b:
    runs-on: ubuntu-latest
    steps:
      - name: B
        run: nix build .#b
  c:
    needs: [a, b]
    runs-on: ubuntu-latest
    steps:
      - name: C
        run: nix build .#c
"#;
        let workflow = parse_workflow_yaml(yaml, Path::new("test.yml")).unwrap();
        let c = workflow.jobs.iter().find(|j| j.job_id == "c").unwrap();
        assert_eq!(c.needs, vec!["a", "b"]);
    }

    #[test]
    fn parse_matrix_expansion() {
        let yaml = r#"
name: "CI"
on: push
jobs:
  test:
    name: "Test ${{ matrix.check }}"
    runs-on: ubuntu-latest
    strategy:
      matrix:
        check:
          - unit
          - integration
          - lint
    steps:
      - uses: actions/checkout@v4
      - name: Run check
        run: nix build -L .#checks.x86_64-linux.${{ matrix.check }}
"#;
        let workflow = parse_workflow_yaml(yaml, Path::new("test.yml")).unwrap();
        assert_eq!(workflow.jobs.len(), 3);
        assert_eq!(workflow.jobs[0].name, "Test unit");
        assert_eq!(workflow.jobs[1].name, "Test integration");
        assert_eq!(workflow.jobs[2].name, "Test lint");
        assert!(workflow.jobs[0].steps[0]
            .run
            .contains(".#checks.x86_64-linux.unit"));
        assert!(workflow.jobs[2].steps[0]
            .run
            .contains(".#checks.x86_64-linux.lint"));
    }

    #[test]
    fn reject_non_nix_command() {
        let yaml = r#"
name: "CI"
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - name: Bad step
        run: cargo build --release
"#;
        let result = parse_workflow_yaml(yaml, Path::new("test.yml"));
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(err.contains("does not start with 'nix'"), "got: {err}");
    }

    #[test]
    fn uses_steps_are_skipped() {
        let yaml = r#"
name: "CI"
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: DeterminateSystems/nix-installer-action@main
      - name: Build
        run: nix build -L .#default
"#;
        let workflow = parse_workflow_yaml(yaml, Path::new("test.yml")).unwrap();
        assert_eq!(workflow.jobs[0].steps.len(), 1);
        assert_eq!(workflow.jobs[0].steps[0].name, "Build");
    }

    #[test]
    fn multiline_continuation_nix_command() {
        let yaml = r#"
name: "CI"
on: push
jobs:
  lint:
    runs-on: ubuntu-latest
    steps:
      - name: Lint
        run: |
          nix build -L \
            .#checks.x86_64-linux.lint-deadnix \
            .#checks.x86_64-linux.lint-statix
"#;
        let workflow = parse_workflow_yaml(yaml, Path::new("test.yml")).unwrap();
        assert_eq!(workflow.jobs[0].steps.len(), 1);
    }

    #[test]
    fn reject_mixed_nix_and_non_nix() {
        let yaml = r#"
name: "CI"
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - name: Mixed
        run: |
          nix build .#foo
          echo "done"
"#;
        let result = parse_workflow_yaml(yaml, Path::new("test.yml"));
        assert!(result.is_err());
    }

    #[test]
    fn matrix_needs_are_preserved() {
        let yaml = r#"
name: "CI"
on: push
jobs:
  fast:
    runs-on: ubuntu-latest
    steps:
      - name: Fast
        run: nix build .#fast
  checks:
    needs: fast
    runs-on: ubuntu-latest
    strategy:
      matrix:
        check: [a, b]
    steps:
      - name: Check
        run: nix build .#checks.x86_64-linux.${{ matrix.check }}
"#;
        let workflow = parse_workflow_yaml(yaml, Path::new("test.yml")).unwrap();
        assert_eq!(workflow.jobs.len(), 3);
        for job in &workflow.jobs[1..] {
            assert_eq!(job.needs, vec!["fast"]);
        }
    }

    #[test]
    fn workflow_name_defaults_to_filename() {
        let yaml = r#"
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - name: Build
        run: nix build .#pkg
"#;
        let workflow = parse_workflow_yaml(yaml, Path::new("my-ci.yml")).unwrap();
        assert_eq!(workflow.name, "my-ci");
    }

    #[test]
    fn nix_run_command_accepted() {
        let yaml = r#"
name: "CI"
on: push
jobs:
  examples:
    runs-on: ubuntu-latest
    steps:
      - name: Check examples
        run: nix run -L .#check-examples
"#;
        let workflow = parse_workflow_yaml(yaml, Path::new("test.yml")).unwrap();
        assert_eq!(workflow.jobs[0].steps.len(), 1);
        assert!(workflow.jobs[0].steps[0].run.contains("nix run"));
    }

    #[test]
    fn parse_repx_workflow() {
        let yaml = r#"
name: "Nix Build and Test"
on:
  push:
    branches: [ "main" ]
jobs:
  fast-checks:
    name: Fast Checks
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: DeterminateSystems/nix-installer-action@main
      - name: Run Fast Checks
        run: |
          nix build -L \
            .#checks.x86_64-linux.lint-deadnix \
            .#checks.x86_64-linux.lint-statix \
            .#checks.x86_64-linux.lint-formatting
  checks:
    name: Check ${{ matrix.check }}
    needs: fast-checks
    runs-on: ubuntu-latest
    strategy:
      fail-fast: false
      matrix:
        check:
          - unit-tests
          - integration
    steps:
      - uses: actions/checkout@v4
      - uses: DeterminateSystems/nix-installer-action@main
      - name: Run Check
        run: nix build -L .#checks.x86_64-linux.${{ matrix.check }}
  example-checks:
    name: Example Checks
    needs: fast-checks
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: DeterminateSystems/nix-installer-action@main
      - name: Run Example Checks
        run: nix run -L .#check-repx-examples
"#;
        let workflow = parse_workflow_yaml(yaml, Path::new("main.yml")).unwrap();
        assert_eq!(workflow.name, "Nix Build and Test");
        assert_eq!(workflow.jobs.len(), 4);

        let fast = workflow
            .jobs
            .iter()
            .find(|j| j.job_id == "fast-checks")
            .unwrap();
        assert!(fast.needs.is_empty());
        assert_eq!(fast.steps.len(), 1);

        let examples = workflow
            .jobs
            .iter()
            .find(|j| j.job_id == "example-checks")
            .unwrap();
        assert_eq!(examples.needs, vec!["fast-checks"]);

        let matrix_jobs: Vec<_> = workflow
            .jobs
            .iter()
            .filter(|j| j.job_id.starts_with("checks"))
            .collect();
        assert_eq!(matrix_jobs.len(), 2);
        for mj in &matrix_jobs {
            assert_eq!(mj.needs, vec!["fast-checks"]);
        }
    }
}

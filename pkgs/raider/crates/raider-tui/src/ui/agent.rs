use ratatui::style::Color;

use crate::app::builtin::Agent;

use super::theme::Theme;

pub(crate) fn agent_palette(theme: &Theme) -> [Color; 7] {
    [
        theme.secondary,
        theme.accent,
        theme.success,
        theme.warning,
        theme.primary,
        theme.error,
        theme.info,
    ]
}

pub(crate) fn agent_color_by_index(theme: &Theme, index: usize) -> Color {
    let palette = agent_palette(theme);
    palette[index % palette.len()]
}

fn opencode_palette_index_for(agent_name: &str, agents: &[Agent]) -> usize {
    const NATIVE_SUBAGENTS: &[&str] = &["explore", "general"];
    const DEFAULT_AGENT: &str = "build";

    let mut names: Vec<String> = agents.iter().map(|a| a.name.clone()).collect();
    for sub in NATIVE_SUBAGENTS {
        if !names.iter().any(|n| n == sub) {
            names.push((*sub).to_string());
        }
    }
    names.sort_by(|a, b| {
        let a_default = (a == DEFAULT_AGENT) as u8;
        let b_default = (b == DEFAULT_AGENT) as u8;
        b_default.cmp(&a_default).then_with(|| a.cmp(b))
    });
    names.iter().position(|n| n == agent_name).unwrap_or(0)
}

pub(crate) fn agent_color(theme: &Theme, agents: &[Agent], agent_name: &str) -> Color {
    let idx = opencode_palette_index_for(agent_name, agents);
    agent_color_by_index(theme, idx)
}

pub(crate) fn titlecase(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut capitalize_next = true;
    for c in s.chars() {
        if matches!(c, '-' | '_' | ' ') {
            out.push(c);
            capitalize_next = true;
        } else if capitalize_next {
            for up in c.to_uppercase() {
                out.push(up);
            }
            capitalize_next = false;
        } else {
            out.push(c);
        }
    }
    out
}

pub(crate) fn resolve_model_display(
    catalog: &crate::provider::ModelCatalog,
    provider_id: Option<&str>,
    model_id: &str,
) -> Option<String> {
    if let Some(pid) = provider_id {
        if let Some(provider) = catalog.find_provider(pid) {
            if let Some(model) = provider.find_model(model_id) {
                let name = model.display_name();
                if !name.is_empty() && name != model_id {
                    return Some(name.to_string());
                }
                return None;
            }
        }
    }

    let mut stub_hit: Option<String> = None;
    for provider in &catalog.providers {
        if let Some(model) = provider.find_model(model_id) {
            let name = model.display_name();
            if name.is_empty() {
                continue;
            }
            if name == model_id {
                if stub_hit.is_none() {
                    stub_hit = Some(name.to_string());
                }
                continue;
            }
            return Some(name.to_string());
        }
    }
    stub_hit
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::theme::{ThemeName, ThemeRegistry};

    #[test]
    fn build_and_plan_match_opencode_default_install() {
        let reg = ThemeRegistry::new();
        let name: ThemeName = reg.lookup("dracula").expect("dracula bundled");
        let theme = reg.get(&name);
        let agents = vec![Agent::new("build", "Build"), Agent::new("plan", "Plan")];

        let build = agent_color(&theme, &agents, "build");
        let plan = agent_color(&theme, &agents, "plan");

        assert_eq!(build, theme.secondary, "build → theme.secondary (pink)");
        assert_eq!(
            plan, theme.warning,
            "plan → theme.warning (yellow), not accent — explore+general \
             subagents push it to palette idx 3",
        );
        assert_ne!(
            plan, theme.accent,
            "regression guard: plan must not collide with index 1 \
             (would require ignoring subagents)",
        );
    }

    #[test]
    fn opencode_palette_index_respects_default_first_then_alphabetical() {
        let agents = vec![Agent::new("build", "Build"), Agent::new("plan", "Plan")];
        assert_eq!(opencode_palette_index_for("build", &agents), 0);
        assert_eq!(opencode_palette_index_for("explore", &agents), 1);
        assert_eq!(opencode_palette_index_for("general", &agents), 2);
        assert_eq!(opencode_palette_index_for("plan", &agents), 3);
    }

    #[test]
    fn unknown_agent_falls_back_to_first_palette_slot() {
        let reg = ThemeRegistry::new();
        let name: ThemeName = reg.lookup("dracula").expect("dracula bundled");
        let theme = reg.get(&name);
        let agents = vec![Agent::new("build", "Build")];
        assert_eq!(agent_color(&theme, &agents, "nope"), theme.secondary);
    }
}

use ratatui::style::Color;

use crate::app::builtin::Agents;

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

pub(crate) fn agent_color(theme: &Theme, agents: &Agents, agent_name: &str) -> Color {
    let idx = agents.palette_index_of(agent_name);
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
    use crate::app::builtin::Agent;
    use crate::ui::theme::{ThemeName, ThemeRegistry};

    fn make_agents(list: Vec<Agent>) -> Agents {
        Agents::try_from_vec(list).expect("non-empty")
    }

    #[test]
    fn build_and_plan_match_opencode_default_install() {
        let reg = ThemeRegistry::new();
        let name: ThemeName = reg.lookup("dracula").expect("dracula bundled");
        let theme = reg.get(&name);
        let agents = make_agents(vec![
            Agent::new("build", "Build"),
            Agent::new("plan", "Plan"),
        ]);

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
    fn palette_index_respects_default_first_then_alphabetical() {
        let agents = make_agents(vec![
            Agent::new("build", "Build"),
            Agent::new("plan", "Plan"),
        ]);
        assert_eq!(agents.palette_index_of("build"), 0);
        assert_eq!(agents.palette_index_of("explore"), 1);
        assert_eq!(agents.palette_index_of("general"), 2);
        assert_eq!(agents.palette_index_of("plan"), 3);
    }

    #[test]
    fn unknown_agent_falls_back_to_first_palette_slot() {
        let reg = ThemeRegistry::new();
        let name: ThemeName = reg.lookup("dracula").expect("dracula bundled");
        let theme = reg.get(&name);
        let agents = make_agents(vec![Agent::new("build", "Build")]);
        assert_eq!(agent_color(&theme, &agents, "nope"), theme.secondary);
    }

    #[test]
    fn palette_version_bumps_on_replace() {
        let mut agents = make_agents(vec![Agent::new("build", "Build")]);
        let v0 = agents.palette_version();
        agents
            .try_replace(vec![Agent::new("build", "Build"), Agent::new("plan", "P")])
            .unwrap();
        assert!(agents.palette_version() > v0);
    }
}

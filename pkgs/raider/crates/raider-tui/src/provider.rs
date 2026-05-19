#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ModelRef {
    pub provider_id: String,
    pub model_id: String,
}

impl ModelRef {
    pub fn new(provider_id: impl Into<String>, model_id: impl Into<String>) -> Self {
        Self {
            provider_id: provider_id.into(),
            model_id: model_id.into(),
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        let (provider, model) = s.split_once('/')?;
        if provider.is_empty() || model.is_empty() {
            return None;
        }
        Some(Self::new(provider, model))
    }

    pub fn wire(&self) -> String {
        format!("{}/{}", self.provider_id, self.model_id)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModelInfo {
    pub id: String,
    pub name: Option<String>,
    pub variants: Vec<String>,
    pub context_limit: u64,
}

impl ModelInfo {
    pub fn display_name(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.id)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProviderInfo {
    pub id: String,
    pub name: Option<String>,
    pub models: Vec<ModelInfo>,
}

impl ProviderInfo {
    pub fn display_name(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.id)
    }

    pub fn find_model(&self, model_id: &str) -> Option<&ModelInfo> {
        self.models.iter().find(|m| m.id == model_id)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModelCatalog {
    pub providers: Vec<ProviderInfo>,
}

impl ModelCatalog {
    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }

    pub fn find_provider(&self, provider_id: &str) -> Option<&ProviderInfo> {
        self.providers.iter().find(|p| p.id == provider_id)
    }

    pub fn find(&self, m: &ModelRef) -> Option<(&ProviderInfo, &ModelInfo)> {
        let provider = self.find_provider(&m.provider_id)?;
        let model = provider.find_model(&m.model_id)?;
        Some((provider, model))
    }

    pub fn has(&self, m: &ModelRef) -> bool {
        self.find(m).is_some()
    }

    pub fn all_refs(&self) -> Vec<ModelRef> {
        let mut out = Vec::new();
        for p in &self.providers {
            for m in &p.models {
                out.push(ModelRef::new(p.id.clone(), m.id.clone()));
            }
        }
        out
    }
}

pub fn pretty_label(
    catalog: &ModelCatalog,
    model: Option<&ModelRef>,
    variant: Option<&str>,
) -> Option<String> {
    let m = model?;
    let (provider_name, model_name) = match catalog.find(m) {
        Some((p, mi)) => (p.display_name().to_string(), mi.display_name().to_string()),
        None => (m.provider_id.clone(), m.model_id.clone()),
    };
    let mut out = model_name;
    if let Some(v) = variant {
        out.push('/');
        out.push_str(v);
    }
    out.push(' ');
    out.push_str(&provider_name);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_round_trips() {
        let m = ModelRef::parse("anthropic/claude-sonnet-4-5").unwrap();
        assert_eq!(m.provider_id, "anthropic");
        assert_eq!(m.model_id, "claude-sonnet-4-5");
        assert_eq!(m.wire(), "anthropic/claude-sonnet-4-5");
    }

    #[test]
    fn parse_keeps_slashes_in_model_id() {
        let m = ModelRef::parse("openrouter/openai/gpt-5").unwrap();
        assert_eq!(m.provider_id, "openrouter");
        assert_eq!(m.model_id, "openai/gpt-5");
    }

    #[test]
    fn parse_rejects_empty_halves() {
        assert!(ModelRef::parse("anthropic/").is_none());
        assert!(ModelRef::parse("/claude").is_none());
        assert!(ModelRef::parse("no-slash").is_none());
    }

    #[test]
    fn catalog_lookup() {
        let cat = ModelCatalog {
            providers: vec![ProviderInfo {
                id: "anthropic".into(),
                name: Some("Anthropic".into()),
                models: vec![ModelInfo {
                    id: "claude-sonnet-4-5".into(),
                    name: Some("Claude Sonnet 4.5".into()),
                    variants: vec!["thinking".into()],
                    context_limit: 0,
                }],
            }],
        };
        let m = ModelRef::new("anthropic", "claude-sonnet-4-5");
        let (p, mi) = cat.find(&m).unwrap();
        assert_eq!(p.display_name(), "Anthropic");
        assert_eq!(mi.display_name(), "Claude Sonnet 4.5");
        assert_eq!(mi.variants, vec!["thinking".to_string()]);

        let label = pretty_label(&cat, Some(&m), Some("thinking")).unwrap();
        assert_eq!(label, "Claude Sonnet 4.5/thinking Anthropic");
    }
}

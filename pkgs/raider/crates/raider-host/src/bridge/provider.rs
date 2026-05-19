use raider_opencode::types::provider::ProviderList;
use raider_tui::{
    Action, HostAction, ModelCatalog, ModelInfo as TuiModelInfo, ModelRef,
    ProviderInfo as TuiProviderInfo,
};

pub fn provider_refresh_actions(
    list: &ProviderList,
    current_model: Option<&ModelRef>,
) -> Vec<Action> {
    let mut providers: Vec<&raider_opencode::types::provider::ProviderInfo> =
        list.all.iter().collect();
    providers.sort_by(|a, b| {
        let a_is_opencode = a.id == "opencode";
        let b_is_opencode = b.id == "opencode";
        b_is_opencode.cmp(&a_is_opencode).then_with(|| {
            a.name
                .to_ascii_lowercase()
                .cmp(&b.name.to_ascii_lowercase())
        })
    });

    let catalog_providers: Vec<TuiProviderInfo> = providers
        .iter()
        .map(|p| {
            let mut models: Vec<&raider_opencode::types::provider::ModelInfo> =
                p.models.values().collect();
            models.retain(|m| m.status.as_deref() != Some("deprecated"));
            models.sort_by(|a, b| {
                a.name
                    .to_ascii_lowercase()
                    .cmp(&b.name.to_ascii_lowercase())
                    .then_with(|| a.id.cmp(&b.id))
            });
            TuiProviderInfo {
                id: p.id.clone(),
                name: if p.name.is_empty() {
                    None
                } else {
                    Some(p.name.clone())
                },
                models: models
                    .into_iter()
                    .map(|m| TuiModelInfo {
                        id: m.id.clone(),
                        name: if m.name.is_empty() {
                            None
                        } else {
                            Some(m.name.clone())
                        },
                        variants: {
                            let mut v: Vec<String> = m.variants.keys().cloned().collect();
                            v.sort();
                            v
                        },
                        context_limit: m.limit.as_ref().map(|l| l.context).unwrap_or(0),
                    })
                    .collect(),
            }
        })
        .collect();

    let catalog = ModelCatalog {
        providers: catalog_providers,
    };

    tracing::debug!(
        provider_count = catalog.providers.len(),
        "provider catalog built",
    );

    let mut out = vec![Action::Host(HostAction::SetCatalog(catalog.clone()))];

    let still_valid = current_model.map(|m| catalog.has(m)).unwrap_or(false);
    if !still_valid {
        let picked = pick_default_model(list, &catalog);
        out.push(Action::Host(HostAction::SetCurrentModel(picked)));
    }

    out
}

pub(super) fn pick_default_model(list: &ProviderList, catalog: &ModelCatalog) -> Option<ModelRef> {
    let provider = catalog.providers.iter().find(|p| !p.models.is_empty())?;
    if let Some(default_model_id) = list.default.get(&provider.id) {
        if provider.models.iter().any(|m| &m.id == default_model_id) {
            return Some(ModelRef::new(provider.id.clone(), default_model_id.clone()));
        }
    }
    let first = provider.models.first()?;
    Some(ModelRef::new(provider.id.clone(), first.id.clone()))
}
